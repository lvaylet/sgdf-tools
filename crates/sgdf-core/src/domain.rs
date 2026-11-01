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
pub struct WhatsappAuditRequest {
    pub participants: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WhatsappAuditMissing {
    pub nom: String,
    pub prenom: String,
    pub telephones: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub telephone_mobile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub telephone_domicile: Option<String>,
    pub jeunes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WhatsappAuditIntruder {
    pub nom: String,
    pub prenom: String,
    pub telephone: String,
    pub description: String,
    pub unites_legitimes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WhatsappAuditResponse {
    pub missing: Vec<WhatsappAuditMissing>,
    #[serde(default)]
    pub unexpected: Vec<WhatsappAuditIntruder>,
    pub unknown: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WhatsappGroupParticipantsInput {
    pub unit_id: Option<i64>,
    pub jid: Option<String>,
    pub participants: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GlobalWhatsappAuditRequest {
    Wrapped {
        groups: Vec<WhatsappGroupParticipantsInput>,
    },
    List(Vec<WhatsappGroupParticipantsInput>),
}

impl GlobalWhatsappAuditRequest {
    pub fn groups(&self) -> &[WhatsappGroupParticipantsInput] {
        match self {
            Self::Wrapped { groups } => groups,
            Self::List(groups) => groups,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitWhatsappAuditResult {
    pub unit_id: i64,
    pub unit_nom: String,
    pub whatsapp_group_jid: Option<String>,
    pub audit: WhatsappAuditResponse,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalWhatsappAuditSummary {
    pub units_auditees: usize,
    pub total_manquants: usize,
    pub total_intrus_connus: usize,
    pub total_inconnus: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalWhatsappAuditResponse {
    pub summary: GlobalWhatsappAuditSummary,
    pub units: Vec<UnitWhatsappAuditResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoogleContactEntry {
    pub nom: String,
    pub prenom: String,
    pub telephone_mobile: Option<String>,
    pub telephone_domicile: Option<String>,
    pub courriel_personnel: Option<String>,
    pub courriel_sgdf: Option<String>,
    pub notes: Option<String>,
    pub labels: Vec<String>,
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
pub struct JeuneRattache {
    pub nom: String,
    pub prenom: String,
    pub unite: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepresentantLegalDetail {
    pub id: i64,
    pub nom: String,
    pub prenom: String,
    pub civilite: Option<String>,
    pub code_adherent: Option<String>,
    pub telephone_mobile: Option<String>,
    pub telephone_domicile: Option<String>,
    pub courriel_personnel: Option<String>,
    pub courriel_sgdf: Option<String>,
    pub adresse_ligne1: Option<String>,
    pub adresse_ligne2: Option<String>,
    pub adresse_code_postal: Option<String>,
    pub adresse_ville: Option<String>,
    pub jeunes: Vec<JeuneRattache>,
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
