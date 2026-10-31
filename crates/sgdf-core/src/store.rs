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
        // 2. Unite
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

        // 3. Individu principal (Adhérent jeune ou bénévole)
        let individu_id = upsert_individu(
            &mut tx,
            row.code_adherent.as_deref(),
            row.civilite.as_deref(),
            &row.nom,
            &row.prenom,
            row.date_naissance,
            row.telephone_mobile.as_deref(),
            row.telephone_domicile.as_deref(),
            row.courriel_personnel.as_deref(),
            row.courriel_sgdf.as_deref(),
            row.adresse_ligne1.as_deref(),
            row.adresse_ligne2.as_deref(),
            row.adresse_code_postal.as_deref(),
            row.adresse_ville.as_deref(),
            row.autorisation_soins,
            row.droit_image_interne,
            row.droit_image_externe,
        )
        .await?;

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
        for (idx, parent) in row.parents.iter().enumerate() {
            let parent_id = upsert_individu(
                &mut tx,
                parent.code_adherent.as_deref(),
                parent.civilite.as_deref(),
                &parent.nom,
                &parent.prenom,
                None,
                parent.telephone_mobile.as_deref(),
                parent.telephone_domicile.as_deref(),
                parent.courriel_personnel.as_deref(),
                parent.courriel_sgdf.as_deref(),
                parent.adresse_ligne1.as_deref(),
                None,
                parent.adresse_code_postal.as_deref(),
                parent.adresse_ville.as_deref(),
                false,
                false,
                false,
            )
            .await?;

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

    // Calcul des totaux finaux
    let total_individus: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM individus")
        .fetch_one(&mut *tx)
        .await?;

    let total_unites: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM unites")
        .fetch_one(&mut *tx)
        .await?;

    let unites_count = total_unites;

    tx.commit().await?;

    Ok(ImportSummary {
        individus_imported: total_individus,
        unites_imported: unites_count,
        inscriptions_imported: inscriptions_count,
        liens_parente_imported: liens_count,
    })
}

type CandidateRecord = (i64, Option<String>, Option<String>, Option<String>);

#[allow(clippy::too_many_arguments)]
async fn upsert_individu(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    code_adherent: Option<&str>,
    civilite: Option<&str>,
    nom: &str,
    prenom: &str,
    date_naissance: Option<NaiveDate>,
    mobile: Option<&str>,
    domicile: Option<&str>,
    courriel_perso: Option<&str>,
    courriel_sgdf: Option<&str>,
    adr1: Option<&str>,
    adr2: Option<&str>,
    cp: Option<&str>,
    ville: Option<&str>,
    soins: bool,
    image_int: bool,
    image_ext: bool,
) -> Result<i64, sqlx::Error> {
    let nom_clean = nom.trim().to_uppercase();
    let prenom_clean = prenom.trim().to_uppercase();
    let dn_str = date_naissance.map(|d| d.to_string());

    // 1. Recherche par code adhérent si renseigné
    if let Some(code) = code_adherent.filter(|c| !c.trim().is_empty()) {
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
            .bind(civilite)
            .bind(&nom_clean)
            .bind(&prenom_clean)
            .bind(dn_str.as_deref())
            .bind(mobile)
            .bind(domicile)
            .bind(courriel_perso)
            .bind(courriel_sgdf)
            .bind(adr1)
            .bind(adr2)
            .bind(cp)
            .bind(ville)
            .bind(soins as i32)
            .bind(image_int as i32)
            .bind(image_ext as i32)
            .bind(id)
            .execute(&mut **tx)
            .await?;

            return Ok(id);
        }
    }

    // 2. Recherche par Nom + Prénom (déduplication pour les parents sans code adhérent ou avant rattachement)
    let candidates: Vec<CandidateRecord> = sqlx::query_as(
        "SELECT id, code_adherent, telephone_mobile, courriel_personnel
         FROM individus
         WHERE UPPER(nom) = ? AND UPPER(prenom) = ?",
    )
    .bind(&nom_clean)
    .bind(&prenom_clean)
    .fetch_all(&mut **tx)
    .await?;

    if !candidates.is_empty() {
        // Si un seul candidat ou correspondance sur mobile/email, on fusionne
        let matched_id = candidates.iter().find(|(_, _, cand_mob, cand_email)| {
            (mobile.is_some() && mobile == cand_mob.as_deref())
                || (courriel_perso.is_some() && courriel_perso == cand_email.as_deref())
        }).map(|(id, ..)| *id).unwrap_or(candidates[0].0);

        sqlx::query(
            "UPDATE individus SET
                code_adherent = COALESCE(?, code_adherent),
                civilite = COALESCE(?, civilite),
                telephone_mobile = COALESCE(?, telephone_mobile),
                telephone_domicile = COALESCE(?, telephone_domicile),
                courriel_personnel = COALESCE(?, courriel_personnel),
                courriel_sgdf = COALESCE(?, courriel_sgdf),
                adresse_ligne1 = COALESCE(?, adresse_ligne1),
                adresse_code_postal = COALESCE(?, adresse_code_postal),
                adresse_ville = COALESCE(?, adresse_ville),
                updated_at = CURRENT_TIMESTAMP
             WHERE id = ?",
        )
        .bind(code_adherent)
        .bind(civilite)
        .bind(mobile)
        .bind(domicile)
        .bind(courriel_perso)
        .bind(courriel_sgdf)
        .bind(adr1)
        .bind(cp)
        .bind(ville)
        .bind(matched_id)
        .execute(&mut **tx)
        .await?;

        return Ok(matched_id);
    }

    // 3. Nouveau profil
    let new_id: i64 = sqlx::query(
        "INSERT INTO individus (
            code_adherent, civilite, nom, prenom, date_naissance,
            telephone_mobile, telephone_domicile, courriel_personnel, courriel_sgdf,
            adresse_ligne1, adresse_ligne2, adresse_code_postal, adresse_ville,
            autorisation_soins_urgence, droit_image_interne, droit_image_externe
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        RETURNING id",
    )
    .bind(code_adherent)
    .bind(civilite)
    .bind(&nom_clean)
    .bind(&prenom_clean)
    .bind(dn_str.as_deref())
    .bind(mobile)
    .bind(domicile)
    .bind(courriel_perso)
    .bind(courriel_sgdf)
    .bind(adr1)
    .bind(adr2)
    .bind(cp)
    .bind(ville)
    .bind(soins as i32)
    .bind(image_int as i32)
    .bind(image_ext as i32)
    .fetch_one(&mut **tx)
    .await?
    .get("id");

    Ok(new_id)
}
