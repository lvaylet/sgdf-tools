use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportSummary {
    pub individus_importes: i64,
    pub unites_importees: i64,
    pub inscriptions_importees: i64,
    pub liens_parente_importes: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Individu {
    pub id: Option<i64>,
    pub civilite: Option<String>,
    pub code_adherent: Option<String>,
    pub nom: String,
    pub prenom: String,
    pub date_naissance: Option<NaiveDate>,
    pub telephone_mobile: Option<String>,
    pub telephone_domicile: Option<String>,
    pub courriel_personnel: Option<String>,
    pub courriel_sgdf: Option<String>,
    pub adresse_ligne1: Option<String>,
    pub adresse_ligne2: Option<String>,
    pub adresse_code_postal: Option<String>,
    pub adresse_ville: Option<String>,
    pub autorisation_soins_urgence: bool,
    pub droit_image_interne: bool,
    pub droit_image_externe: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unite {
    pub id: Option<i64>,
    pub code_structure: String,
    pub nom: String,
    pub whatsapp_group_jid: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitSummary {
    pub id: i64,
    pub code_structure: String,
    pub nom: String,
    pub whatsapp_group_jid: Option<String>,
    #[serde(alias = "members_count")]
    pub adherents_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepresentantLegalSummary {
    pub id: i64,
    pub nom: String,
    pub prenom: String,
    pub telephone_mobile: Option<String>,
    pub courriel: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdherentDetails {
    pub id: i64,
    pub nom: String,
    pub prenom: String,
    pub code_adherent: Option<String>,
    pub fonction: Option<String>,
    pub date_naissance: Option<NaiveDate>,
    pub telephone_mobile: Option<String>,
    pub courriel: Option<String>,
    pub representants_legaux: Vec<RepresentantLegalSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitAdherentsResponse {
    pub unit: Unite,
    pub saison: String,
    #[serde(alias = "members")]
    pub adherents: Vec<AdherentDetails>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateUnitPayload {
    #[serde(default)]
    pub whatsapp_group_jid: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saison {
    pub id: Option<i64>,
    pub libelle: String,
    pub date_debut: NaiveDate,
    pub date_fin: NaiveDate,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedRepresentantLegal {
    pub civilite: Option<String>,
    pub code_adherent: Option<String>,
    pub nom: String,
    pub prenom: String,
    pub telephone_mobile: Option<String>,
    pub telephone_domicile: Option<String>,
    pub courriel_personnel: Option<String>,
    pub courriel_sgdf: Option<String>,
    pub adresse_ligne1: Option<String>,
    pub adresse_code_postal: Option<String>,
    pub adresse_ville: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedRow {
    pub civilite: Option<String>,
    pub code_adherent: Option<String>,
    pub nom: String,
    pub prenom: String,
    pub structure_code: String,
    pub structure_nom: String,
    pub fonction_code: Option<String>,
    pub fonction_nom: Option<String>,
    pub date_naissance: Option<NaiveDate>,
    pub telephone_mobile: Option<String>,
    pub telephone_domicile: Option<String>,
    pub courriel_personnel: Option<String>,
    pub courriel_sgdf: Option<String>,
    pub adresse_ligne1: Option<String>,
    pub adresse_ligne2: Option<String>,
    pub adresse_code_postal: Option<String>,
    pub adresse_ville: Option<String>,
    pub autorisation_soins: bool,
    pub droit_image_interne: bool,
    pub droit_image_externe: bool,
    pub date_debut_adhesion: Option<NaiveDate>,
    pub date_fin_adhesion: Option<NaiveDate>,
    pub statut_adhesion: Option<String>,
    pub representants_legaux: Vec<ParsedRepresentantLegal>,
}
