use phonenumber::country::Id;

/// Normalise un numéro de téléphone au format international E.164 (ex. +33612345678).
/// Si le numéro commence sans indicatif international et a 10 chiffres (standard français),
/// il est interprété par défaut comme un numéro français (FR).
pub fn normalize_phone(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Tente d'abord le parsing avec la France par défaut
    if let Ok(phone) = phonenumber::parse(Some(Id::FR), trimmed) {
        if phonenumber::is_valid(&phone) {
            return Some(phone.format().mode(phonenumber::Mode::E164).to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_french_mobile() {
        assert_eq!(normalize_phone("0609266743"), Some("+33609266743".to_string()));
        assert_eq!(normalize_phone("06 09 26 67 43"), Some("+33609266743".to_string()));
        assert_eq!(normalize_phone("+33 6 09 26 67 43"), Some("+33609266743".to_string()));
    }

    #[test]
    fn test_empty_phone() {
        assert_eq!(normalize_phone(""), None);
        assert_eq!(normalize_phone("   "), None);
    }
}
