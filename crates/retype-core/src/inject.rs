//! Turns a [`ConversionPlan`] into the exact key events and waits the app's
//! executor performs, so the Windows side only calls the OS.

use crate::keystroke::{Keystroke, vk};
use crate::layout::LayoutId;
use crate::plan::{ConversionPlan, PlanOp};

/// Unassigned virtual key, pressed to "mask" a lone Alt or Win release so it
/// does not open the window menu or the Start menu.
pub const MASK_VK: u16 = 0xE8;

/// Left/right modifier keys whose physical state the executor reports.
pub const SIDED_MODIFIERS: [u16; 8] = [
    vk::LSHIFT,
    vk::RSHIFT,
    vk::LCONTROL,
    vk::RCONTROL,
    vk::LMENU,
    vk::RMENU,
    vk::LWIN,
    vk::RWIN,
];

/// Scan code of Backspace.
const BACKSPACE_SCAN: u16 = 0x0E;

/// Insert on the navigation block (extended, not the numpad 0 key).
const INSERT: KeyCode = KeyCode::Scan {
    scan: 0x52,
    extended: true,
};

/// One step for the executor, run in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Inject these key events (one `SendInput` call).
    Keys(Vec<KeyAction>),
    /// Switch the foreground window to this layout and wait until it applies.
    SwitchLayout(LayoutId),
    /// Give the target app time to process injected keys. Needed before a
    /// layout switch: posted messages overtake queued input, and keys are
    /// turned into characters with the layout active when they are processed.
    Settle,
}

/// A key to press or release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyAction {
    pub code: KeyCode,
    pub down: bool,
}

/// How the key is identified to the OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    /// By scan code, so the active layout decides the character.
    Scan { scan: u16, extended: bool },
    /// By virtual key; only for [`MASK_VK`], which has no scan code.
    Vk(u16),
}

/// Builds the steps for `plan`. `held` lists the [`SIDED_MODIFIERS`] the user
/// is physically holding (the hotkey's Shift, say): they are released first,
/// or they would modify the injected keys. Afterwards (or when a step fails)
/// the executor must run [`restore_modifiers`].
pub fn execution_steps(plan: &ConversionPlan, held: &[u16]) -> Vec<Step> {
    let mut steps = Steps::default();
    steps.keys(&release_modifiers(held));
    let mut replayed = false;
    for op in &plan.ops {
        match op {
            PlanOp::Backspace(n) => {
                let backspace = KeyCode::Scan {
                    scan: BACKSPACE_SCAN,
                    extended: false,
                };
                for _ in 0..*n {
                    steps.keys(&tap(backspace));
                }
            }
            PlanOp::SwitchLayout(id) => {
                if replayed {
                    steps.push(Step::Settle);
                }
                steps.push(Step::SwitchLayout(id.clone()));
            }
            PlanOp::Replay(keys) => {
                for key in keys {
                    steps.keys(&replay(key));
                }
                replayed |= !keys.is_empty();
            }
        }
    }
    steps.0
}

/// Releases the held [`SIDED_MODIFIERS`] so they do not modify injected
/// keys, masking Alt/Win first. Undo with [`restore_modifiers`].
pub fn release_modifiers(held: &[u16]) -> Vec<KeyAction> {
    let held = sided(held);
    let mut actions = Vec::new();
    if held.iter().any(|&m| is_alt_or_win(m)) {
        actions.extend(mask());
    }
    actions.extend(held.iter().map(|&m| modifier(m, false)));
    actions
}

/// Ctrl+Insert (copy). Unlike Ctrl+C it does not interrupt the running
/// program in a terminal when nothing is selected, and navigation keys mean
/// the same in every layout, so it goes by scan code.
pub fn copy_keys() -> Vec<KeyAction> {
    with_insert(vk::LCONTROL)
}

/// Shift+Insert (paste), see [`copy_keys`].
pub fn paste_keys() -> Vec<KeyAction> {
    with_insert(vk::LSHIFT)
}

/// Key-ups for the modifiers [`copy_keys`] and [`paste_keys`] press, sent
/// when an injection fails part way so none stays logically down.
pub fn shortcut_release() -> Vec<KeyAction> {
    [vk::LCONTROL, vk::LSHIFT]
        .into_iter()
        .map(|m| modifier(m, false))
        .collect()
}

/// Presses again the modifiers released by [`execution_steps`] that the user
/// still holds. One let go of during the conversion stays up: its physical
/// key-up found it already released, so pressing it would leave it stuck.
pub fn restore_modifiers(held_before: &[u16], held_now: &[u16]) -> Vec<KeyAction> {
    let restore: Vec<u16> = sided(held_before)
        .into_iter()
        .filter(|m| held_now.contains(m))
        .collect();
    let mut actions: Vec<KeyAction> = restore.iter().map(|&m| modifier(m, true)).collect();
    if restore.iter().any(|&m| is_alt_or_win(m)) {
        actions.extend(mask());
    }
    actions
}

/// Accumulates steps, merging adjacent key events into one `Keys` step.
#[derive(Default)]
struct Steps(Vec<Step>);

impl Steps {
    fn keys(&mut self, actions: &[KeyAction]) {
        if actions.is_empty() {
            return;
        }
        if let Some(Step::Keys(last)) = self.0.last_mut() {
            last.extend_from_slice(actions);
        } else {
            self.0.push(Step::Keys(actions.to_vec()));
        }
    }

    fn push(&mut self, step: Step) {
        self.0.push(step);
    }
}

/// The sided modifiers in `keys`, in [`SIDED_MODIFIERS`] order.
fn sided(keys: &[u16]) -> Vec<u16> {
    SIDED_MODIFIERS
        .into_iter()
        .filter(|m| keys.contains(m))
        .collect()
}

fn with_insert(m: u16) -> Vec<KeyAction> {
    let mut actions = vec![modifier(m, true)];
    actions.extend(tap(INSERT));
    actions.push(modifier(m, false));
    actions
}

fn tap(code: KeyCode) -> [KeyAction; 2] {
    [
        KeyAction { code, down: true },
        KeyAction { code, down: false },
    ]
}

fn mask() -> [KeyAction; 2] {
    tap(KeyCode::Vk(MASK_VK))
}

fn replay(key: &Keystroke) -> Vec<KeyAction> {
    let code = KeyCode::Scan {
        scan: key.scan,
        extended: key.extended,
    };
    if key.shift {
        let shift = modifier(vk::LSHIFT, true).code;
        vec![
            KeyAction {
                code: shift,
                down: true,
            },
            KeyAction { code, down: true },
            KeyAction { code, down: false },
            KeyAction {
                code: shift,
                down: false,
            },
        ]
    } else {
        tap(code).to_vec()
    }
}

fn is_alt_or_win(m: u16) -> bool {
    matches!(m, vk::LMENU | vk::RMENU | vk::LWIN | vk::RWIN)
}

/// Scan codes of the sided modifiers (set 1, as Windows reports them).
fn modifier(m: u16, down: bool) -> KeyAction {
    let (scan, extended) = match m {
        vk::LSHIFT => (0x2A, false),
        vk::RSHIFT => (0x36, false),
        vk::LCONTROL => (0x1D, false),
        vk::RCONTROL => (0x1D, true),
        vk::LMENU => (0x38, false),
        vk::RMENU => (0x38, true),
        vk::LWIN => (0x5B, true),
        _ => (0x5C, true), // RWIN: callers pass only SIDED_MODIFIERS
    };
    KeyAction {
        code: KeyCode::Scan { scan, extended },
        down,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: Keystroke = Keystroke {
        vk: 0x41,
        scan: 0x1E,
        extended: false,
        shift: false,
    };
    const MASK_DOWN: KeyAction = KeyAction {
        code: KeyCode::Vk(MASK_VK),
        down: true,
    };
    const SHIFTED_B: Keystroke = Keystroke {
        vk: 0x42,
        scan: 0x30,
        extended: false,
        shift: true,
    };

    #[test]
    fn word_conversion_steps() {
        let p = plan(vec![
            PlanOp::Backspace(2),
            PlanOp::SwitchLayout(id("ru-RU")),
            PlanOp::Replay(vec![A, SHIFTED_B]),
        ]);
        let steps = execution_steps(&p, &[]);
        assert_eq!(steps.len(), 3);
        assert_eq!(
            keys(&steps[0]),
            [
                scan(0x0E, false, true),
                scan(0x0E, false, false),
                scan(0x0E, false, true),
                scan(0x0E, false, false),
            ]
        );
        assert_eq!(steps[1], Step::SwitchLayout(id("ru-RU")));
        assert_eq!(
            keys(&steps[2]),
            [
                scan(0x1E, false, true),
                scan(0x1E, false, false),
                scan(0x2A, false, true),
                scan(0x30, false, true),
                scan(0x30, false, false),
                scan(0x2A, false, false),
            ]
        );
    }

    #[test]
    fn extended_keys_keep_the_flag() {
        let divide = Keystroke {
            vk: 0x6F,
            scan: 0x35,
            extended: true,
            shift: false,
        };
        let steps = execution_steps(&plan(vec![PlanOp::Replay(vec![divide])]), &[]);
        assert_eq!(keys(&steps[0])[0], scan(0x35, true, true));
    }

    #[test]
    fn settles_before_switching_back() {
        let p = plan(vec![
            PlanOp::Backspace(1),
            PlanOp::SwitchLayout(id("ru-RU")),
            PlanOp::Replay(vec![A]),
            PlanOp::SwitchLayout(id("en-US")),
        ]);
        let steps = execution_steps(&p, &[]);
        assert_eq!(steps[3], Step::Settle);
        assert_eq!(steps[4], Step::SwitchLayout(id("en-US")));
        assert_eq!(steps.len(), 5);
    }

    #[test]
    fn copy_and_paste_shortcuts() {
        let insert = |down| scan(0x52, true, down);
        let ctrl = |down| scan(0x1D, false, down);
        let shift = |down| scan(0x2A, false, down);
        assert_eq!(
            copy_keys(),
            [ctrl(true), insert(true), insert(false), ctrl(false)]
        );
        assert_eq!(
            paste_keys(),
            [shift(true), insert(true), insert(false), shift(false)]
        );
        assert_eq!(shortcut_release(), [ctrl(false), shift(false)]);
    }

    #[test]
    fn release_without_held_modifiers_is_empty() {
        assert_eq!(release_modifiers(&[]), []);
        assert_eq!(release_modifiers(&[vk::RSHIFT]), [scan(0x36, false, false)]);
    }

    #[test]
    fn no_settle_without_replay() {
        let p = plan(vec![
            PlanOp::Backspace(1),
            PlanOp::SwitchLayout(id("ru-RU")),
        ]);
        let steps = execution_steps(&p, &[]);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[1], Step::SwitchLayout(id("ru-RU")));
    }

    #[test]
    fn no_empty_key_steps() {
        let p = plan(vec![
            PlanOp::Backspace(0),
            PlanOp::SwitchLayout(id("ru-RU")),
            PlanOp::Replay(vec![]),
        ]);
        assert_eq!(
            execution_steps(&p, &[]),
            vec![Step::SwitchLayout(id("ru-RU"))]
        );
    }

    mod held_modifiers {
        use super::*;

        #[test]
        fn shift_is_released_and_restored() {
            let p = plan(vec![PlanOp::Backspace(1)]);
            let steps = execution_steps(&p, &[vk::RSHIFT]);
            assert_eq!(
                keys(&steps[0]),
                [
                    scan(0x36, false, false),
                    scan(0x0E, false, true),
                    scan(0x0E, false, false),
                ]
            );
            assert_eq!(
                restore_modifiers(&[vk::RSHIFT], &[vk::RSHIFT]),
                [scan(0x36, false, true)]
            );
        }

        #[test]
        fn released_during_conversion_is_not_restored() {
            let held = [vk::LCONTROL, vk::LSHIFT];
            assert_eq!(
                restore_modifiers(&held, &[vk::LCONTROL]),
                [scan(0x1D, false, true)]
            );
            assert_eq!(restore_modifiers(&held, &[]), []);
            assert_eq!(restore_modifiers(&[], &[vk::LSHIFT]), [], "not ours");
        }

        #[test]
        fn alt_and_win_are_masked() {
            let p = plan(vec![PlanOp::SwitchLayout(id("ru-RU"))]);
            let held = [vk::LWIN, vk::LCONTROL];
            let before = keys(&execution_steps(&p, &held)[0]).to_vec();
            assert_eq!(before[0], MASK_DOWN);
            assert_eq!(before[2], scan(0x1D, false, false));
            assert_eq!(before[3], scan(0x5B, true, false));
            let after = restore_modifiers(&held, &held);
            assert_eq!(after[0], scan(0x1D, false, true));
            assert_eq!(after[1], scan(0x5B, true, true));
            assert_eq!(after[2], MASK_DOWN);
        }

        #[test]
        fn lone_alt_is_masked() {
            let p = plan(vec![PlanOp::Backspace(1)]);
            let steps = execution_steps(&p, &[vk::LMENU]);
            assert_eq!(keys(&steps[0])[0], MASK_DOWN);
            assert_eq!(keys(&steps[0])[2], scan(0x38, false, false));
            assert_eq!(restore_modifiers(&[vk::LMENU], &[vk::LMENU])[1], MASK_DOWN);
        }

        #[test]
        fn non_modifiers_are_ignored() {
            let p = plan(vec![PlanOp::Backspace(1)]);
            assert_eq!(
                execution_steps(&p, &[vk::PAUSE, vk::SHIFT]),
                execution_steps(&p, &[])
            );
        }
    }

    fn id(tag: &str) -> LayoutId {
        LayoutId::new(tag).unwrap()
    }

    fn plan(ops: Vec<PlanOp>) -> ConversionPlan {
        ConversionPlan { ops }
    }

    fn scan(scan: u16, extended: bool, down: bool) -> KeyAction {
        KeyAction {
            code: KeyCode::Scan { scan, extended },
            down,
        }
    }

    fn keys(step: &Step) -> &[KeyAction] {
        match step {
            Step::Keys(actions) => actions,
            other => panic!("expected keys, got {other:?}"),
        }
    }
}
