//! Conversion plans: what the app must do to retype text in another layout.

use crate::buffer::KeyBuffer;
use crate::keystroke::{KeyClass, Keystroke, classify};
use crate::layout::LayoutId;

/// Which recorded text to convert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    LastWord,
    LastLine,
}

/// One step for the app's executor, run in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanOp {
    /// Press Backspace this many times.
    Backspace(usize),
    /// Switch the foreground window to this layout and wait until it applies.
    SwitchLayout(LayoutId),
    /// Press these keys by scan code, with their recorded Shift state.
    Replay(Vec<Keystroke>),
}

/// Ordered steps that retype text in another layout (ADR-003).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversionPlan {
    pub ops: Vec<PlanOp>,
}

/// Options that shape a plan, taken from the config.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanOptions {
    pub keep_trailing_spaces: bool,
    pub switch_layout_after_convert: bool,
}

/// A plan plus the buffer contents that will be on screen after it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conversion {
    pub plan: ConversionPlan,
    pub buffer_after: Vec<Keystroke>,
}

/// Builds the plan to retype `scope` from `current` into `target`.
///
/// Returns `None` when there is nothing to convert. Running the same plan
/// again with the layouts swapped converts back, which is how the second
/// hotkey press toggles (SPEC 5.2).
pub fn plan_conversion(
    buffer: &KeyBuffer,
    scope: Scope,
    current: &LayoutId,
    target: &LayoutId,
    options: PlanOptions,
) -> Option<Conversion> {
    let mut line = buffer.line();
    let (erase, replay) = match scope {
        Scope::LastLine => (line.len(), line.clone()),
        Scope::LastWord => {
            let last = buffer.last_word();
            let mut replay = last.word.clone();
            if options.keep_trailing_spaces {
                replay.extend_from_slice(&last.spaces);
            }
            (last.word.len() + last.spaces.len(), replay)
        }
    };
    // Spaces look the same in every layout: nothing to fix.
    if replay.iter().all(|k| classify(k.vk) != KeyClass::Printable) {
        return None;
    }

    let mut ops = vec![
        PlanOp::Backspace(erase),
        PlanOp::SwitchLayout(target.clone()),
        PlanOp::Replay(replay.clone()),
    ];
    if !options.switch_layout_after_convert {
        ops.push(PlanOp::SwitchLayout(current.clone()));
    }

    line.truncate(line.len() - erase);
    line.extend(replay);
    Some(Conversion {
        plan: ConversionPlan { ops },
        buffer_after: line,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keystroke::vk;

    const OPTIONS: PlanOptions = PlanOptions {
        keep_trailing_spaces: true,
        switch_layout_after_convert: true,
    };

    fn key(vk_code: u16) -> Keystroke {
        Keystroke {
            vk: vk_code,
            scan: vk_code,
            extended: false,
            shift: false,
        }
    }

    fn keys(text: &str) -> Vec<Keystroke> {
        text.chars()
            .map(|c| key(if c == ' ' { vk::SPACE } else { c as u16 }))
            .collect()
    }

    fn buffer(text: &str) -> KeyBuffer {
        let mut b = KeyBuffer::new();
        b.replace(&keys(text));
        b
    }

    fn en() -> LayoutId {
        LayoutId::new("en-US").unwrap()
    }

    fn ru() -> LayoutId {
        LayoutId::new("ru-RU").unwrap()
    }

    #[test]
    fn last_word_keeps_trailing_spaces() {
        let c = plan_conversion(&buffer("AB CD "), Scope::LastWord, &en(), &ru(), OPTIONS).unwrap();
        assert_eq!(
            c.plan.ops,
            vec![
                PlanOp::Backspace(3),
                PlanOp::SwitchLayout(ru()),
                PlanOp::Replay(keys("CD ")),
            ]
        );
        assert_eq!(c.buffer_after, keys("AB CD "));
    }

    #[test]
    fn last_word_can_drop_trailing_spaces() {
        let options = PlanOptions {
            keep_trailing_spaces: false,
            ..OPTIONS
        };
        let c =
            plan_conversion(&buffer("AB CD  "), Scope::LastWord, &en(), &ru(), options).unwrap();
        assert_eq!(c.plan.ops[0], PlanOp::Backspace(4));
        assert_eq!(c.plan.ops[2], PlanOp::Replay(keys("CD")));
        assert_eq!(c.buffer_after, keys("AB CD"));
    }

    #[test]
    fn last_line_converts_everything() {
        let c = plan_conversion(&buffer("AB CD"), Scope::LastLine, &en(), &ru(), OPTIONS).unwrap();
        assert_eq!(c.plan.ops[0], PlanOp::Backspace(5));
        assert_eq!(c.plan.ops[2], PlanOp::Replay(keys("AB CD")));
    }

    #[test]
    fn switches_back_when_configured() {
        let options = PlanOptions {
            switch_layout_after_convert: false,
            ..OPTIONS
        };
        let c = plan_conversion(&buffer("AB"), Scope::LastWord, &en(), &ru(), options).unwrap();
        assert_eq!(c.plan.ops.last(), Some(&PlanOp::SwitchLayout(en())));
    }

    #[test]
    fn nothing_to_convert() {
        for text in ["", "   "] {
            for scope in [Scope::LastWord, Scope::LastLine] {
                assert_eq!(
                    plan_conversion(&buffer(text), scope, &en(), &ru(), OPTIONS),
                    None
                );
            }
        }
    }
}
