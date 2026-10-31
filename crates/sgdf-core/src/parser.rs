use crate::domain::{ParsedRepresentantLegal, ParsedRow};
use crate::phone::normalize_phone;
use chrono::NaiveDate;
use scraper::{Html, Selector};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParserError {
    #[error("Format HTML invalide ou aucune ligne trouvée")]
    InvalidFormat,

    #[error("Erreur de parsing de date : {0}")]
    DateParse(#[from] chrono::ParseError),
}

fn clean_text(raw: &str) -> String {
    html_escape::decode_html_entities(raw).trim().to_string()
}

fn parse_opt_text(raw: &str) -> Option<String> {
    let s = clean_text(raw);
    if s.is_empty() || s == "-" {
        None
    } else {
        Some(s)
    }
}

fn parse_date(raw: &str) -> Option<NaiveDate> {
    let s = clean_text(raw);
    if s.is_empty() {
        return None;
    }
    NaiveDate::parse_from_str(&s, "%d/%m/%Y").ok()
}

fn parse_bool(raw: &str) -> bool {
    let s = clean_text(raw);
    s.eq_ignore_ascii_case("oui") || s.eq_ignore_ascii_case("true") || s == "1"
}

fn get_cell<'a>(
    header_map: &HashMap<String, usize>,
    cells: &'a [String],
    col_name: &str,
) -> Option<&'a str> {
    header_map
        .get(col_name)
        .and_then(|&idx| cells.get(idx))
        .map(|s| s.as_str())
}

fn extract_phone(
    header_map: &HashMap<String, usize>,
    cells: &[String],
    col_pref: &str,
    col_alt: Option<&str>,
) -> Option<String> {
    let raw = get_cell(header_map, cells, col_pref)
        .and_then(parse_opt_text)
        .or_else(|| col_alt.and_then(|c| get_cell(header_map, cells, c)).and_then(parse_opt_text));
    raw.as_deref().and_then(normalize_phone).or(raw)
}

pub fn parse_gestact_export(bytes: &[u8]) -> Result<Vec<ParsedRow>, ParserError> {
    let (decoded, _, _) = encoding_rs::WINDOWS_1252.decode(bytes);
    let document = Html::parse_document(&decoded);

    let tr_selector = Selector::parse("tr").map_err(|_| ParserError::InvalidFormat)?;
    let td_selector = Selector::parse("td, th").map_err(|_| ParserError::InvalidFormat)?;

    let mut rows = document.select(&tr_selector);
    let header_row = rows.next().ok_or(ParserError::InvalidFormat)?;

    let mut header_map: HashMap<String, usize> = HashMap::new();
    for (idx, cell) in header_row.select(&td_selector).enumerate() {
        let text = clean_text(&cell.text().collect::<String>());
        header_map.insert(text, idx);
    }

    let mut parsed_records = Vec::new();

    for row in rows {
        let cell_values: Vec<String> = row
            .select(&td_selector)
            .map(|c| c.text().collect::<String>())
            .collect();

        if cell_values.is_empty() {
            continue;
        }

        let nom = match get_cell(&header_map, &cell_values, "Individu.Nom").and_then(parse_opt_text) {
            Some(n) => n,
            None => continue,
        };

        let prenom = match get_cell(&header_map, &cell_values, "Individu.Prenom").and_then(parse_opt_text) {
            Some(p) => p,
            None => continue,
        };

        let structure_code = get_cell(&header_map, &cell_values, "Structure.CodeStructure")
            .and_then(parse_opt_text)
            .unwrap_or_default();

        let structure_nom = get_cell(&header_map, &cell_values, "Structure.Nom")
            .and_then(parse_opt_text)
            .unwrap_or_default();

        let civilite = get_cell(&header_map, &cell_values, "IndividuCivilite.NomCourt").and_then(parse_opt_text);
        let code_adherent = get_cell(&header_map, &cell_values, "Individu.CodeAdherent").and_then(parse_opt_text);
        let fonction_code = get_cell(&header_map, &cell_values, "Fonction.Code").and_then(parse_opt_text);
        let fonction_nom = get_cell(&header_map, &cell_values, "Fonction.NomMasculin")
            .and_then(parse_opt_text)
            .or_else(|| get_cell(&header_map, &cell_values, "Fonction.NomFeminin").and_then(parse_opt_text));

        let date_naissance = get_cell(&header_map, &cell_values, "Individu.DateNaissance").and_then(parse_date);

        let telephone_mobile = extract_phone(
            &header_map,
            &cell_values,
            "Individu.TelephonePortable1",
            Some("Individu.TelephonePortable2"),
        );
        let telephone_domicile = extract_phone(
            &header_map,
            &cell_values,
            "Individu.TelephoneDomicile",
            None,
        );

        let courriel_personnel = get_cell(&header_map, &cell_values, "Individu.CourrielPersonnel").and_then(parse_opt_text);
        let courriel_sgdf = get_cell(&header_map, &cell_values, "Individu.CourrielDédiéSGDF").and_then(parse_opt_text);

        let adresse_ligne1 = get_cell(&header_map, &cell_values, "Individu.Adresse.Ligne1").and_then(parse_opt_text);
        let adresse_ligne2 = get_cell(&header_map, &cell_values, "Individu.Adresse.Ligne2").and_then(parse_opt_text);
        let adresse_code_postal = get_cell(&header_map, &cell_values, "Individu.Adresse.CodePostal").and_then(parse_opt_text);
        let adresse_ville = get_cell(&header_map, &cell_values, "Individu.Adresse.Municipalite").and_then(parse_opt_text);

        let autorisation_soins = get_cell(&header_map, &cell_values, "Individu.AutorisationInterventionChirurgicale")
            .map(parse_bool)
            .unwrap_or(false);
        let droit_image_interne = get_cell(&header_map, &cell_values, "Utilisation d'image interne")
            .map(parse_bool)
            .unwrap_or(false);
        let droit_image_externe = get_cell(&header_map, &cell_values, "Utilisation d'image externe")
            .map(parse_bool)
            .unwrap_or(false);

        let date_debut_adhesion = get_cell(&header_map, &cell_values, "Adhesions.DateDebut").and_then(parse_date);
        let date_fin_adhesion = get_cell(&header_map, &cell_values, "Adhesions.DateFin").and_then(parse_date);
        let statut_adhesion = get_cell(&header_map, &cell_values, "Adhesions.Statut").and_then(parse_opt_text);

        // Représentants légaux 1, 2, 3
        let mut representants_legaux = Vec::new();
        for i in 1..=3 {
            let prefix = format!("RepresentantLegal{}", i);
            let p_nom_key = format!("{}.Nom", prefix);
            let p_prenom_key = format!("{}.Prenom", prefix);

            if let (Some(p_nom), Some(p_prenom)) = (
                get_cell(&header_map, &cell_values, &p_nom_key).and_then(parse_opt_text),
                get_cell(&header_map, &cell_values, &p_prenom_key).and_then(parse_opt_text),
            ) {
                let p_civ = get_cell(&header_map, &cell_values, &format!("{}Civilite.NomCourt", prefix)).and_then(parse_opt_text);
                let p_code = get_cell(&header_map, &cell_values, &format!("{}.CodeAdherent", prefix)).and_then(parse_opt_text);

                let p_mobile = extract_phone(
                    &header_map,
                    &cell_values,
                    &format!("{}.TelephonePortable1", prefix),
                    Some(&format!("{}.TelephonePortable2", prefix)),
                );
                let p_domicile = extract_phone(
                    &header_map,
                    &cell_values,
                    &format!("{}.TelephoneDomicile", prefix),
                    None,
                );

                let p_email = get_cell(&header_map, &cell_values, &format!("{}.CourrielPersonnel", prefix)).and_then(parse_opt_text);
                let p_email_sgdf = get_cell(&header_map, &cell_values, &format!("{}.CourrielDédiéSGDF", prefix)).and_then(parse_opt_text);

                let p_addr1 = get_cell(&header_map, &cell_values, &format!("{}.Adresse.Ligne1", prefix)).and_then(parse_opt_text);
                let p_cp = get_cell(&header_map, &cell_values, &format!("{}.Adresse.CodePostal", prefix)).and_then(parse_opt_text);
                let p_ville = get_cell(&header_map, &cell_values, &format!("{}.Adresse.Municipalite", prefix)).and_then(parse_opt_text);

                representants_legaux.push(ParsedRepresentantLegal {
                    civilite: p_civ,
                    code_adherent: p_code,
                    nom: p_nom,
                    prenom: p_prenom,
                    telephone_mobile: p_mobile,
                    telephone_domicile: p_domicile,
                    courriel_personnel: p_email,
                    courriel_sgdf: p_email_sgdf,
                    adresse_ligne1: p_addr1,
                    adresse_code_postal: p_cp,
                    adresse_ville: p_ville,
                });
            }
        }

        parsed_records.push(ParsedRow {
            civilite,
            code_adherent,
            nom,
            prenom,
            structure_code,
            structure_nom,
            fonction_code,
            fonction_nom,
            date_naissance,
            telephone_mobile,
            telephone_domicile,
            courriel_personnel,
            courriel_sgdf,
            adresse_ligne1,
            adresse_ligne2,
            adresse_code_postal,
            adresse_ville,
            autorisation_soins,
            droit_image_interne,
            droit_image_externe,
            date_debut_adhesion,
            date_fin_adhesion,
            statut_adhesion,
            representants_legaux,
        });
    }

    Ok(parsed_records)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sample_file() {
        let bytes = include_bytes!("../../../assets/ExportIndividus.xls");
        let rows = parse_gestact_export(bytes).expect("Le fichier doit être parsable");
        assert_eq!(rows.len(), 112);

        let laurent = rows.iter().find(|r| r.nom == "VAYLET" && r.prenom == "LAURENT");
        assert!(laurent.is_some());
        let l = laurent.unwrap();
        assert_eq!(l.code_adherent.as_deref(), Some("164620934"));
        assert_eq!(l.telephone_mobile.as_deref(), Some("+33687966975"));
        assert_eq!(l.structure_nom, "GROUPE AUFREDY - LA ROCHELLE");
    }
}
