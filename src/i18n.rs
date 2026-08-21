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
        } else {
            Self::English
        }
    }

    fn catalog(self) -> &'static HashMap<String, String> {
        static EN: OnceLock<HashMap<String, String>> = OnceLock::new();
        static HU: OnceLock<HashMap<String, String>> = OnceLock::new();
        static ES: OnceLock<HashMap<String, String>> = OnceLock::new();
        let (slot, source) = match self {
            Self::English => (&EN, include_str!("../locales/en.json")),
            Self::Hungarian => (&HU, include_str!("../locales/hu.json")),
            Self::Spanish => (&ES, include_str!("../locales/es.json")),
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
        assert_eq!(en.len(), hu.len());
        assert_eq!(en.len(), es.len());
        for key in en.keys() {
            assert!(hu.contains_key(key), "missing Hungarian key {key}");
            assert!(es.contains_key(key), "missing Spanish key {key}");
        }
    }
}
