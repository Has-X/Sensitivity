// Copyright (C) 2026 Chromatic
// Licensed under the GNU AGPL v3.0. See LICENSE file for details.

use std::{collections::HashMap, sync::OnceLock};

macro_rules! define_languages {
    ($(($variant:ident, $code:literal, $prefix:literal)),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Language { $($variant),+ }

        static CATALOGS: OnceLock<HashMap<Language, HashMap<String, String>>> = OnceLock::new();

        impl Language {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            pub fn from_environment() -> Self {
                let requested = std::env::var("SENSITIVITY_LANG")
                    .ok().or_else(|| std::env::var("LC_ALL").ok())
                    .or_else(|| std::env::var("LANG").ok()).unwrap_or_default().to_ascii_lowercase();
                $(if requested.starts_with($prefix) { return Self::$variant; })+
                Self::English
            }

            pub fn code(self) -> &'static str {
                match self { $(Self::$variant => $code),+ }
            }

            fn catalog(self) -> &'static HashMap<String, String> {
                let catalogs = CATALOGS.get_or_init(|| {
                    HashMap::from([
                        $((Self::$variant, serde_json::from_str(include_str!(concat!("../locales/", $code, "/cli.json"))).expect("valid embedded CLI locale")),)+
                    ])
                });
                &catalogs[&self]
            }
        }
    };
}

define_languages!(
    (English, "en", "en"),
    (Hungarian, "hu", "hu"),
    (Spanish, "es", "es"),
    (German, "de", "de"),
    (French, "fr", "fr"),
    (Italian, "it", "it"),
    (Polish, "pl", "pl"),
    (PortuguesePortugal, "pt-PT", "pt-pt"),
    (PortugueseBrazil, "pt-BR", "pt"),
    (Turkish, "tr", "tr"),
    (Indonesian, "id", "id"),
    (Romanian, "ro", "ro"),
    (Czech, "cs", "cs"),
    (Slovak, "sk", "sk"),
    (Russian, "ru", "ru"),
    (Ukrainian, "uk", "uk"),
    (ChineseTraditional, "zh-TW", "zh-tw"),
    (ChineseSimplified, "zh-CN", "zh"),
    (Arabic, "ar", "ar"),
    (Vietnamese, "vi", "vi"),
    (Thai, "th", "th"),
    (Hindi, "hi", "hi"),
    (Japanese, "ja", "ja"),
    (Korean, "ko", "ko"),
    (Dutch, "nl", "nl"),
    (Greek, "el", "el"),
    (Bulgarian, "bg", "bg"),
    (Croatian, "hr", "hr"),
    (Serbian, "sr", "sr"),
    (Slovenian, "sl", "sl"),
    (Swedish, "sv", "sv"),
    (Danish, "da", "da"),
    (Finnish, "fi", "fi"),
    (Norwegian, "nb", "nb"),
);

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
        let english = Language::English.catalog();
        for language in Language::ALL {
            let catalog = language.catalog();
            assert_eq!(
                english.len(),
                catalog.len(),
                "catalog size mismatch for {}",
                language.code()
            );
            for key in english.keys() {
                assert!(
                    catalog.contains_key(key),
                    "missing {} key {key}",
                    language.code()
                );
            }
        }
    }
}
