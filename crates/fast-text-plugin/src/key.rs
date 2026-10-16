use std::fmt;

/// A key, independent of any particular terminal backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyName {
    Char(char),
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Enter,
    Esc,
    Tab,
    Space,
    Backspace,
    Delete,
    F(u8),
}

/// A key plus its modifier state, as written in a plugin, e.g. `ctrl+shift+k`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeySpec {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub name: KeyName,
}

impl KeySpec {
    pub const fn new(name: KeyName, ctrl: bool, alt: bool, shift: bool) -> Self {
        Self {
            ctrl,
            alt,
            shift,
            name,
        }
    }

    /// Parses a human spelling like `ctrl+k`, `alt+left`, `f5`, `shift+enter`.
    pub fn parse(input: &str) -> Option<KeySpec> {
        let parts: Vec<&str> = input
            .split('+')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .collect();
        let (key_part, mods) = parts.split_last()?;

        let mut ctrl = false;
        let mut alt = false;
        let mut shift = false;
        for m in mods {
            match m.to_ascii_lowercase().as_str() {
                "ctrl" | "control" | "c" => ctrl = true,
                "alt" | "opt" | "option" => alt = true,
                "shift" => shift = true,
                "cmd" | "command" | "super" | "meta" | "win" => {}
                _ => return None,
            }
        }

        let name = match key_part.to_ascii_lowercase().as_str() {
            "left" => KeyName::Left,
            "right" => KeyName::Right,
            "up" => KeyName::Up,
            "down" => KeyName::Down,
            "home" => KeyName::Home,
            "end" => KeyName::End,
            "pageup" | "page_up" | "pgup" => KeyName::PageUp,
            "pagedown" | "page_down" | "pgdn" => KeyName::PageDown,
            "enter" | "return" => KeyName::Enter,
            "esc" | "escape" => KeyName::Esc,
            "tab" => KeyName::Tab,
            "space" => KeyName::Space,
            "backspace" | "bs" => KeyName::Backspace,
            "delete" | "del" => KeyName::Delete,
            other => {
                if let Some(num) = other.strip_prefix('f') {
                    if let Ok(n) = num.parse::<u8>() {
                        if (1..=24).contains(&n) {
                            return Some(KeySpec::new(KeyName::F(n), ctrl, alt, shift));
                        }
                    }
                }
                let mut chars = key_part.chars();
                let c = chars.next()?;
                if chars.next().is_some() {
                    return None;
                }
                KeyName::Char(c.to_ascii_lowercase())
            }
        };

        Some(KeySpec::new(name, ctrl, alt, shift))
    }

    /// A human-readable spelling, e.g. `Ctrl+Shift+K`.
    pub fn display(&self) -> String {
        let mut out = String::new();
        if self.ctrl {
            out.push_str("Ctrl+");
        }
        if self.alt {
            out.push_str("Alt+");
        }
        if self.shift {
            out.push_str("Shift+");
        }
        match self.name {
            KeyName::Char(' ') => out.push_str("Space"),
            KeyName::Char(c) => out.push(c.to_ascii_uppercase()),
            KeyName::Left => out.push_str("Left"),
            KeyName::Right => out.push_str("Right"),
            KeyName::Up => out.push_str("Up"),
            KeyName::Down => out.push_str("Down"),
            KeyName::Home => out.push_str("Home"),
            KeyName::End => out.push_str("End"),
            KeyName::PageUp => out.push_str("PageUp"),
            KeyName::PageDown => out.push_str("PageDown"),
            KeyName::Enter => out.push_str("Enter"),
            KeyName::Esc => out.push_str("Esc"),
            KeyName::Tab => out.push_str("Tab"),
            KeyName::Space => out.push_str("Space"),
            KeyName::Backspace => out.push_str("Backspace"),
            KeyName::Delete => out.push_str("Delete"),
            KeyName::F(n) => out.push_str(&format!("F{n}")),
        }
        out
    }
}

impl fmt::Display for KeySpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modifiers_and_keys() {
        let k = KeySpec::parse("Ctrl+Shift+K").unwrap();
        assert!(k.ctrl && k.shift && !k.alt);
        assert_eq!(k.name, KeyName::Char('k'));

        let k = KeySpec::parse("alt+left").unwrap();
        assert!(k.alt);
        assert_eq!(k.name, KeyName::Left);

        let k = KeySpec::parse("f5").unwrap();
        assert_eq!(k.name, KeyName::F(5));

        assert!(KeySpec::parse("nonsense+").is_none());
        assert!(KeySpec::parse("").is_none());
    }

    #[test]
    fn displays_round_trip() {
        assert_eq!(KeySpec::parse("ctrl+f5").unwrap().display(), "Ctrl+F5");
        assert_eq!(
            KeySpec::parse("shift+enter").unwrap().display(),
            "Shift+Enter"
        );
    }
}
