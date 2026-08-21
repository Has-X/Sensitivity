// Copyright (C) 2026 Chromatic
// Licensed under the GNU AGPL v3.0. See LICENSE file for details.
// Website: https://chromatic.hu

use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    English,
    Hungarian,
    Spanish,
    German,
    French,
}

impl Language {
    pub fn from_environment() -> Self {
        let requested = std::env::var("SENSITIVITY_LANG")
            .ok()
            .or_else(|| std::env::var("LC_ALL").ok())
            .or_else(|| std::env::var("LANG").ok())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if requested.starts_with("hu") {
            Self::Hungarian
        } else if requested.starts_with("es") {
            Self::Spanish
        } else if requested.starts_with("de") {
            Self::German
        } else if requested.starts_with("fr") {
            Self::French
        } else {
            Self::English
        }
    }

    fn catalog(self) -> &'static HashMap<String, String> {
        static EN: OnceLock<HashMap<String, String>> = OnceLock::new();
        static HU: OnceLock<HashMap<String, String>> = OnceLock::new();
        static ES: OnceLock<HashMap<String, String>> = OnceLock::new();
        static DE: OnceLock<HashMap<String, String>> = OnceLock::new();
        static FR: OnceLock<HashMap<String, String>> = OnceLock::new();
        let (slot, source) = match self {
            Self::English => (&EN, include_str!("../locales/en/cli.json")),
            Self::Hungarian => (&HU, include_str!("../locales/hu/cli.json")),
            Self::Spanish => (&ES, include_str!("../locales/es/cli.json")),
            Self::German => (&DE, include_str!("../locales/de/cli.json")),
            Self::French => (&FR, include_str!("../locales/fr/cli.json")),
        };
        slot.get_or_init(|| serde_json::from_str(source).expect("valid embedded CLI locale"))
    }
}

pub fn language() -> Language {
    Language::from_environment()
}

pub fn tr(key: &str) -> String {
    language()
        .catalog()
        .get(key)
        .cloned()
        .unwrap_or_else(|| key.to_owned())
}

pub fn trf(key: &str, replacements: &[(&str, &str)]) -> String {
    let mut value = tr(key);
    for (placeholder, replacement) in replacements {
        value = value.replace(placeholder, replacement);
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_catalogs_contain_the_same_keys() {
        let en = Language::English.catalog();
        let hu = Language::Hungarian.catalog();
        let es = Language::Spanish.catalog();
        let de = Language::German.catalog();
        let fr = Language::French.catalog();
        assert_eq!(en.len(), hu.len());
        assert_eq!(en.len(), es.len());
        assert_eq!(en.len(), de.len());
        assert_eq!(en.len(), fr.len());
        for key in en.keys() {
            assert!(hu.contains_key(key), "missing Hungarian key {key}");
            assert!(es.contains_key(key), "missing Spanish key {key}");
            assert!(de.contains_key(key), "missing German key {key}");
            assert!(fr.contains_key(key), "missing French key {key}");
        }
    }
}
