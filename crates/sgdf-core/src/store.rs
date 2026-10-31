use crate::domain::{
    AdherentDetails, GoogleContactEntry, ImportSummary, ParsedRow, RepresentantLegalSummary,
    UnitAdherentsResponse, UnitSummary, Unite, WhatsappAuditMissing, WhatsappAuditResponse,
};
use chrono::{Datelike, NaiveDate};
use sqlx::{Row, SqlitePool};
use std::collections::{BTreeSet, HashMap};

fn deduce_saison(
    start: Option<NaiveDate>,
    end: Option<NaiveDate>,
) -> (String, NaiveDate, NaiveDate) {
    if let (Some(s), Some(e)) = (start, end) {
        let year_start = if s.month() >= 9 {
            s.year()
        } else {
            s.year() - 1
        };
        let libelle = format!("{}-{}", year_start, year_start + 1);
        (libelle, s, e)
    } else {
        let d_start = NaiveDate::from_ymd_opt(2026, 9, 1).unwrap();
        let d_end = NaiveDate::from_ymd_opt(2027, 8, 31).unwrap();
        ("2026-2027".to_string(), d_start, d_end)
    }
}

pub struct IndividuInput<'a> {
    pub code_adherent: Option<&'a str>,
    pub civilite: Option<&'a str>,
    pub nom: &'a str,
    pub prenom: &'a str,
    pub date_naissance: Option<NaiveDate>,
    pub telephone_mobile: Option<&'a str>,
    pub telephone_domicile: Option<&'a str>,
    pub courriel_personnel: Option<&'a str>,
    pub courriel_sgdf: Option<&'a str>,
    pub adresse_ligne1: Option<&'a str>,
    pub adresse_ligne2: Option<&'a str>,
    pub adresse_code_postal: Option<&'a str>,
    pub adresse_ville: Option<&'a str>,
    pub autorisation_soins_urgence: bool,
    pub droit_image_interne: bool,
    pub droit_image_externe: bool,
}

struct CandidateIndividual {
    id: i64,
    telephone_mobile: Option<String>,
    courriel_personnel: Option<String>,
}

pub async fn import_rows(
    pool: &SqlitePool,
    rows: &[ParsedRow],
) -> Result<ImportSummary, sqlx::Error> {
    let mut tx = pool.begin().await?;

    // 1. Saison
    let first_row = rows.first();
    let (saison_libelle, s_debut, s_fin) = deduce_saison(
        first_row.and_then(|r| r.date_debut_adhesion),
        first_row.and_then(|r| r.date_fin_adhesion),
    );

    let saison_id: i64 = sqlx::query(
        "INSERT INTO saisons (libelle, date_debut, date_fin, active)
         VALUES (?, ?, ?, 1)
         ON CONFLICT(libelle) DO UPDATE SET date_debut = excluded.date_debut, date_fin = excluded.date_fin
         RETURNING id",
    )
    .bind(&saison_libelle)
    .bind(s_debut.to_string())
    .bind(s_fin.to_string())
    .fetch_one(&mut *tx)
    .await?
    .get("id");

    let mut inscriptions_count = 0;
    let mut liens_count = 0;

    for row in rows {
        // 2. Unité
        let unite_id: i64 = sqlx::query(
            "INSERT INTO unites (code_structure, nom)
             VALUES (?, ?)
             ON CONFLICT(code_structure) DO UPDATE SET nom = excluded.nom
             RETURNING id",
        )
        .bind(&row.structure_code)
        .bind(&row.structure_nom)
        .fetch_one(&mut *tx)
        .await?
        .get("id");

        // 3. Individu principal (Adhérent jeune ou adulte bénévole)
        let main_input = IndividuInput {
            code_adherent: row.code_adherent.as_deref(),
            civilite: row.civilite.as_deref(),
            nom: &row.nom,
            prenom: &row.prenom,
            date_naissance: row.date_naissance,
            telephone_mobile: row.telephone_mobile.as_deref(),
            telephone_domicile: row.telephone_domicile.as_deref(),
            courriel_personnel: row.courriel_personnel.as_deref(),
            courriel_sgdf: row.courriel_sgdf.as_deref(),
            adresse_ligne1: row.adresse_ligne1.as_deref(),
            adresse_ligne2: row.adresse_ligne2.as_deref(),
            adresse_code_postal: row.adresse_code_postal.as_deref(),
            adresse_ville: row.adresse_ville.as_deref(),
            autorisation_soins_urgence: row.autorisation_soins,
            droit_image_interne: row.droit_image_interne,
            droit_image_externe: row.droit_image_externe,
        };

        let individu_id = upsert_individu(&mut tx, &main_input).await?;

        // 4. Inscription
        let date_deb_str = row.date_debut_adhesion.map(|d| d.to_string());
        let date_fin_str = row.date_fin_adhesion.map(|d| d.to_string());
        let statut = row.statut_adhesion.as_deref().unwrap_or("Inscrit(e)");

        sqlx::query(
            "INSERT INTO inscriptions (individu_id, unite_id, saison_id, fonction_code, fonction_libelle, statut, active, date_debut, date_fin)
             VALUES (?, ?, ?, ?, ?, ?, 1, ?, ?)
             ON CONFLICT(individu_id, unite_id, saison_id) DO UPDATE SET
                fonction_code = excluded.fonction_code,
                fonction_libelle = excluded.fonction_libelle,
                statut = excluded.statut,
                active = 1,
                date_debut = excluded.date_debut,
                date_fin = excluded.date_fin",
        )
        .bind(individu_id)
        .bind(unite_id)
        .bind(saison_id)
        .bind(row.fonction_code.as_deref())
        .bind(row.fonction_nom.as_deref())
        .bind(statut)
        .bind(date_deb_str)
        .bind(date_fin_str)
        .execute(&mut *tx)
        .await?;

        inscriptions_count += 1;

        // 5. Représentants légaux & liens de parenté
        for (idx, parent) in row.representants_legaux.iter().enumerate() {
            let parent_input = IndividuInput {
                code_adherent: parent.code_adherent.as_deref(),
                civilite: parent.civilite.as_deref(),
                nom: &parent.nom,
                prenom: &parent.prenom,
                date_naissance: None,
                telephone_mobile: parent.telephone_mobile.as_deref(),
                telephone_domicile: parent.telephone_domicile.as_deref(),
                courriel_personnel: parent.courriel_personnel.as_deref(),
                courriel_sgdf: parent.courriel_sgdf.as_deref(),
                adresse_ligne1: parent.adresse_ligne1.as_deref(),
                adresse_ligne2: None,
                adresse_code_postal: parent.adresse_code_postal.as_deref(),
                adresse_ville: parent.adresse_ville.as_deref(),
                autorisation_soins_urgence: false,
                droit_image_interne: false,
                droit_image_externe: false,
            };

            let parent_id = upsert_individu(&mut tx, &parent_input).await?;

            sqlx::query(
                "INSERT INTO liens_parente (parent_id, jeune_id, saison_id, ordre)
                 VALUES (?, ?, ?, ?)
                 ON CONFLICT(parent_id, jeune_id, saison_id) DO UPDATE SET ordre = excluded.ordre",
            )
            .bind(parent_id)
            .bind(individu_id)
            .bind(saison_id)
            .bind((idx + 1) as i64)
            .execute(&mut *tx)
            .await?;

            liens_count += 1;
        }
    }

    let total_individus: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM individus")
        .fetch_one(&mut *tx)
        .await?;

    let total_unites: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM unites")
        .fetch_one(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(ImportSummary {
        individus_importes: total_individus,
        unites_importees: total_unites,
        inscriptions_importees: inscriptions_count,
        liens_parente_importes: liens_count,
    })
}

async fn upsert_individu(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    input: &IndividuInput<'_>,
) -> Result<i64, sqlx::Error> {
    let nom_clean = input.nom.trim().to_uppercase();
    let prenom_clean = input.prenom.trim().to_uppercase();
    let date_naissance_str = input.date_naissance.map(|d| d.to_string());

    // 1. Recherche par code adhérent si renseigné
    if let Some(code) = input.code_adherent.filter(|c| !c.trim().is_empty()) {
        let existing: Option<(i64,)> =
            sqlx::query_as("SELECT id FROM individus WHERE code_adherent = ?")
                .bind(code)
                .fetch_optional(&mut **tx)
                .await?;

        if let Some((id,)) = existing {
            sqlx::query(
                "UPDATE individus SET
                    civilite = COALESCE(?, civilite),
                    nom = ?,
                    prenom = ?,
                    date_naissance = COALESCE(?, date_naissance),
                    telephone_mobile = COALESCE(?, telephone_mobile),
                    telephone_domicile = COALESCE(?, telephone_domicile),
                    courriel_personnel = COALESCE(?, courriel_personnel),
                    courriel_sgdf = COALESCE(?, courriel_sgdf),
                    adresse_ligne1 = COALESCE(?, adresse_ligne1),
                    adresse_ligne2 = COALESCE(?, adresse_ligne2),
                    adresse_code_postal = COALESCE(?, adresse_code_postal),
                    adresse_ville = COALESCE(?, adresse_ville),
                    autorisation_soins_urgence = MAX(?, autorisation_soins_urgence),
                    droit_image_interne = MAX(?, droit_image_interne),
                    droit_image_externe = MAX(?, droit_image_externe),
                    updated_at = CURRENT_TIMESTAMP
                 WHERE id = ?",
            )
            .bind(input.civilite)
            .bind(&nom_clean)
            .bind(&prenom_clean)
            .bind(date_naissance_str.as_deref())
            .bind(input.telephone_mobile)
            .bind(input.telephone_domicile)
            .bind(input.courriel_personnel)
            .bind(input.courriel_sgdf)
            .bind(input.adresse_ligne1)
            .bind(input.adresse_ligne2)
            .bind(input.adresse_code_postal)
            .bind(input.adresse_ville)
            .bind(input.autorisation_soins_urgence as i32)
            .bind(input.droit_image_interne as i32)
            .bind(input.droit_image_externe as i32)
            .bind(id)
            .execute(&mut **tx)
            .await?;

            return Ok(id);
        }
    }

    // 2. Recherche par Nom + Prénom (déduplication pour les représentants légaux ou ré-identification)
    let rows = sqlx::query(
        "SELECT id, telephone_mobile, courriel_personnel
         FROM individus
         WHERE UPPER(nom) = ? AND UPPER(prenom) = ?",
    )
    .bind(&nom_clean)
    .bind(&prenom_clean)
    .fetch_all(&mut **tx)
    .await?;

    let candidates: Vec<CandidateIndividual> = rows
        .into_iter()
        .map(|r| CandidateIndividual {
            id: r.get("id"),
            telephone_mobile: r.get("telephone_mobile"),
            courriel_personnel: r.get("courriel_personnel"),
        })
        .collect();

    if !candidates.is_empty() {
        let matched = candidates.iter().find(|cand| {
            let phone_matches = input.telephone_mobile.is_some()
                && input.telephone_mobile == cand.telephone_mobile.as_deref();
            let email_matches = input.courriel_personnel.is_some()
                && input.courriel_personnel == cand.courriel_personnel.as_deref();
            let candidate_has_no_contact =
                cand.telephone_mobile.is_none() && cand.courriel_personnel.is_none();

            phone_matches || email_matches || (candidate_has_no_contact && candidates.len() == 1)
        });

        if let Some(cand) = matched {
            let matched_id = cand.id;
            sqlx::query(
                "UPDATE individus SET
                    code_adherent = COALESCE(?, code_adherent),
                    civilite = COALESCE(?, civilite),
                    date_naissance = COALESCE(?, date_naissance),
                    telephone_mobile = COALESCE(?, telephone_mobile),
                    telephone_domicile = COALESCE(?, telephone_domicile),
                    courriel_personnel = COALESCE(?, courriel_personnel),
                    courriel_sgdf = COALESCE(?, courriel_sgdf),
                    adresse_ligne1 = COALESCE(?, adresse_ligne1),
                    adresse_ligne2 = COALESCE(?, adresse_ligne2),
                    adresse_code_postal = COALESCE(?, adresse_code_postal),
                    adresse_ville = COALESCE(?, adresse_ville),
                    autorisation_soins_urgence = MAX(?, autorisation_soins_urgence),
                    droit_image_interne = MAX(?, droit_image_interne),
                    droit_image_externe = MAX(?, droit_image_externe),
                    updated_at = CURRENT_TIMESTAMP
                 WHERE id = ?",
            )
            .bind(input.code_adherent)
            .bind(input.civilite)
            .bind(date_naissance_str.as_deref())
            .bind(input.telephone_mobile)
            .bind(input.telephone_domicile)
            .bind(input.courriel_personnel)
            .bind(input.courriel_sgdf)
            .bind(input.adresse_ligne1)
            .bind(input.adresse_ligne2)
            .bind(input.adresse_code_postal)
            .bind(input.adresse_ville)
            .bind(input.autorisation_soins_urgence as i32)
            .bind(input.droit_image_interne as i32)
            .bind(input.droit_image_externe as i32)
            .bind(matched_id)
            .execute(&mut **tx)
            .await?;

            return Ok(matched_id);
        }
    }

    // 3. Nouvel individu
    let new_id: i64 = sqlx::query(
        "INSERT INTO individus (
            code_adherent, civilite, nom, prenom, date_naissance,
            telephone_mobile, telephone_domicile, courriel_personnel, courriel_sgdf,
            adresse_ligne1, adresse_ligne2, adresse_code_postal, adresse_ville,
            autorisation_soins_urgence, droit_image_interne, droit_image_externe
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        RETURNING id",
    )
    .bind(input.code_adherent)
    .bind(input.civilite)
    .bind(&nom_clean)
    .bind(&prenom_clean)
    .bind(date_naissance_str.as_deref())
    .bind(input.telephone_mobile)
    .bind(input.telephone_domicile)
    .bind(input.courriel_personnel)
    .bind(input.courriel_sgdf)
    .bind(input.adresse_ligne1)
    .bind(input.adresse_ligne2)
    .bind(input.adresse_code_postal)
    .bind(input.adresse_ville)
    .bind(input.autorisation_soins_urgence as i32)
    .bind(input.droit_image_interne as i32)
    .bind(input.droit_image_externe as i32)
    .fetch_one(&mut **tx)
    .await?
    .get("id");

    Ok(new_id)
}

pub async fn get_units(pool: &SqlitePool) -> Result<Vec<UnitSummary>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT u.id, u.code_structure, u.nom, u.whatsapp_group_jid,
                COUNT(i.id) as adherents_count
         FROM unites u
         LEFT JOIN inscriptions i ON i.unite_id = u.id AND i.active = 1
              AND i.saison_id = (SELECT id FROM saisons WHERE active = 1 ORDER BY date_debut DESC LIMIT 1)
         GROUP BY u.id
         ORDER BY u.nom",
    )
    .fetch_all(pool)
    .await?;

    let units = rows
        .into_iter()
        .map(|r| UnitSummary {
            id: r.get("id"),
            code_structure: r.get("code_structure"),
            nom: r.get("nom"),
            whatsapp_group_jid: r.get("whatsapp_group_jid"),
            adherents_count: r.get("adherents_count"),
        })
        .collect();

    Ok(units)
}

pub async fn update_unit_jid(
    pool: &SqlitePool,
    unit_id: i64,
    jid: Option<&str>,
) -> Result<Option<Unite>, sqlx::Error> {
    let row = sqlx::query(
        "UPDATE unites
         SET whatsapp_group_jid = ?
         WHERE id = ?
         RETURNING id, code_structure, nom, whatsapp_group_jid",
    )
    .bind(jid)
    .bind(unit_id)
    .fetch_optional(pool)
    .await?;

    let unit = row.map(|r| Unite {
        id: Some(r.get("id")),
        code_structure: r.get("code_structure"),
        nom: r.get("nom"),
        whatsapp_group_jid: r.get("whatsapp_group_jid"),
    });

    Ok(unit)
}

pub async fn get_unit_adherents(
    pool: &SqlitePool,
    unit_id: i64,
) -> Result<Option<UnitAdherentsResponse>, sqlx::Error> {
    let unit_row = sqlx::query(
        "SELECT id, code_structure, nom, whatsapp_group_jid
         FROM unites
         WHERE id = ?",
    )
    .bind(unit_id)
    .fetch_optional(pool)
    .await?;

    let unit = match unit_row {
        Some(r) => Unite {
            id: Some(r.get("id")),
            code_structure: r.get("code_structure"),
            nom: r.get("nom"),
            whatsapp_group_jid: r.get("whatsapp_group_jid"),
        },
        None => return Ok(None),
    };

    let saison_row = sqlx::query(
        "SELECT id, libelle FROM saisons WHERE active = 1 ORDER BY date_debut DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;

    let (saison_id, saison_libelle) = match saison_row {
        Some(r) => (r.get::<i64, _>("id"), r.get::<String, _>("libelle")),
        None => {
            return Ok(Some(UnitAdherentsResponse {
                unit,
                saison: "N/A".to_string(),
                adherents: vec![],
            }))
        }
    };

    let member_rows = sqlx::query(
        "SELECT ind.id, ind.nom, ind.prenom, ind.code_adherent,
                insc.fonction_libelle, ind.date_naissance,
                ind.telephone_mobile, ind.courriel_personnel
         FROM inscriptions insc
         JOIN individus ind ON ind.id = insc.individu_id
         WHERE insc.unite_id = ? AND insc.saison_id = ? AND insc.active = 1
         ORDER BY ind.nom, ind.prenom",
    )
    .bind(unit_id)
    .bind(saison_id)
    .fetch_all(pool)
    .await?;

    let mut adherents = Vec::new();

    for m in member_rows {
        let adherent_id: i64 = m.get("id");
        let date_naissance_raw: Option<String> = m.get("date_naissance");
        let date_naissance =
            date_naissance_raw.and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok());

        let rep_rows = sqlx::query(
            "SELECT p.id, p.nom, p.prenom, p.telephone_mobile, p.courriel_personnel
             FROM liens_parente lp
             JOIN individus p ON p.id = lp.parent_id
             WHERE lp.jeune_id = ? AND lp.saison_id = ?
             ORDER BY lp.ordre",
        )
        .bind(adherent_id)
        .bind(saison_id)
        .fetch_all(pool)
        .await?;

        let representants_legaux = rep_rows
            .into_iter()
            .map(|r| RepresentantLegalSummary {
                id: r.get("id"),
                nom: r.get("nom"),
                prenom: r.get("prenom"),
                telephone_mobile: r.get("telephone_mobile"),
                courriel: r.get("courriel_personnel"),
            })
            .collect();

        adherents.push(AdherentDetails {
            id: adherent_id,
            nom: m.get("nom"),
            prenom: m.get("prenom"),
            code_adherent: m.get("code_adherent"),
            fonction: m.get("fonction_libelle"),
            date_naissance,
            telephone_mobile: m.get("telephone_mobile"),
            courriel: m.get("courriel_personnel"),
            representants_legaux,
        });
    }

    Ok(Some(UnitAdherentsResponse {
        unit,
        saison: saison_libelle,
        adherents,
    }))
}

struct ContactBuilder {
    nom: String,
    prenom: String,
    telephone_mobile: Option<String>,
    telephone_domicile: Option<String>,
    courriel_personnel: Option<String>,
    courriel_sgdf: Option<String>,
    filiations: Vec<String>,
    fonctions: Vec<String>,
    unit_labels: BTreeSet<String>,
}

pub async fn get_google_contacts_export(
    pool: &SqlitePool,
) -> Result<Vec<GoogleContactEntry>, sqlx::Error> {
    let saison_row =
        sqlx::query("SELECT id FROM saisons WHERE active = 1 ORDER BY date_debut DESC LIMIT 1")
            .fetch_optional(pool)
            .await?;

    let saison_id: i64 = match saison_row {
        Some(r) => r.get("id"),
        None => return Ok(vec![]),
    };

    let mut builders: HashMap<i64, ContactBuilder> = HashMap::new();

    // 1. Représentants légaux des jeunes inscrits dans la saison active
    let parent_rows = sqlx::query(
        "SELECT p.id as parent_id, p.nom as parent_nom, p.prenom as parent_prenom,
                p.telephone_mobile as parent_mob, p.telephone_domicile as parent_dom,
                p.courriel_personnel as parent_email, p.courriel_sgdf as parent_sgdf,
                j.prenom as jeune_prenom,
                u.nom as unite_nom
         FROM liens_parente lp
         JOIN individus p ON p.id = lp.parent_id
         JOIN individus j ON j.id = lp.jeune_id
         JOIN inscriptions insc ON insc.individu_id = j.id AND insc.saison_id = lp.saison_id AND insc.active = 1
         JOIN unites u ON u.id = insc.unite_id
         WHERE lp.saison_id = ?
         ORDER BY p.nom, p.prenom, j.prenom",
    )
    .bind(saison_id)
    .fetch_all(pool)
    .await?;

    for r in parent_rows {
        let parent_id: i64 = r.get("parent_id");
        let parent_nom: String = r.get("parent_nom");
        let parent_prenom: String = r.get("parent_prenom");
        let parent_mob: Option<String> = r.get("parent_mob");
        let parent_dom: Option<String> = r.get("parent_dom");
        let parent_email: Option<String> = r.get("parent_email");
        let parent_sgdf: Option<String> = r.get("parent_sgdf");
        let jeune_prenom: String = r.get("jeune_prenom");
        let unite_nom: String = r.get("unite_nom");

        let entry = builders.entry(parent_id).or_insert_with(|| ContactBuilder {
            nom: parent_nom,
            prenom: parent_prenom,
            telephone_mobile: parent_mob,
            telephone_domicile: parent_dom,
            courriel_personnel: parent_email,
            courriel_sgdf: parent_sgdf,
            filiations: Vec::new(),
            fonctions: Vec::new(),
            unit_labels: BTreeSet::new(),
        });

        let filiation = format!("{} ({})", jeune_prenom, unite_nom);
        if !entry.filiations.contains(&filiation) {
            entry.filiations.push(filiation);
        }
        entry.unit_labels.insert(format!("SGDF - {}", unite_nom));
    }

    // 2. Adultes, cadres, chefs et compagnons (code fonction >= 200, Impeesa 181, ou Compagnon 140)
    let adult_rows = sqlx::query(
        "SELECT ind.id, ind.nom, ind.prenom,
                ind.telephone_mobile, ind.telephone_domicile,
                ind.courriel_personnel, ind.courriel_sgdf,
                insc.fonction_libelle, insc.fonction_code, u.nom as unite_nom
         FROM inscriptions insc
         JOIN individus ind ON ind.id = insc.individu_id
         JOIN unites u ON u.id = insc.unite_id
         WHERE insc.saison_id = ? AND insc.active = 1
           AND (
                CAST(insc.fonction_code AS INTEGER) >= 200
                OR insc.fonction_code IN ('140', '181')
           )",
    )
    .bind(saison_id)
    .fetch_all(pool)
    .await?;

    for r in adult_rows {
        let ind_id: i64 = r.get("id");
        let nom: String = r.get("nom");
        let prenom: String = r.get("prenom");
        let mob: Option<String> = r.get("telephone_mobile");
        let dom: Option<String> = r.get("telephone_domicile");
        let email: Option<String> = r.get("courriel_personnel");
        let email_sgdf: Option<String> = r.get("courriel_sgdf");
        let fonction: Option<String> = r.get("fonction_libelle");
        let unite_nom: String = r.get("unite_nom");

        let entry = builders.entry(ind_id).or_insert_with(|| ContactBuilder {
            nom: nom.clone(),
            prenom: prenom.clone(),
            telephone_mobile: mob.clone(),
            telephone_domicile: dom.clone(),
            courriel_personnel: email.clone(),
            courriel_sgdf: email_sgdf.clone(),
            filiations: Vec::new(),
            fonctions: Vec::new(),
            unit_labels: BTreeSet::new(),
        });

        if entry.telephone_mobile.is_none() {
            entry.telephone_mobile = mob;
        }
        if entry.telephone_domicile.is_none() {
            entry.telephone_domicile = dom;
        }
        if entry.courriel_personnel.is_none() {
            entry.courriel_personnel = email;
        }
        if entry.courriel_sgdf.is_none() {
            entry.courriel_sgdf = email_sgdf;
        }

        if let Some(f) = fonction {
            let desc = format!("{} ({})", f, unite_nom);
            if !entry.fonctions.contains(&desc) {
                entry.fonctions.push(desc);
            }
        }
        entry.unit_labels.insert(format!("SGDF - {}", unite_nom));
    }

    // 3. Transformation en liste ordonnée finale
    let mut contacts: Vec<GoogleContactEntry> = builders
        .into_values()
        .map(|b| {
            let mut notes_parts = Vec::new();
            if !b.filiations.is_empty() {
                notes_parts.push(format!("Parent de : {}", b.filiations.join(", ")));
            }
            if !b.fonctions.is_empty() {
                notes_parts.push(format!("Fonction : {}", b.fonctions.join(", ")));
            }
            let notes = if notes_parts.is_empty() {
                None
            } else {
                Some(notes_parts.join(" | "))
            };

            let mut labels = vec!["SGDF".to_string()];
            labels.extend(b.unit_labels);

            GoogleContactEntry {
                nom: b.nom,
                prenom: b.prenom,
                telephone_mobile: b.telephone_mobile,
                telephone_domicile: b.telephone_domicile,
                courriel_personnel: b.courriel_personnel,
                courriel_sgdf: b.courriel_sgdf,
                notes,
                labels,
            }
        })
        .collect();

    contacts.sort_by(|a, b| a.nom.cmp(&b.nom).then_with(|| a.prenom.cmp(&b.prenom)));

    Ok(contacts)
}

pub async fn audit_unit_whatsapp(
    pool: &SqlitePool,
    unit_id: i64,
    raw_participants: &[String],
) -> Result<Option<WhatsappAuditResponse>, sqlx::Error> {
    // 1. Vérifier si l'unité existe
    let unit_exists: Option<(i64,)> = sqlx::query_as("SELECT id FROM unites WHERE id = ?")
        .bind(unit_id)
        .fetch_optional(pool)
        .await?;

    if unit_exists.is_none() {
        return Ok(None);
    }

    // 2. Trouver la saison active
    let saison_row =
        sqlx::query("SELECT id FROM saisons WHERE active = 1 ORDER BY date_debut DESC LIMIT 1")
            .fetch_optional(pool)
            .await?;

    let saison_id: i64 = match saison_row {
        Some(r) => r.get("id"),
        None => {
            return Ok(Some(WhatsappAuditResponse {
                missing: vec![],
                unknown: vec![],
            }))
        }
    };

    // 3. Normaliser les participants reçus
    let mut normalized_participants = Vec::new();
    let mut participants_set = BTreeSet::new();

    for raw in raw_participants {
        let norm = match crate::phone::normalize_phone(raw) {
            Some(n) => n,
            None => {
                let cleaned = if let Some((before_at, _)) = raw.split_once('@') {
                    before_at.trim()
                } else {
                    raw.trim()
                };
                cleaned.to_string()
            }
        };

        if !norm.is_empty() {
            participants_set.insert(norm.clone());
            if !normalized_participants.contains(&norm) {
                normalized_participants.push(norm);
            }
        }
    }

    // 4. Collecter les téléphones de tous les adhérents inscrits dans l'unité (jeunes, chefs, cadres)
    let member_rows = sqlx::query(
        "SELECT ind.id, ind.nom, ind.prenom, ind.telephone_mobile, ind.telephone_domicile
         FROM inscriptions insc
         JOIN individus ind ON ind.id = insc.individu_id
         WHERE insc.unite_id = ? AND insc.saison_id = ? AND insc.active = 1",
    )
    .bind(unit_id)
    .bind(saison_id)
    .fetch_all(pool)
    .await?;

    let mut all_known_phones = BTreeSet::new();

    for r in &member_rows {
        let mob: Option<String> = r.get("telephone_mobile");
        let dom: Option<String> = r.get("telephone_domicile");
        if let Some(m) = mob {
            all_known_phones.insert(m);
        }
        if let Some(d) = dom {
            all_known_phones.insert(d);
        }
    }

    // 5. Déterminer si l'unité est une unité de jeunes mineurs (Farfadets, Louveteaux, Scouts, Pionniers)
    let youth_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM inscriptions
         WHERE unite_id = ? AND saison_id = ? AND active = 1
           AND fonction_code IN ('110', '111', '120', '121', '130', '131', '170')",
    )
    .bind(unit_id)
    .bind(saison_id)
    .fetch_one(pool)
    .await?;

    let is_youth_unit = youth_count > 0;

    struct ExpectedContact {
        nom: String,
        prenom: String,
        mobile: Option<String>,
        domicile: Option<String>,
        jeunes: BTreeSet<String>,
    }

    let mut expected_map: HashMap<i64, ExpectedContact> = HashMap::new();

    if is_youth_unit {
        // Unité de jeunes : les contacts attendus sont les représentants légaux des jeunes inscrits
        let rep_rows = sqlx::query(
            "SELECT p.id as rep_id, p.nom as rep_nom, p.prenom as rep_prenom,
                    p.telephone_mobile as rep_mobile, p.telephone_domicile as rep_domicile,
                    j.nom as jeune_nom, j.prenom as jeune_prenom
             FROM liens_parente lp
             JOIN individus p ON p.id = lp.parent_id
             JOIN individus j ON j.id = lp.jeune_id
             JOIN inscriptions insc ON insc.individu_id = j.id AND insc.saison_id = lp.saison_id AND insc.active = 1
             WHERE insc.unite_id = ? AND lp.saison_id = ?
               AND insc.fonction_code IN ('110', '111', '120', '121', '130', '131', '170')
             ORDER BY p.nom, p.prenom, j.prenom",
        )
        .bind(unit_id)
        .bind(saison_id)
        .fetch_all(pool)
        .await?;

        for r in rep_rows {
            let rep_id: i64 = r.get("rep_id");
            let rep_nom: String = r.get("rep_nom");
            let rep_prenom: String = r.get("rep_prenom");
            let rep_mobile: Option<String> = r.get("rep_mobile");
            let rep_domicile: Option<String> = r.get("rep_domicile");
            let jeune_nom: String = r.get("jeune_nom");
            let jeune_prenom: String = r.get("jeune_prenom");

            if let Some(m) = &rep_mobile {
                all_known_phones.insert(m.clone());
            }
            if let Some(d) = &rep_domicile {
                all_known_phones.insert(d.clone());
            }

            let entry = expected_map
                .entry(rep_id)
                .or_insert_with(|| ExpectedContact {
                    nom: rep_nom,
                    prenom: rep_prenom,
                    mobile: rep_mobile,
                    domicile: rep_domicile,
                    jeunes: BTreeSet::new(),
                });

            entry
                .jeunes
                .insert(format!("{} {}", jeune_prenom, jeune_nom));
        }
    } else {
        // Unité adulte / compagnons : les contacts attendus sont les adhérents eux-mêmes
        for r in &member_rows {
            let ind_id: i64 = r.get("id");
            let nom: String = r.get("nom");
            let prenom: String = r.get("prenom");
            let mobile: Option<String> = r.get("telephone_mobile");
            let domicile: Option<String> = r.get("telephone_domicile");

            expected_map.insert(
                ind_id,
                ExpectedContact {
                    nom,
                    prenom,
                    mobile,
                    domicile,
                    jeunes: BTreeSet::new(),
                },
            );
        }
    }

    // 6. Calcul différentiel des manquants
    let mut missing = Vec::new();

    for (_, contact) in expected_map {
        let is_present = match (&contact.mobile, &contact.domicile) {
            (Some(m), Some(d)) => participants_set.contains(m) || participants_set.contains(d),
            (Some(m), None) => participants_set.contains(m),
            (None, Some(d)) => participants_set.contains(d),
            (None, None) => false,
        };

        if !is_present {
            let mut telephones = Vec::new();
            if let Some(m) = &contact.mobile {
                telephones.push(m.clone());
            }
            if let Some(d) = &contact.domicile {
                telephones.push(d.clone());
            }

            missing.push(WhatsappAuditMissing {
                nom: contact.nom,
                prenom: contact.prenom,
                telephones,
                telephone_mobile: contact.mobile,
                telephone_domicile: contact.domicile,
                jeunes: contact.jeunes.into_iter().collect(),
            });
        }
    }

    missing.sort_by(|a, b| a.nom.cmp(&b.nom).then_with(|| a.prenom.cmp(&b.prenom)));

    // 7. Calcul différentiel des participants inconnus
    let mut unknown = Vec::new();
    for p in normalized_participants {
        if !all_known_phones.contains(&p) && !unknown.contains(&p) {
            unknown.push(p);
        }
    }
    unknown.sort();

    Ok(Some(WhatsappAuditResponse { missing, unknown }))
}
