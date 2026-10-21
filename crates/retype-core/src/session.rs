//! The session turns input events into buffer updates and commands.

use crate::buffer::KeyBuffer;
use crate::config::Config;
use crate::event::{InputEvent, Millis};
use crate::hotkey::HotkeyAction;
use crate::keystroke::{KeyClass, Modifiers, classify};
use crate::layout::{LayoutId, maybe_target_layout};
use crate::plan::{Conversion, ConversionPlan, PlanOptions, Scope, plan_conversion};

/// What the app should do after an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Ask the session for a plan with [`Session::plan`] and execute it.
    Convert(Scope),
    /// Convert the selected text via the clipboard (SPEC 5.3).
    ConvertSelection,
    /// Pause state changed; update the tray.
    PauseChanged { paused: bool },
}

/// State of one running retype instance.
#[derive(Debug)]
pub struct Session {
    config: Config,
    buffer: KeyBuffer,
    last_key: Option<Millis>,
    paused: bool,
    /// Hotkey key (and modifiers) that is down; its auto-repeat must not
    /// fire again.
    maybe_held_hotkey: Option<(u16, Modifiers)>,
}

impl Session {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            buffer: KeyBuffer::new(),
            last_key: None,
            paused: false,
            maybe_held_hotkey: None,
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Applies a reloaded config. The buffer is cleared.
    pub fn set_config(&mut self, config: Config) {
        self.config = config;
        self.reset();
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Number of recorded keys (for logging counts, never content).
    pub fn buffered_keys(&self) -> usize {
        self.buffer.len()
    }

    /// Feeds one event. Returns a command when a hotkey fired.
    pub fn handle(&mut self, event: InputEvent) -> Option<Command> {
        match event {
            InputEvent::MouseClick | InputEvent::FocusChanged => {
                self.reset();
                None
            }
            InputEvent::KeyUp { vk } => {
                if self.maybe_held_hotkey.is_some_and(|(held, _)| held == vk) {
                    self.maybe_held_hotkey = None;
                }
                None
            }
            InputEvent::Tick { time } => {
                if self.is_idle(time) {
                    self.reset();
                }
                None
            }
            InputEvent::KeyDown { key, mods, time } => {
                if let Some(action) = self.config.hotkeys.action_for(key.vk, mods) {
                    let pressed = (key.vk, mods);
                    if self.maybe_held_hotkey.replace(pressed) == Some(pressed) {
                        return None; // auto-repeat
                    }
                    return self.on_hotkey(action);
                }
                if classify(key.vk) != KeyClass::Modifier {
                    // A lost key-up must not swallow the next real press.
                    self.maybe_held_hotkey = None;
                }
                if self.paused {
                    return None;
                }
                if self.is_idle(time) {
                    self.reset();
                }
                self.last_key = Some(time);
                match classify(key.vk) {
                    KeyClass::Modifier => {}
                    _ if mods.is_command() => self.reset(),
                    KeyClass::Printable | KeyClass::Space => self.buffer.push(key),
                    KeyClass::Backspace => self.buffer.backspace(),
                    KeyClass::Reset => self.reset(),
                }
                None
            }
        }
    }

    /// Builds the plan for a [`Command::Convert`], given the foreground
    /// window's current layout, and updates the buffer to what will be on
    /// screen afterwards so that pressing the hotkey again converts back.
    pub fn plan(&mut self, scope: Scope, current: &LayoutId) -> Option<ConversionPlan> {
        if self.paused {
            return None;
        }
        let target = maybe_target_layout(&self.config.layouts, current, self.config.mode)?;
        let options = PlanOptions {
            keep_trailing_spaces: self.config.keep_trailing_spaces,
            switch_layout_after_convert: self.config.switch_layout_after_convert,
        };
        let Conversion { plan, buffer_after } =
            plan_conversion(&self.buffer, scope, current, target, options)?;
        self.buffer.replace(&buffer_after);
        Some(plan)
    }

    /// Forgets the recorded text, e.g. when a plan could not be executed and
    /// the screen may no longer match the buffer.
    pub fn clear(&mut self) {
        self.reset();
    }

    /// The layout to convert a selection into, from `current`.
    pub fn maybe_selection_target(&self, current: &LayoutId) -> Option<LayoutId> {
        maybe_target_layout(&self.config.layouts, current, self.config.mode).cloned()
    }

    /// Pauses or resumes, e.g. from the tray menu. The buffer is cleared.
    pub fn toggle_pause(&mut self) -> Command {
        self.paused = !self.paused;
        self.reset();
        Command::PauseChanged {
            paused: self.paused,
        }
    }

    fn on_hotkey(&mut self, action: HotkeyAction) -> Option<Command> {
        match action {
            HotkeyAction::TogglePause => Some(self.toggle_pause()),
            _ if self.paused => None,
            HotkeyAction::ConvertLastWord => Some(Command::Convert(Scope::LastWord)),
            HotkeyAction::ConvertLastLine => Some(Command::Convert(Scope::LastLine)),
            HotkeyAction::ConvertSelection => {
                // The paste replaces text we did not record.
                self.reset();
                Some(Command::ConvertSelection)
            }
        }
    }

    fn is_idle(&self, now: Millis) -> bool {
        let limit = Millis::from(self.config.idle_reset_seconds) * 1000;
        match self.last_key {
            Some(last) => limit > 0 && now.saturating_sub(last) >= limit,
            None => false,
        }
    }

    fn reset(&mut self) {
        self.buffer.clear();
        self.last_key = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keystroke::{Keystroke, vk};
    use crate::layout::SwitchMode;
    use crate::plan::PlanOp;

    const SHIFT: Modifiers = Modifiers {
        shift: true,
        ctrl: false,
        alt: false,
        win: false,
    };
    const CTRL: Modifiers = Modifiers {
        shift: false,
        ctrl: true,
        alt: false,
        win: false,
    };

    fn key(vk_code: u16, shift: bool) -> Keystroke {
        Keystroke {
            vk: vk_code,
            scan: vk_code,
            extended: false,
            shift,
        }
    }

    fn down(vk_code: u16, mods: Modifiers, time: Millis) -> InputEvent {
        InputEvent::KeyDown {
            key: key(vk_code, mods.shift),
            mods,
            time,
        }
    }

    /// Types ASCII text (upper case letters use Shift) at time 0.
    fn type_text(session: &mut Session, text: &str) {
        for c in text.chars() {
            let (vk_code, mods) = match c {
                ' ' => (vk::SPACE, Modifiers::NONE),
                'a'..='z' => (c.to_ascii_uppercase() as u16, Modifiers::NONE),
                _ => (c as u16, SHIFT),
            };
            assert_eq!(session.handle(down(vk_code, mods, 0)), None);
        }
    }

    fn id(tag: &str) -> LayoutId {
        LayoutId::new(tag).unwrap()
    }

    fn replayed(plan: &ConversionPlan) -> Vec<Keystroke> {
        plan.ops
            .iter()
            .find_map(|op| match op {
                PlanOp::Replay(keys) => Some(keys.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }

    #[test]
    fn records_typing_and_converts_last_word() {
        let mut s = Session::new(Config::default());
        type_text(&mut s, "ok Ghbdtn");
        assert_eq!(
            s.handle(down(vk::PAUSE, Modifiers::NONE, 0)),
            Some(Command::Convert(Scope::LastWord))
        );
        let plan = s.plan(Scope::LastWord, &id("en-US")).unwrap();
        assert_eq!(plan.ops[0], PlanOp::Backspace(6));
        assert_eq!(plan.ops[1], PlanOp::SwitchLayout(id("ru-RU")));
        let keys = replayed(&plan);
        assert_eq!(keys.len(), 6);
        assert!(keys[0].shift && !keys[1].shift);
    }

    #[test]
    fn second_press_toggles_back() {
        let mut s = Session::new(Config::default());
        type_text(&mut s, "ghbdtn");
        let first = s.plan(Scope::LastWord, &id("en-US")).unwrap();
        let second = s.plan(Scope::LastWord, &id("ru-RU")).unwrap();
        assert_eq!(second.ops[0], PlanOp::Backspace(6));
        assert_eq!(second.ops[1], PlanOp::SwitchLayout(id("en-US")));
        assert_eq!(replayed(&first), replayed(&second));
    }

    #[test]
    fn shift_pause_converts_line() {
        let mut s = Session::new(Config::default());
        type_text(&mut s, "ghbdtn vbh");
        assert_eq!(
            s.handle(down(vk::PAUSE, SHIFT, 0)),
            Some(Command::Convert(Scope::LastLine))
        );
        let plan = s.plan(Scope::LastLine, &id("en-US")).unwrap();
        assert_eq!(plan.ops[0], PlanOp::Backspace(10));
    }

    #[test]
    fn backspace_is_mirrored() {
        let mut s = Session::new(Config::default());
        type_text(&mut s, "abc");
        s.handle(down(vk::BACK, Modifiers::NONE, 0));
        assert_eq!(s.buffered_keys(), 2);
    }

    #[test]
    fn resets_clear_the_buffer() {
        let resets = [
            down(vk::RETURN, Modifiers::NONE, 0),
            down(vk::TAB, Modifiers::NONE, 0),
            down(0x25, Modifiers::NONE, 0), // Left arrow
            down(0x43, CTRL, 0),            // Ctrl+C
            InputEvent::MouseClick,
            InputEvent::FocusChanged,
        ];
        for event in resets {
            let mut s = Session::new(Config::default());
            type_text(&mut s, "abc");
            s.handle(event);
            assert_eq!(s.buffered_keys(), 0, "{event:?}");
        }
    }

    #[test]
    fn modifiers_alone_do_not_reset() {
        let mut s = Session::new(Config::default());
        type_text(&mut s, "abc");
        s.handle(down(vk::LSHIFT, SHIFT, 0));
        s.handle(down(vk::LCONTROL, CTRL, 0));
        s.handle(down(vk::CAPITAL, Modifiers::NONE, 0));
        assert_eq!(s.buffered_keys(), 3);
    }

    #[test]
    fn idle_timeout_clears_on_tick_and_on_next_key() {
        let mut s = Session::new(Config::default());
        s.handle(down(0x41, Modifiers::NONE, 1_000));
        s.handle(InputEvent::Tick { time: 30_999 });
        assert_eq!(s.buffered_keys(), 1);
        s.handle(InputEvent::Tick { time: 31_000 });
        assert_eq!(s.buffered_keys(), 0);

        s.handle(down(0x41, Modifiers::NONE, 40_000));
        s.handle(down(0x42, Modifiers::NONE, 80_000));
        assert_eq!(s.buffered_keys(), 1);
    }

    #[test]
    fn idle_reset_can_be_disabled() {
        let config = Config {
            idle_reset_seconds: 0,
            ..Config::default()
        };
        let mut s = Session::new(config);
        s.handle(down(0x41, Modifiers::NONE, 0));
        s.handle(InputEvent::Tick { time: u64::MAX });
        assert_eq!(s.buffered_keys(), 1);
    }

    #[test]
    fn pause_stops_recording_and_hotkeys() {
        let mut s = Session::new(Config::default());
        type_text(&mut s, "abc");
        assert_eq!(
            s.handle(down(vk::CANCEL, CTRL, 0)),
            Some(Command::PauseChanged { paused: true })
        );
        assert!(s.is_paused());
        assert_eq!(s.buffered_keys(), 0);
        type_text(&mut s, "abc");
        assert_eq!(s.buffered_keys(), 0);
        assert_eq!(s.handle(down(vk::PAUSE, Modifiers::NONE, 0)), None);
        assert_eq!(s.plan(Scope::LastWord, &id("en-US")), None);
        assert_eq!(
            s.handle(down(vk::PAUSE, CTRL, 0)),
            Some(Command::PauseChanged { paused: false })
        );
        type_text(&mut s, "abc");
        assert_eq!(s.buffered_keys(), 3);
    }

    #[test]
    fn toggle_pause_from_tray() {
        let mut s = Session::new(Config::default());
        type_text(&mut s, "abc");
        assert_eq!(s.toggle_pause(), Command::PauseChanged { paused: true });
        assert_eq!(s.buffered_keys(), 0);
        assert_eq!(s.toggle_pause(), Command::PauseChanged { paused: false });
        assert!(!s.is_paused());
    }

    #[test]
    fn selection_hotkey_and_target() {
        let config = Config {
            layouts: vec![id("en-US"), id("uk-UA"), id("ru-RU")],
            mode: SwitchMode::Cycle,
            ..Config::default()
        };
        let mut s = Session::new(config);
        type_text(&mut s, "abc");
        assert_eq!(
            s.handle(down(vk::SCROLL, Modifiers::NONE, 0)),
            Some(Command::ConvertSelection)
        );
        assert_eq!(s.buffered_keys(), 0);
        assert_eq!(s.maybe_selection_target(&id("uk-UA")), Some(id("ru-RU")));
    }

    #[test]
    fn hotkey_keys_are_not_recorded() {
        let mut s = Session::new(Config::default());
        type_text(&mut s, "abc");
        s.handle(down(vk::PAUSE, Modifiers::NONE, 0));
        assert_eq!(s.buffered_keys(), 3);
    }

    #[test]
    fn hotkey_auto_repeat_fires_once() {
        let mut s = Session::new(Config::default());
        let pause = down(vk::PAUSE, Modifiers::NONE, 0);
        let convert = Some(Command::Convert(Scope::LastWord));
        assert_eq!(s.handle(pause), convert);
        assert_eq!(s.handle(pause), None);
        assert_eq!(s.handle(pause), None);
        s.handle(InputEvent::KeyUp { vk: vk::PAUSE });
        assert_eq!(s.handle(pause), convert);
    }

    #[test]
    fn other_key_ends_hotkey_hold() {
        let mut s = Session::new(Config::default());
        let pause = down(vk::PAUSE, Modifiers::NONE, 0);
        s.handle(pause);
        s.handle(down(vk::LSHIFT, SHIFT, 0));
        assert_eq!(s.handle(pause), None, "modifiers keep the hold");
        type_text(&mut s, "a");
        assert_eq!(s.handle(pause), Some(Command::Convert(Scope::LastWord)));
    }

    #[test]
    fn clear_forgets_text() {
        let mut s = Session::new(Config::default());
        type_text(&mut s, "abc");
        s.clear();
        assert_eq!(s.buffered_keys(), 0);
    }

    #[test]
    fn nothing_to_convert_after_reset() {
        let mut s = Session::new(Config::default());
        type_text(&mut s, "abc");
        s.handle(InputEvent::FocusChanged);
        assert_eq!(s.plan(Scope::LastWord, &id("en-US")), None);
    }
}
