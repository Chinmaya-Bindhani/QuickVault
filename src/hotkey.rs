use crate::Result;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN,
};

pub const DEFAULT: &str = "Alt+F";
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Shortcut {
    pub mods: u32,
    pub key: u32,
}
impl Shortcut {
    pub fn parse(text: &str) -> Result<Self> {
        let parts: Vec<_> = text
            .split('+')
            .map(|s| s.trim().to_ascii_uppercase())
            .collect();
        let (key, mods) = parts.split_last().ok_or("Enter a shortcut.")?;
        let mut bits = 0;
        for name in mods {
            let flag = match name.as_str() {
                "ALT" => MOD_ALT,
                "CTRL" => MOD_CONTROL,
                "SHIFT" => MOD_SHIFT,
                "WIN" => MOD_WIN,
                _ => return Err("Use Ctrl, Alt, Shift or Win as modifiers.".into()),
            };
            if bits & flag.0 != 0 {
                return Err("A modifier appears twice.".into());
            }
            bits |= flag.0;
        }
        let vk = match key.as_str() {
            "SPACE" => 32,
            s if s.len() == 1 && s.as_bytes()[0].is_ascii_alphanumeric() => s.as_bytes()[0] as u32,
            s if s.starts_with('F') => s[1..]
                .parse::<u32>()
                .ok()
                .filter(|v| (1..=24).contains(v))
                .map(|v| 111 + v)
                .ok_or("Use F1 through F24.")?,
            _ => return Err("Use a letter, digit, Space or a function key.".into()),
        };
        Self::from_parts(bits, vk)
    }
    pub fn from_parts(mods: u32, key: u32) -> Result<Self> {
        if mods == 0
            || mods & !15 != 0
            || !(key == 32
                || (48..=57).contains(&key)
                || (65..=90).contains(&key)
                || (112..=135).contains(&key))
        {
            return Err("Choose at least one modifier and a supported key.".into());
        }
        Ok(Self { mods, key })
    }
    pub fn label(self) -> String {
        let mut parts = Vec::new();
        for (mask, name) in [
            (MOD_CONTROL, "Ctrl"),
            (MOD_ALT, "Alt"),
            (MOD_SHIFT, "Shift"),
            (MOD_WIN, "Win"),
        ] {
            if self.mods & mask.0 != 0 {
                parts.push(name.to_owned());
            }
        }
        parts.push(match self.key {
            32 => "Space".into(),
            112..=135 => format!("F{}", self.key - 111),
            x => char::from_u32(x).unwrap_or('?').to_string(),
        });
        parts.join("+")
    }
    pub fn flags(self) -> HOT_KEY_MODIFIERS {
        HOT_KEY_MODIFIERS(self.mods | 0x4000)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_and_format() {
        for s in [
            "Alt+F",
            "Ctrl+Shift+Space",
            "Win+F24",
            "Alt+Space",
            "Ctrl+9",
        ] {
            assert_eq!(Shortcut::parse(s).unwrap().label(), s);
        }
        for s in ["F", "Ctrl+", "Ctrl+F28", "Alt+Alt+F", "Ctrl+XX", "Ctrl+A+B"] {
            assert!(Shortcut::parse(s).is_err());
        }
    }
}
