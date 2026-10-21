//! Converting selected text (SPEC 5.3): which layout it was typed in, which
//! layout to convert it to, and whether there is anything to do at all.

use std::hint::black_box;

use crate::config::Config;
use crate::layout::{LayoutId, LayoutTable, convert_text, maybe_target_layout};

/// Longer selections are left alone: converting a whole document by
/// accident is worse than doing nothing.
pub const MAX_SELECTION_CHARS: usize = 10_000;

/// The converted text and the layouts involved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionConversion {
    pub text: String,
    pub source: LayoutId,
    pub target: LayoutId,
}

/// Converts `text` between the configured layouts. `tables` holds the
/// installed layouts' tables; `current` is the foreground layout.
///
/// The source layout is the configured one whose characters the text uses
/// most, so `руддщ` selected while EN is active still becomes `hello`.
/// Returns `None` when the text is empty, too long, or would not change.
pub fn convert_selection(
    text: &str,
    current: &LayoutId,
    config: &Config,
    tables: &[(LayoutId, LayoutTable)],
) -> Option<SelectionConversion> {
    if text.is_empty() || text.chars().count() > MAX_SELECTION_CHARS {
        return None;
    }
    let candidates: Vec<(&LayoutId, &LayoutTable)> = config
        .layouts
        .iter()
        .filter_map(|id| Some((id, table_of(tables, id)?)))
        .collect();
    let source = maybe_source_layout(text, current, &candidates)?;
    let target = maybe_target_layout(&config.layouts, source, config.mode)?;
    let converted = convert_text(text, table_of(tables, source)?, table_of(tables, target)?);
    if converted == text {
        wipe_string(converted);
        return None;
    }
    Some(SelectionConversion {
        text: converted,
        source: source.clone(),
        target: target.clone(),
    })
}

/// Overwrites the string's bytes before freeing it (typed text should not
/// linger in freed memory, `.claude/rules/security.md`).
pub fn wipe_string(text: String) {
    let mut bytes = text.into_bytes();
    bytes.fill(0);
    black_box(&bytes); // keep the writes from being optimised away
}

/// Scores each candidate by the characters of `text` that it has and not
/// every other candidate has (digits and shared punctuation say nothing).
/// Ties prefer `current`, then the earlier candidate.
fn maybe_source_layout<'a>(
    text: &str,
    current: &LayoutId,
    candidates: &[(&'a LayoutId, &LayoutTable)],
) -> Option<&'a LayoutId> {
    let distinctive = |c: char| !candidates.iter().all(|(_, t)| t.position_of(c).is_some());
    let score = |table: &LayoutTable| {
        text.chars()
            .filter(|&c| distinctive(c) && table.position_of(c).is_some())
            .count()
    };
    let mut best: Option<(&LayoutId, usize)> = None;
    for &(id, table) in candidates {
        let points = score(table);
        let better = match best {
            None => points > 0,
            Some((_, top)) => points > top || (points == top && id == current),
        };
        if better {
            best = Some((id, points));
        }
    }
    best.map(|(id, _)| id)
}

fn table_of<'a>(tables: &'a [(LayoutId, LayoutTable)], id: &LayoutId) -> Option<&'a LayoutTable> {
    tables.iter().find(|(t, _)| t == id).map(|(_, table)| table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::SwitchMode;
    use crate::testing::{en, ru, uk};

    #[test]
    fn converts_english_typed_text_to_russian() {
        let c = convert_selection("Ghbdtn? vbh!", &id("en-US"), &config2(), &tables()).unwrap();
        assert_eq!(c.text, "Привет, мир!");
        assert_eq!(c.source, id("en-US"));
        assert_eq!(c.target, id("ru-RU"));
    }

    #[test]
    fn detects_russian_text_while_english_is_active() {
        let c = convert_selection("руддщ", &id("en-US"), &config2(), &tables()).unwrap();
        assert_eq!(c.text, "hello");
        assert_eq!(c.target, id("en-US"));
    }

    #[test]
    fn ukrainian_letters_pick_ukrainian() {
        let config = Config {
            layouts: vec![id("en-US"), id("ru-RU"), id("uk-UA")],
            mode: SwitchMode::Pair,
            ..Config::default()
        };
        let c = convert_selection("привіт", &id("en-US"), &config, &tables()).unwrap();
        assert_eq!(c.source, id("uk-UA"));
        assert_eq!(c.text, "ghbdsn");
    }

    #[test]
    fn ties_prefer_the_current_layout() {
        let config = Config {
            layouts: vec![id("en-US"), id("ru-RU"), id("uk-UA")],
            ..Config::default()
        };
        // Every letter exists in both ru and uk; both convert to en.
        let c = convert_selection("привет", &id("uk-UA"), &config, &tables()).unwrap();
        assert_eq!((c.source, c.text.as_str()), (id("uk-UA"), "ghbdtn"));
        let c = convert_selection("привет", &id("en-US"), &config, &tables()).unwrap();
        assert_eq!(c.source, id("ru-RU"));
    }

    #[test]
    fn nothing_to_do() {
        let config = config2();
        let tables = tables();
        let long = "a".repeat(MAX_SELECTION_CHARS + 1);
        for text in ["", "123 ,", "€€", long.as_str()] {
            assert_eq!(
                convert_selection(text, &id("en-US"), &config, &tables),
                None,
                "{:.10}",
                text
            );
        }
    }

    #[test]
    fn missing_table_means_no_conversion() {
        let only_en = vec![(id("en-US"), en())];
        assert_eq!(
            convert_selection("ghbdtn", &id("en-US"), &config2(), &only_en),
            None
        );
    }

    fn id(tag: &str) -> LayoutId {
        LayoutId::new(tag).unwrap()
    }

    fn config2() -> Config {
        Config::default() // en-US, ru-RU, pair
    }

    fn tables() -> Vec<(LayoutId, LayoutTable)> {
        vec![
            (id("en-US"), en()),
            (id("ru-RU"), ru()),
            (id("uk-UA"), uk()),
        ]
    }
}
