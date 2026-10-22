//! Double-tap detection for a single key (double right Shift).

use crate::event::Millis;

/// Longest tap (press to release) and longest gap between the two taps.
pub const DOUBLE_TAP_WINDOW: Millis = 400;

/// Recognises two quick taps of one key with nothing pressed in between.
///
/// It fires on the second *release*, so a tap followed by Shift+letter
/// (Shift used as a modifier) never fires.
#[derive(Debug, Clone)]
pub struct DoubleTap {
    key: u16,
    /// When the key went down, while it is held.
    maybe_down: Option<Millis>,
    /// When a first clean tap ended.
    maybe_first_tap: Option<Millis>,
}

impl DoubleTap {
    pub fn new(key: u16) -> Self {
        Self {
            key,
            maybe_down: None,
            maybe_first_tap: None,
        }
    }

    /// Feeds every key press. Any other key breaks the sequence.
    pub fn key_down(&mut self, vk: u16, time: Millis) {
        if vk != self.key {
            self.reset();
            return;
        }
        if self.maybe_down.is_some() {
            return; // auto-repeat while held
        }
        let too_late = self
            .maybe_first_tap
            .is_some_and(|tap| time.saturating_sub(tap) > DOUBLE_TAP_WINDOW);
        if too_late {
            self.maybe_first_tap = None;
        }
        self.maybe_down = Some(time);
    }

    /// Feeds every key release. Returns true when this completes a double tap.
    pub fn key_up(&mut self, vk: u16, time: Millis) -> bool {
        if vk != self.key {
            return false;
        }
        let Some(down) = self.maybe_down.take() else {
            return false;
        };
        if time.saturating_sub(down) > DOUBLE_TAP_WINDOW {
            self.maybe_first_tap = None; // held: used as a modifier
            return false;
        }
        if self.maybe_first_tap.take().is_some() {
            return true;
        }
        self.maybe_first_tap = Some(time);
        false
    }

    /// Forgets any partial sequence (mouse click, focus change).
    pub fn reset(&mut self) {
        self.maybe_down = None;
        self.maybe_first_tap = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: u16 = 0xA1;
    const OTHER: u16 = 0x41;

    #[test]
    fn two_quick_taps_fire_once() {
        let mut d = DoubleTap::new(KEY);
        assert!(!tap(&mut d, 0, 80));
        assert!(tap(&mut d, 200, 260));
        assert!(!tap(&mut d, 300, 360), "a third tap starts over");
    }

    #[test]
    fn slow_gap_does_not_fire() {
        let mut d = DoubleTap::new(KEY);
        tap(&mut d, 0, 80);
        assert!(!tap(&mut d, 600, 650));
        assert!(tap(&mut d, 800, 850), "the slow tap counts as a first tap");
    }

    #[test]
    fn held_key_is_not_a_tap() {
        let mut d = DoubleTap::new(KEY);
        tap(&mut d, 0, 80);
        assert!(!tap(&mut d, 200, 900));
    }

    #[test]
    fn auto_repeat_is_ignored() {
        let mut d = DoubleTap::new(KEY);
        tap(&mut d, 0, 80);
        d.key_down(KEY, 150);
        d.key_down(KEY, 180);
        assert!(d.key_up(KEY, 250));
    }

    #[test]
    fn other_keys_break_the_sequence() {
        let mut d = DoubleTap::new(KEY);
        tap(&mut d, 0, 80);
        d.key_down(OTHER, 100);
        assert!(!tap(&mut d, 150, 200));

        // Shift used as a modifier: Shift down, letter, Shift up.
        let mut d = DoubleTap::new(KEY);
        tap(&mut d, 0, 80);
        d.key_down(KEY, 120);
        d.key_down(OTHER, 140);
        assert!(!d.key_up(KEY, 200));
    }

    #[test]
    fn reset_forgets_the_first_tap() {
        let mut d = DoubleTap::new(KEY);
        tap(&mut d, 0, 80);
        d.reset();
        assert!(!tap(&mut d, 150, 200));
    }

    fn tap(d: &mut DoubleTap, down: Millis, up: Millis) -> bool {
        d.key_down(KEY, down);
        d.key_up(KEY, up)
    }
}
