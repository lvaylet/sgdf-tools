use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Individu {
    pub civilite: Option<String>,
    pub code_adherent: Option<String>,
    pub nom: String,
    pub prenom: String,
    pub date_naissance: Option<NaiveDate>,
    pub telephone_mobile: Option<String>,
    pub telephone_domicile: Option<String>,
    pub courriel_personnel: Option<String>,
    pub courriel_sgdf: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unite {
    pub code_structure: String,
    pub nom: String,
    pub whatsapp_group_jid: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LienParente {
    pub parent: Individu,
    pub jeune: Individu,
    pub unite: Unite,
}
