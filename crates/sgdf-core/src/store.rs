use crate::domain::{
    AdherentDetails, GoogleContactEntry, RepresentantLegalSummary, UnitAdherentsResponse,
    UnitSummary, Unite, WhatsappAuditMissing, WhatsappAuditResponse,
};
use chrono::NaiveDate;
use sqlx::{Row, SqlitePool};
use std::collections::{BTreeSet, HashMap};

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

// Codes fonctions officiels SGDF pour les jeunes mineurs (Farfadets, Louveteaux-Jeannettes, Scouts-Guides, Pionniers-Caravelles)
const YOUTH_FUNCTION_CODES_SQL: &str = "'110', '111', '120', '121', '130', '131', '170'";

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
        let norm = crate::phone::normalize_phone(raw).unwrap_or_else(|| raw.trim().to_string());
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
    let youth_query = format!(
        "SELECT COUNT(*) FROM inscriptions
         WHERE unite_id = ? AND saison_id = ? AND active = 1
           AND fonction_code IN ({})",
        YOUTH_FUNCTION_CODES_SQL
    );
    let youth_count: i64 = sqlx::query_scalar(&youth_query)
        .bind(unit_id)
        .bind(saison_id)
        .fetch_one(pool)
        .await?;

    let is_youth_unit = youth_count > 0;

    struct ExpectedIndividu {
        nom: String,
        prenom: String,
        mobile: Option<String>,
        domicile: Option<String>,
        jeunes: BTreeSet<String>,
    }

    let mut expected_map: HashMap<i64, ExpectedIndividu> = HashMap::new();

    if is_youth_unit {
        // Unité de jeunes : les contacts attendus sont les représentants légaux des jeunes inscrits
        let rep_query = format!(
            "SELECT p.id as rep_id, p.nom as rep_nom, p.prenom as rep_prenom,
                    p.telephone_mobile as rep_mobile, p.telephone_domicile as rep_domicile,
                    j.nom as jeune_nom, j.prenom as jeune_prenom
             FROM liens_parente lp
             JOIN individus p ON p.id = lp.parent_id
             JOIN individus j ON j.id = lp.jeune_id
             JOIN inscriptions insc ON insc.individu_id = j.id AND insc.saison_id = lp.saison_id AND insc.active = 1
             WHERE insc.unite_id = ? AND lp.saison_id = ?
               AND insc.fonction_code IN ({})
             ORDER BY p.nom, p.prenom, j.prenom",
            YOUTH_FUNCTION_CODES_SQL
        );
        let rep_rows = sqlx::query(&rep_query)
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
                .or_insert_with(|| ExpectedIndividu {
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
                ExpectedIndividu {
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
