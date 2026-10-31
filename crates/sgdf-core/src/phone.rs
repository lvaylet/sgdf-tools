use phonenumber::country::Id;

/// Normalise un numéro de téléphone au format international E.164 (ex. +33612345678).
/// Si le numéro commence sans indicatif international et a 10 chiffres (standard français),
/// il est interprété par défaut comme un numéro français (FR).
pub fn normalize_phone(raw: &str) -> Option<String> {
    let mut cleaned = raw.trim();
    if cleaned.is_empty() {
        return None;
    }

    // Si le numéro se termine par un domaine WhatsApp (ex: @c.us ou @s.whatsapp.net), on le retire
    if let Some((before_at, _)) = cleaned.split_once('@') {
        cleaned = before_at.trim();
    }

    // Supprime les séparateurs courants (espaces, tirets, points, parenthèses)
    let filtered: String = cleaned
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '+')
        .collect();

    if filtered.is_empty() {
        return None;
    }

    // Si commence par 00, remplace par +
    let candidate = if let Some(stripped) = filtered.strip_prefix("00") {
        format!("+{}", stripped)
    } else {
        filtered
    };

    // 1. Tente le parsing avec l'indicatif FR par défaut
    if let Ok(phone) = phonenumber::parse(Some(Id::FR), &candidate) {
        if phonenumber::is_valid(&phone) {
            return Some(phone.format().mode(phonenumber::Mode::E164).to_string());
        }
    }

    // 2. Si pas de '+' au début (ex: "33612345678"), tente en préfixant par '+'
    if !candidate.starts_with('+') {
        let with_plus = format!("+{}", candidate);
        if let Ok(phone) = phonenumber::parse(Some(Id::FR), &with_plus) {
            if phonenumber::is_valid(&phone) {
                return Some(phone.format().mode(phonenumber::Mode::E164).to_string());
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_french_mobile() {
        assert_eq!(
            normalize_phone("0609266743"),
            Some("+33609266743".to_string())
        );
        assert_eq!(
            normalize_phone("06 09 26 67 43"),
            Some("+33609266743".to_string())
        );
        assert_eq!(
            normalize_phone("06.09.26.67.43"),
            Some("+33609266743".to_string())
        );
        assert_eq!(
            normalize_phone("+33 6 09 26 67 43"),
            Some("+33609266743".to_string())
        );
        assert_eq!(
            normalize_phone("33609266743"),
            Some("+33609266743".to_string())
        );
        assert_eq!(
            normalize_phone("33609266743@c.us"),
            Some("+33609266743".to_string())
        );
        assert_eq!(
            normalize_phone("0033609266743"),
            Some("+33609266743".to_string())
        );
    }

    #[test]
    fn test_empty_phone() {
        assert_eq!(normalize_phone(""), None);
        assert_eq!(normalize_phone("   "), None);
        assert_eq!(normalize_phone("non-un-numero"), None);
    }
}
