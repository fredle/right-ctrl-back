//! The chord state machine. The AI key sends Win+Shift+F23: Left Win down,
//! Left Shift down, then F23 down (and the reverse on release). We cannot
//! know a Win or Shift press is part of the chord until F23 arrives, so those
//! are let through; when F23 completes the chord we suppress it, release the
//! Win and Shift the system thinks are held, and press the target instead.
//!
//! This module is pure: it takes key events and returns what to do, so it is
//! fully unit-tested. `hook.rs` does the actual Windows calls.

use crate::keys::{self, Target};

/// Unassigned virtual key, tapped to stop a lone Win release opening Start.
pub const MASK_VK: u16 = 0xE8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    KeyDown(u16),
    KeyUp(u16),
    Launch(String),
}

#[derive(Default, Debug, PartialEq, Eq)]
pub struct Outcome {
    /// Swallow the event instead of passing it to the rest of the system.
    pub suppress: bool,
    /// Keys to inject / programs to start, in order.
    pub actions: Vec<Action>,
}

impl Outcome {
    fn pass() -> Self {
        Self::default()
    }
    fn swallow() -> Self {
        Self {
            suppress: true,
            actions: vec![],
        }
    }
}

#[derive(Default, Debug)]
pub struct Engine {
    win_down: Option<u16>,
    shift_down: Option<u16>,
    /// F23 is physically down and we own the chord.
    chord_active: bool,
    /// The key we are holding on the target's behalf.
    target_down: Option<u16>,
    /// Physical key-ups to swallow because we already released those keys.
    swallow_up: Vec<u16>,
}

impl Engine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one physical key event. `enabled` is false while paused.
    pub fn on_key(&mut self, vk: u16, down: bool, enabled: bool, target: &Target) -> Outcome {
        if vk == keys::VK_F23 {
            return if down {
                self.f23_down(enabled, target)
            } else {
                self.f23_up()
            };
        }
        if keys::is_win(vk) || keys::is_shift(vk) {
            if down {
                if keys::is_win(vk) {
                    self.win_down = Some(vk);
                } else {
                    self.shift_down = Some(vk);
                }
                return Outcome::pass();
            }
            if let Some(i) = self.swallow_up.iter().position(|&k| k == vk) {
                self.swallow_up.remove(i);
                return Outcome::swallow();
            }
            if self.win_down == Some(vk) {
                self.win_down = None;
            }
            if self.shift_down == Some(vk) {
                self.shift_down = None;
            }
        }
        Outcome::pass()
    }

    fn f23_down(&mut self, enabled: bool, target: &Target) -> Outcome {
        if self.chord_active {
            return Outcome::swallow(); // auto-repeat
        }
        let (Some(win), Some(shift), true) = (self.win_down, self.shift_down, enabled) else {
            return Outcome::pass(); // a bare F23 is not ours to handle
        };
        self.chord_active = true;
        let mut actions = Vec::new();
        match target.key() {
            Some(vk) => {
                actions.push(Action::KeyDown(vk));
                self.target_down = Some(vk);
            }
            None => {
                if let Target::Launch(path) = target {
                    if !target.incomplete() {
                        actions.push(Action::Launch(path.trim().to_string()));
                    }
                }
                actions.push(Action::KeyDown(MASK_VK));
                actions.push(Action::KeyUp(MASK_VK));
            }
        }
        // Undo the Shift and Win the system saw, so they don't combine with
        // whatever the user types next.
        actions.push(Action::KeyUp(shift));
        actions.push(Action::KeyUp(win));
        self.swallow_up.push(shift);
        self.swallow_up.push(win);
        self.win_down = None;
        self.shift_down = None;
        Outcome {
            suppress: true,
            actions,
        }
    }

    fn f23_up(&mut self) -> Outcome {
        if !self.chord_active {
            return Outcome::pass();
        }
        self.chord_active = false;
        let actions = self.release();
        Outcome {
            suppress: true,
            actions,
        }
    }

    /// Let go of anything we are holding (pause, quit).
    pub fn release(&mut self) -> Vec<Action> {
        self.target_down
            .take()
            .map(|vk| vec![Action::KeyUp(vk)])
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::*;

    fn press_chord(e: &mut Engine, t: &Target) -> Outcome {
        assert_eq!(e.on_key(VK_LWIN, true, true, t), Outcome::pass());
        assert_eq!(e.on_key(VK_SHIFT_L, true, true, t), Outcome::pass());
        e.on_key(VK_F23, true, true, t)
    }

    #[test]
    fn chord_becomes_right_ctrl() {
        let mut e = Engine::new();
        let t = Target::RightCtrl;
        let o = press_chord(&mut e, &t);
        assert!(o.suppress);
        assert_eq!(
            o.actions,
            vec![
                Action::KeyDown(VK_RCONTROL),
                Action::KeyUp(VK_SHIFT_L),
                Action::KeyUp(VK_LWIN)
            ]
        );
    }

    #[test]
    fn release_lets_go_and_swallows_physical_ups() {
        let mut e = Engine::new();
        let t = Target::RightCtrl;
        press_chord(&mut e, &t);
        let o = e.on_key(VK_F23, false, true, &t);
        assert!(o.suppress);
        assert_eq!(o.actions, vec![Action::KeyUp(VK_RCONTROL)]);
        // Physical Win/Shift releases were already injected.
        assert!(e.on_key(VK_SHIFT_L, false, true, &t).suppress);
        assert!(e.on_key(VK_LWIN, false, true, &t).suppress);
        // ...and only once.
        assert!(!e.on_key(VK_LWIN, false, true, &t).suppress);
    }

    #[test]
    fn release_order_win_first() {
        let mut e = Engine::new();
        let t = Target::RightCtrl;
        press_chord(&mut e, &t);
        assert!(e.on_key(VK_LWIN, false, true, &t).suppress);
        assert!(e.on_key(VK_SHIFT_L, false, true, &t).suppress);
        let o = e.on_key(VK_F23, false, true, &t);
        assert_eq!(o.actions, vec![Action::KeyUp(VK_RCONTROL)]);
    }

    #[test]
    fn held_ctrl_survives_other_keys() {
        let mut e = Engine::new();
        let t = Target::RightCtrl;
        press_chord(&mut e, &t);
        // Pressing C while the key is held is passed through untouched; the
        // injected Right Ctrl makes it Ctrl+C.
        assert_eq!(e.on_key(0x43, true, true, &t), Outcome::pass());
        assert_eq!(e.on_key(0x43, false, true, &t), Outcome::pass());
    }

    #[test]
    fn auto_repeat_is_swallowed_without_new_actions() {
        let mut e = Engine::new();
        let t = Target::RightCtrl;
        press_chord(&mut e, &t);
        let o = e.on_key(VK_F23, true, true, &t);
        assert_eq!(o, Outcome::swallow());
    }

    #[test]
    fn bare_f23_passes_through() {
        let mut e = Engine::new();
        let t = Target::RightCtrl;
        assert_eq!(e.on_key(VK_F23, true, true, &t), Outcome::pass());
        assert_eq!(e.on_key(VK_F23, false, true, &t), Outcome::pass());
    }

    #[test]
    fn win_alone_or_shift_alone_is_not_a_chord() {
        let mut e = Engine::new();
        let t = Target::RightCtrl;
        e.on_key(VK_LWIN, true, true, &t);
        assert_eq!(e.on_key(VK_F23, true, true, &t), Outcome::pass());
        e.on_key(VK_LWIN, false, true, &t);
        e.on_key(VK_SHIFT_L, true, true, &t);
        assert_eq!(e.on_key(VK_F23, true, true, &t), Outcome::pass());
    }

    #[test]
    fn paused_passes_chord_through() {
        let mut e = Engine::new();
        let t = Target::RightCtrl;
        e.on_key(VK_LWIN, true, false, &t);
        e.on_key(VK_SHIFT_L, true, false, &t);
        assert_eq!(e.on_key(VK_F23, true, false, &t), Outcome::pass());
    }

    #[test]
    fn pausing_mid_hold_still_releases() {
        let mut e = Engine::new();
        let t = Target::RightCtrl;
        press_chord(&mut e, &t);
        let o = e.on_key(VK_F23, false, false, &t);
        assert_eq!(o.actions, vec![Action::KeyUp(VK_RCONTROL)]);
    }

    #[test]
    fn right_win_and_right_shift_count() {
        let mut e = Engine::new();
        let t = Target::Menu;
        e.on_key(VK_RWIN, true, true, &t);
        e.on_key(VK_SHIFT_R, true, true, &t);
        let o = e.on_key(VK_F23, true, true, &t);
        assert_eq!(
            o.actions,
            vec![
                Action::KeyDown(VK_APPS),
                Action::KeyUp(VK_SHIFT_R),
                Action::KeyUp(VK_RWIN)
            ]
        );
    }

    #[test]
    fn launch_target_runs_once_and_masks_start_menu() {
        let mut e = Engine::new();
        let t = Target::Launch(" calc.exe ".into());
        let o = press_chord(&mut e, &t);
        assert_eq!(o.actions[0], Action::Launch("calc.exe".into()));
        assert!(o.actions.contains(&Action::KeyDown(MASK_VK)));
        assert_eq!(o.actions.last(), Some(&Action::KeyUp(VK_LWIN)));
        // Releasing has nothing to let go of.
        let up = e.on_key(VK_F23, false, true, &t);
        assert!(up.suppress);
        assert!(up.actions.is_empty());
    }

    #[test]
    fn empty_launch_does_nothing_but_still_swallows() {
        let mut e = Engine::new();
        let t = Target::Launch(String::new());
        let o = press_chord(&mut e, &t);
        assert!(o.suppress);
        assert!(!o.actions.iter().any(|a| matches!(a, Action::Launch(_))));
    }

    #[test]
    fn media_and_single_key_targets() {
        for t in [Target::Media(0xB3), Target::Key(0x41)] {
            let mut e = Engine::new();
            let vk = t.key().unwrap();
            let o = press_chord(&mut e, &t);
            assert_eq!(o.actions[0], Action::KeyDown(vk));
            let up = e.on_key(VK_F23, false, true, &t);
            assert_eq!(up.actions, vec![Action::KeyUp(vk)]);
        }
    }

    #[test]
    fn explicit_release_is_idempotent() {
        let mut e = Engine::new();
        let t = Target::RightCtrl;
        press_chord(&mut e, &t);
        assert_eq!(e.release(), vec![Action::KeyUp(VK_RCONTROL)]);
        assert!(e.release().is_empty());
    }

    #[test]
    fn works_twice_in_a_row() {
        let mut e = Engine::new();
        let t = Target::RightCtrl;
        for _ in 0..2 {
            assert!(press_chord(&mut e, &t).suppress);
            e.on_key(VK_F23, false, true, &t);
            e.on_key(VK_SHIFT_L, false, true, &t);
            e.on_key(VK_LWIN, false, true, &t);
        }
    }
}
