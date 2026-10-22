//! Which keys an autoswitch converts: the word that triggered it plus the
//! short words right before it that read better in the target layout
//! (`рш ьн тфьу` → `hi my name`). Short words are never judged on their own
//! (ADR-015), only together with a long word that was clearly mistyped.

use crate::autoswitch::{score_in, text_in};
use crate::keystroke::{KeyClass, Keystroke, classify};
use crate::layout::{LayoutId, LayoutTable};

/// Words of at most this many keys may join a correction.
const SHORT_WORD_KEYS: usize = 2;

/// The only one-letter words of each language: other single letters are not
/// words, so they read worse.
const ONE_LETTER_WORDS: [(&str, &str); 3] = [("en", "ai"), ("ru", "аивкосуя"), ("uk", "аівзоуйя")];

/// Index in `line` of the first key to convert when the last word switches
/// from `current` to `target`.
pub fn autoswitch_start(
    line: &[Keystroke],
    current: &LayoutId,
    target: &LayoutId,
    tables: &[(LayoutId, LayoutTable)],
) -> usize {
    let words = word_ranges(line);
    let Some(&(mut start, _)) = words.last() else {
        return line.len();
    };
    for &(word_start, word_end) in words.iter().rev().skip(1) {
        let word = &line[word_start..word_end];
        if word.len() > SHORT_WORD_KEYS || !reads_better(word, current, target, tables) {
            break;
        }
        start = word_start;
    }
    start
}

/// `[start, end)` key ranges of the words in `line` (runs between spaces).
fn word_ranges(line: &[Keystroke]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut maybe_start = None;
    for (i, key) in line.iter().enumerate() {
        let space = classify(key.vk) == KeyClass::Space;
        match (space, maybe_start) {
            (false, None) => maybe_start = Some(i),
            (true, Some(start)) => {
                ranges.push((start, i));
                maybe_start = None;
            }
            _ => {}
        }
    }
    if let Some(start) = maybe_start {
        ranges.push((start, line.len()));
    }
    ranges
}

/// A short word reads better in `target` than in `current`.
fn reads_better(
    word: &[Keystroke],
    current: &LayoutId,
    target: &LayoutId,
    tables: &[(LayoutId, LayoutTable)],
) -> bool {
    if word.len() == 1 {
        return is_one_letter_word(word, target, tables)
            && !is_one_letter_word(word, current, tables);
    }
    match (
        score_in(word, current, tables),
        score_in(word, target, tables),
    ) {
        (Some(was), Some(now)) => now.normalized > was.normalized,
        (None, Some(now)) => now.letters == word.len(), // e.g. `,s` → `бы`
        _ => false,
    }
}

fn is_one_letter_word(
    word: &[Keystroke],
    layout: &LayoutId,
    tables: &[(LayoutId, LayoutTable)],
) -> bool {
    let Some(language) = layout.as_str().split('-').next() else {
        return false;
    };
    let Some((_, letters)) = ONE_LETTER_WORDS.iter().find(|(lang, _)| *lang == language) else {
        return false;
    };
    text_in(word, layout, tables)
        .and_then(|text| text.chars().next())
        .is_some_and(|c| c.to_lowercase().all(|lower| letters.contains(lower)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keystroke::vk;
    use crate::testing::{en, ru, uk};

    #[test]
    fn short_words_before_join() {
        // "hi my name " typed in RU: рш ьн тфьу
        let line = typed_ru("рш ьн тфьу ");
        assert_eq!(start(&line), 0);
    }

    #[test]
    fn one_letter_words() {
        // "i am " typed in RU: ш фь — ш is no Russian word, i is English
        assert_eq!(start(&typed_ru("ш фьфяштп ")), 0);
        // "я" is a Russian word: it stays out
        let line = typed_ru("я цщкдв ");
        assert_eq!(start(&line), 2);
    }

    #[test]
    fn stops_at_long_or_fitting_words() {
        // "привет" is a fine Russian word: the run stops there.
        let line = typed_ru("привет рш тфьу ");
        assert_eq!(start(&line), 7);
        // A Russian 2-letter word that also fits stays: "не тфьу"
        let line = typed_ru("не тфьу ");
        assert_eq!(start(&line), 3);
    }

    #[test]
    fn single_word_line() {
        assert_eq!(start(&typed_ru("тфьу ")), 0);
        assert_eq!(start(&[]), 0);
    }

    fn start(line: &[Keystroke]) -> usize {
        autoswitch_start(line, &id("ru-RU"), &id("en-US"), &tables())
    }

    /// Keys that type `text` in ru-RU (spaces as Space keys).
    fn typed_ru(text: &str) -> Vec<Keystroke> {
        let table = ru();
        text.chars()
            .map(|c| {
                if c == ' ' {
                    return Keystroke {
                        vk: vk::SPACE,
                        scan: 0x39,
                        extended: false,
                        shift: false,
                    };
                }
                let pos = table.position_of(c).unwrap();
                Keystroke {
                    vk: 0x41,
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
