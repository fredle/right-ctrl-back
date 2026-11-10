//! Virtual-key constants, key names, and the set of things the AI key can be
//! turned into. Pure data and functions - no Windows calls - so it is testable.

use serde::{Deserialize, Serialize};

pub const VK_SHIFT_L: u16 = 0xA0;
pub const VK_SHIFT_R: u16 = 0xA1;
pub const VK_LWIN: u16 = 0x5B;
pub const VK_RWIN: u16 = 0x5C;
pub const VK_F23: u16 = 0x86;
pub const VK_RCONTROL: u16 = 0xA3;
pub const VK_APPS: u16 = 0x5D;
pub const VK_ESCAPE: u16 = 0x1B;

pub fn is_win(vk: u16) -> bool {
    vk == VK_LWIN || vk == VK_RWIN
}

pub fn is_shift(vk: u16) -> bool {
    vk == VK_SHIFT_L || vk == VK_SHIFT_R || vk == 0x10
}

/// What the AI key should do.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", content = "value")]
pub enum Target {
    /// A true, held Right Ctrl.
    RightCtrl,
    /// The context-menu key.
    Menu,
    /// One of the media keys in [`MEDIA_KEYS`] (virtual-key code).
    Media(u16),
    /// Any single key (virtual-key code), held while the AI key is held.
    Key(u16),
    /// Launch a program, document or URL.
    Launch(String),
}

impl Default for Target {
    fn default() -> Self {
        Target::RightCtrl
    }
}

/// The kinds shown in the target picker.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    RightCtrl,
    Menu,
    Media,
    Key,
    Launch,
}

impl Kind {
    pub const ALL: [Kind; 5] = [Kind::RightCtrl, Kind::Menu, Kind::Media, Kind::Key, Kind::Launch];

    pub fn label(self) -> &'static str {
        match self {
            Kind::RightCtrl => "Right Ctrl",
            Kind::Menu => "Menu key",
            Kind::Media => "A media key",
            Kind::Key => "Any single key",
            Kind::Launch => "Launch an app",
        }
    }

    /// A sensible starting target when the user switches to this kind.
    pub fn default_target(self) -> Target {
        match self {
            Kind::RightCtrl => Target::RightCtrl,
            Kind::Menu => Target::Menu,
            Kind::Media => Target::Media(MEDIA_KEYS[0].0),
            Kind::Key => Target::Key(0x7B), // F12 until the user picks one
            Kind::Launch => Target::Launch(String::new()),
        }
    }
}

impl Target {
    pub fn kind(&self) -> Kind {
        match self {
            Target::RightCtrl => Kind::RightCtrl,
            Target::Menu => Kind::Menu,
            Target::Media(_) => Kind::Media,
            Target::Key(_) => Kind::Key,
            Target::Launch(_) => Kind::Launch,
        }
    }

    /// The key to hold down, for targets that are a key.
    pub fn key(&self) -> Option<u16> {
        match self {
            Target::RightCtrl => Some(VK_RCONTROL),
            Target::Menu => Some(VK_APPS),
            Target::Media(vk) | Target::Key(vk) => Some(*vk),
            Target::Launch(_) => None,
        }
    }

    /// Short human description, used in the status line.
    pub fn describe(&self) -> String {
        match self {
            Target::Launch(p) if p.trim().is_empty() => "Launch an app (none chosen yet)".into(),
            Target::Launch(p) => format!("Launch {}", p.trim()),
            other => other.key().map(vk_name).unwrap_or_default(),
        }
    }

    /// True when the target cannot do anything yet.
    pub fn incomplete(&self) -> bool {
        matches!(self, Target::Launch(p) if p.trim().is_empty())
    }
}

pub const MEDIA_KEYS: [(u16, &str); 7] = [
    (0xB3, "Play / Pause"),
    (0xB0, "Next track"),
    (0xB1, "Previous track"),
    (0xB2, "Stop"),
    (0xAD, "Mute"),
    (0xAF, "Volume up"),
    (0xAE, "Volume down"),
];

/// Keys that need the extended-key flag when sent.
pub fn is_extended(vk: u16) -> bool {
    matches!(
        vk,
        0xA3 | 0xA5 // right ctrl, right alt
            | 0x5B | 0x5C | 0x5D // win keys, menu
            | 0x21..=0x28 // page up .. arrows
            | 0x2D | 0x2E // insert, delete
            | 0x6F // numpad divide
            | 0x90 // num lock
            | 0xA6..=0xB9 // browser, volume, media, launch keys
    )
}

/// Readable name for a virtual-key code.
pub fn vk_name(vk: u16) -> String {
    if let Some((_, n)) = MEDIA_KEYS.iter().find(|(v, _)| *v == vk) {
        return (*n).to_string();
    }
    let s: String = match vk {
        0x08 => "Backspace".into(),
        0x09 => "Tab".into(),
        0x0D => "Enter".into(),
        0x10 => "Shift".into(),
        0x11 => "Ctrl".into(),
        0x12 => "Alt".into(),
        0x13 => "Pause".into(),
        0x14 => "Caps Lock".into(),
        0x1B => "Esc".into(),
        0x20 => "Space".into(),
        0x21 => "Page Up".into(),
        0x22 => "Page Down".into(),
        0x23 => "End".into(),
        0x24 => "Home".into(),
        0x25 => "Left".into(),
        0x26 => "Up".into(),
        0x27 => "Right".into(),
        0x28 => "Down".into(),
        0x2C => "Print Screen".into(),
        0x2D => "Insert".into(),
        0x2E => "Delete".into(),
        0x30..=0x39 | 0x41..=0x5A => char::from(vk as u8).to_string(),
        0x5B => "Left Win".into(),
        0x5C => "Right Win".into(),
        0x5D => "Menu".into(),
        0x60..=0x69 => format!("Numpad {}", vk - 0x60),
        0x6A => "Numpad *".into(),
        0x6B => "Numpad +".into(),
        0x6D => "Numpad -".into(),
        0x6E => "Numpad .".into(),
        0x6F => "Numpad /".into(),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        0x90 => "Num Lock".into(),
        0x91 => "Scroll Lock".into(),
        0xA0 => "Left Shift".into(),
        0xA1 => "Right Shift".into(),
        0xA2 => "Left Ctrl".into(),
        0xA3 => "Right Ctrl".into(),
        0xA4 => "Left Alt".into(),
        0xA5 => "Right Alt".into(),
        0xA6 => "Browser Back".into(),
        0xA7 => "Browser Forward".into(),
        0xAC => "Browser Home".into(),
        0xB4 => "Launch Mail".into(),
        0xB6 => "Launch App 1".into(),
        0xB7 => "Launch App 2".into(),
        0xBA => ";".into(),
        0xBB => "=".into(),
        0xBC => ",".into(),
        0xBD => "-".into(),
        0xBE => ".".into(),
        0xBF => "/".into(),
        0xC0 => "`".into(),
        0xDB => "[".into(),
        0xDC => "\\".into(),
        0xDD => "]".into(),
        0xDE => "'".into(),
        _ => format!("Key 0x{vk:02X}"),
    };
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(vk_name(VK_F23), "F23");
        assert_eq!(vk_name(0x70), "F1");
        assert_eq!(vk_name(0x41), "A");
        assert_eq!(vk_name(VK_RCONTROL), "Right Ctrl");
        assert_eq!(vk_name(0xB3), "Play / Pause");
        assert_eq!(vk_name(0xFE), "Key 0xFE");
    }

    #[test]
    fn target_keys() {
        assert_eq!(Target::RightCtrl.key(), Some(VK_RCONTROL));
        assert_eq!(Target::Menu.key(), Some(VK_APPS));
        assert_eq!(Target::Key(0x41).key(), Some(0x41));
        assert_eq!(Target::Launch("x".into()).key(), None);
    }

    #[test]
    fn incomplete_launch() {
        assert!(Target::Launch("  ".into()).incomplete());
        assert!(!Target::Launch("notepad.exe".into()).incomplete());
        assert!(!Target::RightCtrl.incomplete());
    }

    #[test]
    fn kind_roundtrip() {
        for k in Kind::ALL {
            assert_eq!(k.default_target().kind(), k);
        }
    }

    #[test]
    fn extended_flags() {
        assert!(is_extended(VK_RCONTROL));
        assert!(is_extended(VK_APPS));
        assert!(is_extended(0xB3));
        assert!(!is_extended(0x41));
    }

    #[test]
    fn target_serde() {
        for t in [
            Target::RightCtrl,
            Target::Media(0xAD),
            Target::Key(0x41),
            Target::Launch("calc.exe".into()),
        ] {
            let s = serde_json::to_string(&t).unwrap();
            assert_eq!(serde_json::from_str::<Target>(&s).unwrap(), t);
        }
    }
}
