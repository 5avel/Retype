//! Keyboard layouts as data: identifiers, target choice, character tables.

use std::collections::HashMap;
use std::fmt;

use serde::Deserialize;

/// A layout identified by its language tag, e.g. `en-US`.
///
/// Comparison is case-insensitive; the tag is stored lowercased.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LayoutId(String);

impl LayoutId {
    /// Accepts tags made of ASCII letters, digits and `-`, like `en-US`.
    pub fn new(tag: &str) -> Option<Self> {
        let valid = !tag.is_empty()
            && !tag.starts_with('-')
            && !tag.ends_with('-')
            && tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
        valid.then(|| Self(tag.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for LayoutId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// How the target layout is chosen when more than two are configured.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SwitchMode {
    /// Current layout ⇄ the first other layout in the list (ADR-006).
    #[default]
    Pair,
    /// The next layout in the list, wrapping around.
    Cycle,
}

/// Picks the layout to convert to from `current`.
///
/// If `current` is not in the list, the first listed layout is the target.
/// Returns `None` when there is nothing to switch to.
pub fn maybe_target_layout<'a>(
    layouts: &'a [LayoutId],
    current: &LayoutId,
    mode: SwitchMode,
) -> Option<&'a LayoutId> {
    let position = layouts.iter().position(|l| l == current);
    match (mode, position) {
        (_, None) => layouts.first(),
        (SwitchMode::Pair, Some(_)) => layouts.iter().find(|l| *l != current),
        (SwitchMode::Cycle, Some(i)) => {
            let next = &layouts[(i + 1) % layouts.len()];
            (next != current).then_some(next)
        }
    }
}

/// Configured layouts that are not among the `installed` ones.
pub fn missing_layouts<'a>(
    configured: &'a [LayoutId],
    installed: &[LayoutId],
) -> Vec<&'a LayoutId> {
    configured
        .iter()
        .filter(|id| !installed.contains(id))
        .collect()
}

/// Scan codes of the keys that type characters: the four main rows plus the
/// extra ISO key next to left Shift (0x56). Space is the same everywhere.
pub const CHARACTER_SCANS: [std::ops::RangeInclusive<u16>; 5] = [
    0x02..=0x0D,
    0x10..=0x1B,
    0x1E..=0x29,
    0x2B..=0x35,
    0x56..=0x56,
];

/// A physical key position: scan code plus Shift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyPos {
    pub scan: u16,
    pub shift: bool,
}

/// Which character each key position produces in one layout.
///
/// Built by the app (via `ToUnicodeEx`) or by tests from fixed data.
#[derive(Debug, Clone, Default)]
pub struct LayoutTable {
    by_pos: HashMap<KeyPos, char>,
    by_char: HashMap<char, KeyPos>,
}

impl LayoutTable {
    /// Builds a table. If several positions produce the same character, the
    /// first one wins (callers list unshifted positions first).
    pub fn from_entries(entries: impl IntoIterator<Item = (KeyPos, char)>) -> Self {
        let mut table = Self::default();
        for (pos, c) in entries {
            table.by_pos.entry(pos).or_insert(c);
            table.by_char.entry(c).or_insert(pos);
        }
        table
    }

    /// Builds a table by asking `probe` which character each key of
    /// [`CHARACTER_SCANS`] types, unshifted positions first. Keys that type
    /// nothing, a control character or a dead key (`None`) are skipped.
    pub fn from_probe(mut probe: impl FnMut(KeyPos) -> Option<char>) -> Self {
        let mut entries = Vec::new();
        for shift in [false, true] {
            for scan in CHARACTER_SCANS.into_iter().flatten() {
                let pos = KeyPos { scan, shift };
                if let Some(c) = probe(pos).filter(|c| !c.is_control()) {
                    entries.push((pos, c));
                }
            }
        }
        Self::from_entries(entries)
    }

    pub fn char_at(&self, pos: KeyPos) -> Option<char> {
        self.by_pos.get(&pos).copied()
    }

    pub fn position_of(&self, c: char) -> Option<KeyPos> {
        self.by_char.get(&c).copied()
    }

    pub fn is_empty(&self) -> bool {
        self.by_pos.is_empty()
    }
}

/// Re-types `text` as if its keys had been pressed in the `to` layout.
///
/// Characters missing from either table are kept as they are (SPEC 5.3).
pub fn convert_text(text: &str, from: &LayoutTable, to: &LayoutTable) -> String {
    text.chars()
        .map(|c| {
            from.position_of(c)
                .and_then(|pos| to.char_at(pos))
                .unwrap_or(c)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{en, ru, uk};

    fn ids(tags: &[&str]) -> Vec<LayoutId> {
        tags.iter().map(|t| LayoutId::new(t).unwrap()).collect()
    }

    fn id(tag: &str) -> LayoutId {
        LayoutId::new(tag).unwrap()
    }

    #[test]
    fn layout_id_is_case_insensitive_and_validated() {
        assert_eq!(id("en-US"), id("EN-us"));
        assert_eq!(id("ru-RU").to_string(), "ru-ru");
        for bad in ["", "-en", "en-", "en US", "en_US", "рус"] {
            assert!(LayoutId::new(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn pair_mode_toggles() {
        let list = ids(&["en-US", "ru-RU"]);
        let to = |cur| maybe_target_layout(&list, &id(cur), SwitchMode::Pair).cloned();
        assert_eq!(to("en-US"), Some(id("ru-RU")));
        assert_eq!(to("ru-RU"), Some(id("en-US")));
        assert_eq!(to("de-DE"), Some(id("en-US")));
    }

    #[test]
    fn pair_mode_with_three_layouts_uses_first_other() {
        let list = ids(&["en-US", "uk-UA", "ru-RU"]);
        let to = |cur| maybe_target_layout(&list, &id(cur), SwitchMode::Pair).cloned();
        assert_eq!(to("en-US"), Some(id("uk-UA")));
        assert_eq!(to("uk-UA"), Some(id("en-US")));
        assert_eq!(to("ru-RU"), Some(id("en-US")));
    }

    #[test]
    fn cycle_mode_wraps() {
        let list = ids(&["en-US", "uk-UA", "ru-RU"]);
        let to = |cur| maybe_target_layout(&list, &id(cur), SwitchMode::Cycle).cloned();
        assert_eq!(to("en-US"), Some(id("uk-UA")));
        assert_eq!(to("uk-UA"), Some(id("ru-RU")));
        assert_eq!(to("ru-RU"), Some(id("en-US")));
    }

    #[test]
    fn no_target_with_single_layout() {
        let list = ids(&["en-US"]);
        assert_eq!(
            maybe_target_layout(&list, &id("en-US"), SwitchMode::Pair),
            None
        );
        assert_eq!(
            maybe_target_layout(&list, &id("en-US"), SwitchMode::Cycle),
            None
        );
        assert_eq!(
            maybe_target_layout(&[], &id("en-US"), SwitchMode::Pair),
            None
        );
    }

    #[test]
    fn probe_builds_the_same_table() {
        let fixture = ru();
        let table = LayoutTable::from_probe(|pos| fixture.char_at(pos));
        assert_eq!(convert_text("ghbdtn", &en(), &table), "привет");
        assert_eq!(
            table.position_of(','),
            Some(KeyPos {
                scan: 0x35,
                shift: true
            })
        );
    }

    #[test]
    fn probe_skips_control_characters() {
        let table = LayoutTable::from_probe(|pos| (pos.scan == 0x1C).then_some('\r'));
        assert!(table.is_empty());
        let table = LayoutTable::from_probe(|_| Some('\u{1b}'));
        assert!(table.is_empty());
    }

    #[test]
    fn reports_missing_layouts() {
        let configured = ids(&["en-US", "uk-UA", "ru-RU"]);
        let installed = ids(&["ru-ru", "en-us", "de-de"]);
        assert_eq!(missing_layouts(&configured, &installed), [&id("uk-UA")]);
    }

    #[test]
    fn converts_en_to_ru_and_back() {
        assert_eq!(convert_text("ghbdtn", &en(), &ru()), "привет");
        assert_eq!(convert_text("Ghbdtn? vbh!", &en(), &ru()), "Привет, мир!");
        assert_eq!(convert_text("руддщ", &ru(), &en()), "hello");
        assert_eq!(convert_text("Ckjdj ;bpym", &en(), &ru()), "Слово жизнь");
    }

    #[test]
    fn converts_en_to_uk() {
        assert_eq!(convert_text("ghbdsn", &en(), &uk()), "привіт");
        assert_eq!(convert_text("`]\\", &en(), &uk()), "'їґ");
    }

    #[test]
    fn keeps_unknown_characters() {
        assert_eq!(convert_text("ab 12\t€\n", &en(), &ru()), "фи 12\t€\n");
        assert_eq!(convert_text("x", &LayoutTable::default(), &ru()), "x");
    }

    #[test]
    fn duplicate_characters_prefer_first_position() {
        // '.' is unshifted on 0x35 in ru; ',' only shifted there.
        let table = ru();
        assert_eq!(
            table.position_of('.'),
            Some(KeyPos {
                scan: 0x35,
                shift: false
            })
        );
        assert_eq!(
            table.char_at(KeyPos {
                scan: 0x35,
                shift: true
            }),
            Some(',')
        );
    }
}
