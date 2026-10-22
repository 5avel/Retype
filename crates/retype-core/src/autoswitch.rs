//! Autoswitch: after a word is finished, decide whether it was typed in the
//! wrong layout (ADR-015). Off by default (`[autoswitch] enabled`).

use std::collections::VecDeque;

use crate::config::Config;
use crate::keystroke::Keystroke;
use crate::layout::{KeyPos, LayoutId, LayoutTable, maybe_target_layout};
use crate::ngram::{Score, model_for};
use crate::selection::wipe_string;

/// Shorter words are too ambiguous to touch.
pub const MIN_LETTERS: usize = 3;
/// The word in the other layout must look like an ordinary word...
const MIN_TARGET_SCORE: f64 = -2.0;
/// ...and much more likely than what was typed.
const MIN_GAIN: f64 = 4.0;
/// When two targets are about equally good (Russian and Ukrainian share
/// most letters), the usual target from `layouts`/`mode` wins.
const PREFER_MARGIN: f64 = 0.75;

/// Words the user undid stay alone until exit. Only hashes of their key
/// positions are kept, at most this many (SPEC P2, ADR-015).
pub const MAX_REJECTED: usize = 128;

/// What autoswitch remembers between words.
#[derive(Debug, Default)]
pub struct AutoswitchMemory {
    /// The last autoswitch, while undoing it is still possible.
    maybe_undo: Option<Undo>,
    rejected: VecDeque<u64>,
}

#[derive(Debug)]
struct Undo {
    word: u64,
    original: LayoutId,
    /// Keys the autoswitch replaced, from the end of the line.
    span: usize,
}

impl AutoswitchMemory {
    /// Records an autoswitch of `word` away from `original` that replaced
    /// the last `span` keys of the line.
    pub fn switched(&mut self, word: &[Keystroke], original: &LayoutId, span: usize) {
        self.maybe_undo = Some(Undo {
            word: word_hash(word),
            original: original.clone(),
            span,
        });
    }

    /// The layout to convert back to and how many keys from the end of the
    /// line, if undoing the last autoswitch is still possible. Confirm with
    /// [`AutoswitchMemory::undone`].
    pub fn maybe_undo_target(&self) -> Option<(&LayoutId, usize)> {
        self.maybe_undo
            .as_ref()
            .map(|undo| (&undo.original, undo.span))
    }

    /// The undo was carried out: remember the word as rejected.
    pub fn undone(&mut self) {
        let Some(undo) = self.maybe_undo.take() else {
            return;
        };
        if self.rejected.len() == MAX_REJECTED {
            self.rejected.pop_front();
        }
        self.rejected.push_back(undo.word);
    }

    /// Forgets everything (pause, autoswitch turned off).
    pub fn clear(&mut self) {
        self.maybe_undo = None;
        self.rejected.clear();
    }

    /// Something else happened: undoing is no longer possible.
    pub fn forget_undo(&mut self) {
        self.maybe_undo = None;
    }

    pub fn is_rejected(&self, word: &[Keystroke]) -> bool {
        self.rejected.contains(&word_hash(word))
    }
}

/// FNV-1a over the key positions (case-insensitive: Shift is ignored).
fn word_hash(word: &[Keystroke]) -> u64 {
    word.iter().fold(0xcbf2_9ce4_8422_2325, |hash, key| {
        key.scan.to_le_bytes().iter().fold(hash, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
        })
    })
}

/// The layout `word` should have been typed in, if it clearly was not
/// `current`. `tables` are the installed layouts' character tables.
pub fn maybe_autoswitch_target(
    word: &[Keystroke],
    current: &LayoutId,
    config: &Config,
    tables: &[(LayoutId, LayoutTable)],
) -> Option<LayoutId> {
    let current_score = score_in(word, current, tables)?;
    let preferred = maybe_target_layout(&config.layouts, current, config.mode);
    let mut candidates: Vec<(&LayoutId, f64)> = config
        .layouts
        .iter()
        .filter(|id| *id != current)
        .filter_map(|id| Some((id, score_in(word, id, tables)?)))
        .filter(|(_, score)| {
            score.letters >= MIN_LETTERS
                && score.normalized >= MIN_TARGET_SCORE
                && score.normalized - current_score.normalized >= MIN_GAIN
        })
        .map(|(id, score)| (id, score.normalized))
        .collect();
    candidates.sort_by(|a, b| b.1.total_cmp(&a.1));
    let &(best, best_score) = candidates.first()?;
    let chosen = candidates
        .iter()
        .find(|(id, score)| Some(*id) == preferred && best_score - score < PREFER_MARGIN)
        .map_or(best, |(id, _)| *id);
    Some(chosen.clone())
}

/// The word as typed in `layout`, scored by that language's model.
pub(crate) fn score_in(
    word: &[Keystroke],
    layout: &LayoutId,
    tables: &[(LayoutId, LayoutTable)],
) -> Option<Score> {
    let language = layout.as_str().split('-').next()?;
    let model = model_for(language)?;
    let text = text_in(word, layout, tables)?;
    let score = model.score(&text);
    wipe_string(text);
    score
}

/// The characters `word` types in `layout` (unknown keys become spaces).
/// Callers wipe the result.
pub(crate) fn text_in(
    word: &[Keystroke],
    layout: &LayoutId,
    tables: &[(LayoutId, LayoutTable)],
) -> Option<String> {
    let (_, table) = tables.iter().find(|(id, _)| id == layout)?;
    Some(
        word.iter()
            .map(|key| {
                let pos = KeyPos {
                    scan: key.scan,
                    shift: key.shift,
                };
                table.char_at(pos).unwrap_or(' ') // unknown key: word boundary
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::SwitchMode;
    use crate::testing::{en, ru, uk};

    #[test]
    fn wrong_layout_words_are_switched() {
        let config = three_layouts();
        for (typed, want) in [
            ("ghbdtn", "ru-RU"),
            ("Ghbdtn", "ru-RU"),
            ("ltkf", "ru-RU"),
            (";bpym", "ru-RU"),
            ("pfdnhf", "ru-RU"),
            ("ghbdsn", "uk-UA"),
        ] {
            assert_eq!(target(typed, "en-US", &config), Some(id(want)), "{typed}");
        }
        assert_eq!(target_from_ru("еру", &config), Some(id("en-US")));
        assert_eq!(target_from_ru("цщкдв", &config), Some(id("en-US")));
    }

    #[test]
    fn real_words_and_jargon_stay() {
        let config = three_layouts();
        for typed in [
            "hello", "world", "the", "cargo", "git", "rust", "nginx", "kubectl", "https", "json",
            "xyz", "npm", "tmux", "ffmpeg", "btw", "ok",
        ] {
            assert_eq!(target(typed, "en-US", &config), None, "{typed}");
        }
        for typed in ["привет", "как", "дела", "сообщение"] {
            assert_eq!(target_from_ru(typed, &config), None, "{typed}");
        }
    }

    #[test]
    fn undo_memory_is_bounded() {
        let mut memory = AutoswitchMemory::default();
        let word = keys_typed("ghbdtn", &en());
        assert_eq!(memory.maybe_undo_target(), None);
        memory.switched(&word, &id("en-US"), 7);
        assert_eq!(memory.maybe_undo_target(), Some((&id("en-US"), 7)));
        assert!(!memory.is_rejected(&word), "not until the undo happened");
        memory.undone();
        assert_eq!(memory.maybe_undo_target(), None, "only once");
        assert!(memory.is_rejected(&word));
        let upper = keys_typed("GHBDTN", &en());
        assert!(memory.is_rejected(&upper), "case-insensitive");

        for i in 0..MAX_REJECTED {
            let other = [Keystroke {
                vk: 0,
                scan: 0x100 + i as u16,
                extended: false,
                shift: false,
            }];
            memory.switched(&other, &id("en-US"), 2);
            memory.undone();
        }
        assert!(!memory.is_rejected(&word), "oldest dropped");
        assert_eq!(memory.rejected.len(), MAX_REJECTED);
        memory.clear();
        assert!(memory.rejected.is_empty());
    }

    #[test]
    fn needs_models_and_tables() {
        let config = Config {
            layouts: vec![id("en-US"), id("de-DE")],
            ..Config::default()
        };
        assert_eq!(target("ghbdtn", "en-US", &config), None, "no de model");
        let only_en = [(id("en-US"), en())];
        let keys = keys_typed("ghbdtn", &en());
        assert_eq!(
            maybe_autoswitch_target(&keys, &id("en-US"), &three_layouts(), &only_en),
            None
        );
    }

    fn three_layouts() -> Config {
        Config {
            layouts: vec![id("en-US"), id("ru-RU"), id("uk-UA")],
            mode: SwitchMode::Pair,
            ..Config::default()
        }
    }

    fn target(typed: &str, current: &str, config: &Config) -> Option<LayoutId> {
        let keys = keys_typed(typed, &en());
        maybe_autoswitch_target(&keys, &id(current), config, &tables())
    }

    fn target_from_ru(typed: &str, config: &Config) -> Option<LayoutId> {
        let keys = keys_typed(typed, &ru());
        maybe_autoswitch_target(&keys, &id("ru-RU"), config, &tables())
    }

    /// The keystrokes that type `text` in the layout of `table`.
    fn keys_typed(text: &str, table: &LayoutTable) -> Vec<Keystroke> {
        text.chars()
            .map(|c| {
                let pos = table.position_of(c).unwrap();
                Keystroke {
                    vk: 0,
                    scan: pos.scan,
                    extended: false,
                    shift: pos.shift,
                }
            })
            .collect()
    }

    fn tables() -> Vec<(LayoutId, LayoutTable)> {
        vec![
            (id("en-US"), en()),
            (id("ru-RU"), ru()),
            (id("uk-UA"), uk()),
        ]
    }

    fn id(tag: &str) -> LayoutId {
        LayoutId::new(tag).unwrap()
    }
}
