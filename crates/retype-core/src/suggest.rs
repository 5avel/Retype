//! "Did you mean …?" for configured layouts that are not installed, e.g.
//! `ua-UA` (country code) where Windows calls Ukrainian `uk-UA`.

use crate::layout::LayoutId;

/// Country codes people write where a language code belongs.
const COUNTRY_AS_LANGUAGE: [(&str, &str); 14] = [
    ("ua", "uk"),
    ("by", "be"),
    ("kz", "kk"),
    ("cz", "cs"),
    ("dk", "da"),
    ("se", "sv"),
    ("gr", "el"),
    ("jp", "ja"),
    ("kr", "ko"),
    ("cn", "zh"),
    ("il", "he"),
    ("ee", "et"),
    ("si", "sl"),
    ("rs", "sr"),
];

/// An installed layout the user probably meant by `missing`, if any.
///
/// In order of preference: the same language in another region (`en-GB` →
/// `en-US`), the language a country code stands for (`ua` → `uk-UA`), the
/// same region with another language (`ua-UA` → `uk-UA`).
pub fn maybe_suggest_layout<'a>(
    missing: &LayoutId,
    installed: &'a [LayoutId],
) -> Option<&'a LayoutId> {
    let (language, maybe_region) = split(missing);
    let same_language = |lang: &str| installed.iter().find(|id| split(id).0 == lang);
    same_language(language)
        .or_else(|| {
            COUNTRY_AS_LANGUAGE
                .iter()
                .find(|(country, _)| *country == language)
                .and_then(|(_, lang)| same_language(lang))
        })
        .or_else(|| {
            let region = maybe_region?;
            installed.iter().find(|id| split(id).1 == Some(region))
        })
}

/// Language and optional region subtags of a lowercased tag.
fn split(id: &LayoutId) -> (&str, Option<&str>) {
    let mut parts = id.as_str().splitn(2, '-');
    let language = parts.next().unwrap_or_default();
    (language, parts.next())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn country_code_for_ukrainian() {
        let installed = ids(&["en-us", "ru-ru", "uk-ua"]);
        assert_eq!(suggest("ua-UA", &installed), Some(id("uk-UA")));
        assert_eq!(suggest("ua", &installed), Some(id("uk-UA")));
        assert_eq!(suggest("by-BY", &ids(&["be-by"])), Some(id("be-BY")));
    }

    #[test]
    fn same_language_other_region_comes_first() {
        let installed = ids(&["en-us", "de-gb"]);
        assert_eq!(suggest("en-GB", &installed), Some(id("en-US")));
    }

    #[test]
    fn same_region_as_last_resort() {
        let installed = ids(&["en-us", "fr-ca"]);
        assert_eq!(suggest("xx-CA", &installed), Some(id("fr-CA")));
    }

    #[test]
    fn nothing_similar() {
        let installed = ids(&["en-us", "ru-ru"]);
        assert_eq!(suggest("de-DE", &installed), None);
        assert_eq!(suggest("ua-UA", &installed), None, "uk not installed");
        assert_eq!(suggest("de", &[]), None);
    }

    fn suggest(tag: &str, installed: &[LayoutId]) -> Option<LayoutId> {
        maybe_suggest_layout(&id(tag), installed).cloned()
    }

    fn ids(tags: &[&str]) -> Vec<LayoutId> {
        tags.iter().map(|t| id(t)).collect()
    }

    fn id(tag: &str) -> LayoutId {
        LayoutId::new(tag).unwrap()
    }
}
