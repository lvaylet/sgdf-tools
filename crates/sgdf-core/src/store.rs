use crate::domain::{ImportSummary, ParsedRow};
use chrono::{Datelike, NaiveDate};
use sqlx::{Row, SqlitePool};

fn deduce_saison(start: Option<NaiveDate>, end: Option<NaiveDate>) -> (String, NaiveDate, NaiveDate) {
    if let (Some(s), Some(e)) = (start, end) {
        let year_start = if s.month() >= 9 { s.year() } else { s.year() - 1 };
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

pub async fn import_rows(pool: &SqlitePool, rows: &[ParsedRow]) -> Result<ImportSummary, sqlx::Error> {
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
        let existing: Option<(i64,)> = sqlx::query_as("SELECT id FROM individus WHERE code_adherent = ?")
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
        // Déduplication basée sur la clé composite (nom + prénom + concordance contact)
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
