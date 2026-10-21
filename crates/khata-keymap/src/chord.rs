//! Key chords such as `Ctrl+Alt+U`, `F8` or `Shift+Enter`.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Key {
    /// A letter or digit, stored upper-case.
    Char(char),
    F(u8),
    Enter,
    Esc,
    Backspace,
    Tab,
    Space,
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Chord {
    pub mods: Mods,
    pub key: Key,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ChordParseError {
    #[error("empty chord")]
    Empty,
    #[error("unknown key {0:?}")]
    UnknownKey(String),
}

impl FromStr for Chord {
    type Err = ChordParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts: Vec<&str> = s.split('+').map(str::trim).collect();
        let key = parts
            .pop()
            .filter(|k| !k.is_empty())
            .ok_or(ChordParseError::Empty)?;
        let mut mods = Mods::default();
        for m in parts {
            match m.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => mods.ctrl = true,
                "alt" | "option" => mods.alt = true,
                "shift" => mods.shift = true,
                _ => return Err(ChordParseError::UnknownKey(m.to_owned())),
            }
        }
        Ok(Chord {
            mods,
            key: parse_key(key)?,
        })
    }
}

fn parse_key(k: &str) -> Result<Key, ChordParseError> {
    let lower = k.to_ascii_lowercase();
    let key = match lower.as_str() {
        "enter" | "return" => Key::Enter,
        "esc" | "escape" => Key::Esc,
        "backspace" => Key::Backspace,
        "tab" => Key::Tab,
        "space" | "spacebar" => Key::Space,
        "up" => Key::Up,
        "down" => Key::Down,
        "left" => Key::Left,
        "right" => Key::Right,
        "pageup" | "pgup" => Key::PageUp,
        "pagedown" | "pgdn" => Key::PageDown,
        "home" => Key::Home,
        "end" => Key::End,
        _ => {
            if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
                if (1..=12).contains(&n) {
                    return Ok(Key::F(n));
                }
            }
            let mut chars = k.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if c.is_ascii_alphanumeric() => Key::Char(c.to_ascii_uppercase()),
                _ => return Err(ChordParseError::UnknownKey(k.to_owned())),
            }
        }
    };
    Ok(key)
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.mods.ctrl {
            f.write_str("Ctrl+")?;
        }
        if self.mods.alt {
            f.write_str("Alt+")?;
        }
        if self.mods.shift {
            f.write_str("Shift+")?;
        }
        match self.key {
            Key::Char(c) => write!(f, "{c}"),
            Key::F(n) => write!(f, "F{n}"),
            other => write!(f, "{other:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_round_trips() {
        for s in [
            "F8",
            "Alt+F6",
            "Ctrl+Alt+U",
            "Shift+Enter",
            "Ctrl+A",
            "Esc",
            "PageUp",
            "K",
        ] {
            let c: Chord = s.parse().unwrap();
            assert_eq!(c.to_string(), s);
        }
    }

    #[test]
    fn normalises_case_and_aliases() {
        assert_eq!(
            "ctrl+a".parse::<Chord>().unwrap(),
            "Ctrl+A".parse().unwrap()
        );
        assert_eq!("Return".parse::<Chord>().unwrap().key, Key::Enter);
        assert_eq!("f12".parse::<Chord>().unwrap().key, Key::F(12));
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!("".parse::<Chord>(), Err(ChordParseError::Empty));
        assert!("Hyper+A".parse::<Chord>().is_err());
        assert!("F13".parse::<Chord>().is_err());
        assert!("Ctrl+".parse::<Chord>().is_err());
    }
}
