//! Bounded record of the keys typed on the current line (SPEC P2, P3).

use std::collections::VecDeque;

use crate::MAX_BUFFER_KEYS;
use crate::keystroke::{KeyClass, Keystroke, classify, wipe_keys};

/// Keys typed since the last reset, oldest first.
///
/// Holds at most [`MAX_BUFFER_KEYS`] entries; on overflow the oldest key is
/// dropped, so the last word stays correct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyBuffer {
    keys: VecDeque<Keystroke>,
}

impl Default for KeyBuffer {
    /// Allocated once at full size, so growing never leaves copies of typed
    /// keys behind in freed memory.
    fn default() -> Self {
        Self {
            keys: VecDeque::with_capacity(MAX_BUFFER_KEYS),
        }
    }
}

impl KeyBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Records a printable key or a space.
    pub fn push(&mut self, key: Keystroke) {
        if self.keys.len() == MAX_BUFFER_KEYS {
            if let Some(oldest) = self.keys.front_mut() {
                *oldest = Keystroke::ZEROED;
            }
            self.keys.pop_front();
        }
        self.keys.push_back(key);
    }

    /// Mirrors a Backspace typed by the user.
    pub fn backspace(&mut self) {
        if let Some(last) = self.keys.back_mut() {
            *last = Keystroke::ZEROED;
        }
        self.keys.pop_back();
    }

    /// Forgets all keys, overwriting them first (best effort, SPEC P2).
    pub fn clear(&mut self) {
        self.keys
            .iter_mut()
            .for_each(|key| *key = Keystroke::ZEROED);
        std::hint::black_box(&self.keys);
        self.keys.clear();
    }

    /// Replaces the contents, e.g. with the keys a conversion replayed.
    pub fn replace(&mut self, keys: &[Keystroke]) {
        self.clear();
        for &key in keys {
            self.push(key);
        }
    }

    /// The whole line.
    pub fn line(&self) -> Vec<Keystroke> {
        self.keys.iter().copied().collect()
    }

    /// The last word and the spaces typed after it.
    ///
    /// Only Space separates words: punctuation keys are letters in other
    /// layouts (`,` is `б`, `;` is `ж`).
    pub fn last_word(&self) -> LastWord {
        let is_space = |k: &&Keystroke| classify(k.vk) == KeyClass::Space;
        let trailing_spaces = self.keys.iter().rev().take_while(is_space).count();
        let word = self
            .keys
            .iter()
            .rev()
            .skip(trailing_spaces)
            .take_while(|k| !is_space(k))
            .count();
        let start = self.keys.len() - trailing_spaces - word;
        let end = self.keys.len() - trailing_spaces;
        LastWord {
            word: self.keys.range(start..end).copied().collect(),
            spaces: self.keys.range(end..).copied().collect(),
        }
    }
}

/// Result of [`KeyBuffer::last_word`]. Wiped when dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastWord {
    pub word: Vec<Keystroke>,
    pub spaces: Vec<Keystroke>,
}

impl Drop for LastWord {
    fn drop(&mut self) {
        wipe_keys(&mut self.word);
        wipe_keys(&mut self.spaces);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keystroke::vk;

    fn key(vk_code: u16) -> Keystroke {
        Keystroke {
            vk: vk_code,
            scan: vk_code,
            extended: false,
            shift: false,
        }
    }

    fn buffer(text: &str) -> KeyBuffer {
        let mut b = KeyBuffer::new();
        for c in text.chars() {
            b.push(key(if c == ' ' { vk::SPACE } else { c as u16 }));
        }
        b
    }

    fn text(keys: &[Keystroke]) -> String {
        keys.iter()
            .map(|k| {
                if k.vk == vk::SPACE {
                    ' '
                } else {
                    k.vk as u8 as char
                }
            })
            .collect()
    }

    #[test]
    fn last_word_without_spaces() {
        let w = buffer("AB CD").last_word();
        assert_eq!(text(&w.word), "CD");
        assert!(w.spaces.is_empty());
    }

    #[test]
    fn last_word_with_trailing_spaces() {
        let w = buffer("AB CD  ").last_word();
        assert_eq!(text(&w.word), "CD");
        assert_eq!(w.spaces.len(), 2);
    }

    #[test]
    fn last_word_of_single_word_and_empty_buffer() {
        assert_eq!(text(&buffer("ABC").last_word().word), "ABC");
        let empty = KeyBuffer::new().last_word();
        assert!(empty.word.is_empty() && empty.spaces.is_empty());
        let spaces = buffer("   ").last_word();
        assert!(spaces.word.is_empty());
        assert_eq!(spaces.spaces.len(), 3);
    }

    #[test]
    fn backspace_removes_last_key() {
        let mut b = buffer("AB");
        b.backspace();
        assert_eq!(text(&b.line()), "A");
        b.backspace();
        b.backspace(); // on empty buffer: no-op
        assert!(b.is_empty());
    }

    #[test]
    fn bounded_drops_oldest() {
        let mut b = KeyBuffer::new();
        for _ in 0..MAX_BUFFER_KEYS {
            b.push(key(0x41));
        }
        b.push(key(0x42));
        assert_eq!(b.len(), MAX_BUFFER_KEYS);
        assert_eq!(b.line().last().map(|k| k.vk), Some(0x42));
    }

    #[test]
    fn replace_and_clear() {
        let mut b = buffer("AB");
        b.replace(&[key(0x43)]);
        assert_eq!(text(&b.line()), "C");
        b.clear();
        assert!(b.is_empty());
    }
}
