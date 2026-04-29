use std::fmt;

/// Modifier keys that can be combined with a base key
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers(u8);

impl Modifiers {
    pub const NONE: Modifiers = Modifiers(0);
    pub const CTRL: Modifiers = Modifiers(1 << 0);
    pub const ALT: Modifiers = Modifiers(1 << 1);
    pub const SHIFT: Modifiers = Modifiers(1 << 2);

    #[inline]
    pub fn has_ctrl(self) -> bool {
        (self.0 & Self::CTRL.0) != 0
    }

    #[inline]
    pub fn has_alt(self) -> bool {
        (self.0 & Self::ALT.0) != 0
    }

    #[inline]
    pub fn has_shift(self) -> bool {
        (self.0 & Self::SHIFT.0) != 0
    }

    pub fn add(&mut self, other: Modifiers) {
        self.0 |= other.0;
    }
}

impl std::ops::BitOr for Modifiers {
    type Output = Modifiers;
    fn bitor(self, rhs: Modifiers) -> Modifiers {
        Modifiers(self.0 | rhs.0)
    }
}

/// Base key without modifiers
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BaseKey {
    Char(char),
    Enter,
    Tab,
    Escape,
    Backspace,
    Delete,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
}

/// A key that can be sent to the terminal, with optional modifiers
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key {
    pub base: BaseKey,
    pub modifiers: Modifiers,
}

impl Key {
    /// Parse a key name string from MCP tool arguments
    /// Supports formats: "a", "Enter", "Ctrl+A", "Shift+Tab", "Ctrl+Alt+Delete", etc.
    pub fn from_str(s: &str) -> Result<Self, String> {
        let parts: Vec<&str> = s.split('+').collect();
        if parts.is_empty() {
            return Err("Empty key".to_string());
        }

        let mut modifiers = Modifiers::NONE;
        let mut base_str = parts[parts.len() - 1]; // Last part is the base key
        let modifier_parts = &parts[..parts.len() - 1]; // Everything else is modifiers

        // Parse modifiers
        for part in modifier_parts {
            match part.to_lowercase().as_str() {
                "ctrl" => modifiers.add(Modifiers::CTRL),
                "alt" => modifiers.add(Modifiers::ALT),
                "shift" => modifiers.add(Modifiers::SHIFT),
                "" => {}
                _ => return Err(format!("Unknown modifier: {}", part)),
            }
        }

        // Handle Shift+Tab specially - it's the only Shift+special key with a distinct sequence
        if modifiers.has_shift() && base_str.eq_ignore_ascii_case("tab") {
            return Ok(Key { base: BaseKey::Tab, modifiers });
        }

        // Parse the base key
        // Check for single character FIRST to preserve case
        if base_str.len() == 1 {
            let c = base_str.chars().next().unwrap();
            if !c.is_ascii_control() {
                return Ok(Key { base: BaseKey::Char(c), modifiers });
            }
        }

        let base = match base_str.to_lowercase().as_str() {
            // Named keys
            "enter" | "return" => BaseKey::Enter,
            "tab" => BaseKey::Tab,
            "escape" | "esc" => BaseKey::Escape,
            "backspace" | "bs" => BaseKey::Backspace,
            "delete" | "del" => BaseKey::Delete,
            "up" => BaseKey::Up,
            "down" => BaseKey::Down,
            "left" => BaseKey::Left,
            "right" => BaseKey::Right,
            "home" => BaseKey::Home,
            "end" => BaseKey::End,
            "pageup" | "page_up" | "pgup" => BaseKey::PageUp,
            "pagedown" | "page_down" | "pgdn" => BaseKey::PageDown,
            "space" => BaseKey::Char(' '),
            // Function keys F1-F12
            s if s.starts_with('f') => {
                let rest = &s[1..];
                if let Ok(n) = rest.parse::<u8>() {
                    if (1..=12).contains(&n) {
                        BaseKey::F(n)
                    } else {
                        return Err(format!("Function key F{} out of range (1-12)", n));
                    }
                } else {
                    return Err(format!("Invalid function key: {}", s));
                }
            }
            _ => return Err(format!("Unknown key: {}", base_str)),
        };

        Ok(Key { base, modifiers })
    }

    /// Convert to escape sequence bytes to send to PTY
    pub fn to_escape_sequence(&self, application_cursor_keys: bool) -> Vec<u8> {
        let mut result = Vec::new();

        // Add ESC prefix for Alt modifier
        if self.modifiers.has_alt() {
            result.push(0x1b);
        }

        // Handle Ctrl modifier for character keys
        if self.modifiers.has_ctrl() {
            match &self.base {
                BaseKey::Char(c) => {
                    // Ctrl+A = 0x01, Ctrl+B = 0x02, etc.
                    let c = c.to_ascii_lowercase();
                    if c.is_ascii_lowercase() {
                        let code = (c as u8).wrapping_sub(b'a').wrapping_add(1);
                        result.push(code);
                    } else if c == ' ' {
                        result.push(0x00); // Ctrl+Space = NUL
                    } else {
                        // For other chars, just use the char as-is
                        result.push(c as u8);
                    }
                    return result;
                }
                BaseKey::Tab => {
                    result.push(0x09); // Ctrl+Tab = HT
                    return result;
                }
                BaseKey::Enter => {
                    result.push(0x0d); // Ctrl+Enter = CR (same as just Enter)
                    return result;
                }
                BaseKey::Escape => {
                    result.push(0x1b); // Ctrl+Escape = ESC (same as just Escape)
                    return result;
                }
                // For special keys with Ctrl, we send the regular sequence
                // (most terminals don't have Ctrl modifier sequences for special keys)
                _ => {}
            }
        }

        // Handle Shift for special keys
        if self.modifiers.has_shift() {
            match &self.base {
                BaseKey::Tab => {
                    // Shift+Tab = ESC [ Z
                    result.push(0x1b);
                    result.push(b'[');
                    result.push(b'Z');
                    return result;
                }
                // Shift+Enter and other Shift+special keys typically just send the base key
                // Most terminals don't distinguish Shift for special keys
                _ => {}
            }
        }

        // Regular key (possibly with Alt prefix already added)
        match &self.base {
            BaseKey::Char(c) => {
                // If Shift was pressed on a letter, use uppercase
                let c = if self.modifiers.has_shift() && c.is_ascii_lowercase() {
                    c.to_ascii_uppercase()
                } else {
                    *c
                };
                let mut buf = [0u8; 4];
                let s = c.encode_utf8(&mut buf);
                result.extend_from_slice(s.as_bytes());
            }
            BaseKey::Enter => result.push(b'\r'),
            BaseKey::Tab => result.push(b'\t'),
            BaseKey::Escape => result.push(0x1b),
            BaseKey::Backspace => result.push(0x7f),
            BaseKey::Delete => result.extend_from_slice(b"\x1b[3~"),
            BaseKey::Up => {
                if application_cursor_keys {
                    result.extend_from_slice(b"\x1bOA");
                } else {
                    result.extend_from_slice(b"\x1b[A");
                }
            }
            BaseKey::Down => {
                if application_cursor_keys {
                    result.extend_from_slice(b"\x1bOB");
                } else {
                    result.extend_from_slice(b"\x1b[B");
                }
            }
            BaseKey::Right => {
                if application_cursor_keys {
                    result.extend_from_slice(b"\x1bOC");
                } else {
                    result.extend_from_slice(b"\x1b[C");
                }
            }
            BaseKey::Left => {
                if application_cursor_keys {
                    result.extend_from_slice(b"\x1bOD");
                } else {
                    result.extend_from_slice(b"\x1b[D");
                }
            }
            BaseKey::Home => result.extend_from_slice(b"\x1b[H"),
            BaseKey::End => result.extend_from_slice(b"\x1b[F"),
            BaseKey::PageUp => result.extend_from_slice(b"\x1b[5~"),
            BaseKey::PageDown => result.extend_from_slice(b"\x1b[6~"),
            BaseKey::F(n) => {
                result.extend_from_slice(match n {
                    1 => b"\x1bOP",
                    2 => b"\x1bOQ",
                    3 => b"\x1bOR",
                    4 => b"\x1bOS",
                    5 => b"\x1b[15~",
                    6 => b"\x1b[17~",
                    7 => b"\x1b[18~",
                    8 => b"\x1b[19~",
                    9 => b"\x1b[20~",
                    10 => b"\x1b[21~",
                    11 => b"\x1b[23~",
                    12 => b"\x1b[24~",
                    _ => return result,
                });
            }
        }

        result
    }

    /// Convenience method for simple keys without modifiers
    pub fn from_base(base: BaseKey) -> Self {
        Key { base, modifiers: Modifiers::NONE }
    }

    /// Create a key with Ctrl modifier
    pub fn ctrl(base: BaseKey) -> Self {
        Key { base, modifiers: Modifiers::CTRL }
    }

    /// Create a key with Alt modifier
    pub fn alt(base: BaseKey) -> Self {
        Key { base, modifiers: Modifiers::ALT }
    }

    /// Create a key with Shift modifier
    pub fn shift(base: BaseKey) -> Self {
        Key { base, modifiers: Modifiers::SHIFT }
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut sep = "";
        if self.modifiers.has_ctrl() {
            write!(f, "Ctrl")?;
            sep = "+";
        }
        if self.modifiers.has_alt() {
            write!(f, "{}Alt", sep)?;
            sep = "+";
        }
        if self.modifiers.has_shift() {
            write!(f, "{}Shift", sep)?;
            sep = "+";
        }

        match &self.base {
            BaseKey::Char(c) => write!(f, "{}{}", sep, c),
            BaseKey::Enter => write!(f, "{}Enter", sep),
            BaseKey::Tab => write!(f, "{}Tab", sep),
            BaseKey::Escape => write!(f, "{}Escape", sep),
            BaseKey::Backspace => write!(f, "{}Backspace", sep),
            BaseKey::Delete => write!(f, "{}Delete", sep),
            BaseKey::Up => write!(f, "{}Up", sep),
            BaseKey::Down => write!(f, "{}Down", sep),
            BaseKey::Left => write!(f, "{}Left", sep),
            BaseKey::Right => write!(f, "{}Right", sep),
            BaseKey::Home => write!(f, "{}Home", sep),
            BaseKey::End => write!(f, "{}End", sep),
            BaseKey::PageUp => write!(f, "{}PageUp", sep),
            BaseKey::PageDown => write!(f, "{}PageDown", sep),
            BaseKey::F(n) => write!(f, "{}F{}", sep, n),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_char() {
        assert_eq!(Key::from_str("a").unwrap(), Key::from_base(BaseKey::Char('a')));
        assert_eq!(Key::from_str("Z").unwrap(), Key::from_base(BaseKey::Char('Z')));
    }

    #[test]
    fn test_parse_named() {
        assert_eq!(Key::from_str("Enter").unwrap().base, BaseKey::Enter);
        assert_eq!(Key::from_str("Tab").unwrap().base, BaseKey::Tab);
        assert_eq!(Key::from_str("Escape").unwrap().base, BaseKey::Escape);
        assert_eq!(Key::from_str("Delete").unwrap().base, BaseKey::Delete);
    }

    #[test]
    fn test_parse_ctrl() {
        let key = Key::from_str("Ctrl+a").unwrap();
        assert!(key.modifiers.has_ctrl());
        assert_eq!(key.base, BaseKey::Char('a'));
    }

    #[test]
    fn test_parse_alt() {
        let key = Key::from_str("Alt+x").unwrap();
        assert!(key.modifiers.has_alt());
        assert_eq!(key.base, BaseKey::Char('x'));
    }

    #[test]
    fn test_parse_shift_tab() {
        let key = Key::from_str("Shift+Tab").unwrap();
        assert!(key.modifiers.has_shift());
        assert_eq!(key.base, BaseKey::Tab);
    }

    #[test]
    fn test_parse_ctrl_shift() {
        let key = Key::from_str("Ctrl+Shift+A").unwrap();
        assert!(key.modifiers.has_ctrl());
        assert!(key.modifiers.has_shift());
        assert_eq!(key.base, BaseKey::Char('A')); // Preserves original case
    }

    #[test]
    fn test_parse_ctrl_alt() {
        let key = Key::from_str("Ctrl+Alt+Delete").unwrap();
        assert!(key.modifiers.has_ctrl());
        assert!(key.modifiers.has_alt());
        assert_eq!(key.base, BaseKey::Delete);
    }

    #[test]
    fn test_parse_function_keys() {
        assert_eq!(Key::from_str("F1").unwrap().base, BaseKey::F(1));
        assert_eq!(Key::from_str("F12").unwrap().base, BaseKey::F(12));
    }

    #[test]
    fn test_escape_sequences() {
        assert_eq!(Key::from_base(BaseKey::Enter).to_escape_sequence(false), vec![b'\r']);
        assert_eq!(Key::ctrl(BaseKey::Char('c')).to_escape_sequence(false), vec![0x03]);
        assert_eq!(Key::ctrl(BaseKey::Char('d')).to_escape_sequence(false), vec![0x04]);
        assert_eq!(Key::from_base(BaseKey::Up).to_escape_sequence(false), b"\x1b[A".to_vec());
        assert_eq!(Key::from_base(BaseKey::Up).to_escape_sequence(true), b"\x1bOA".to_vec());
        assert_eq!(Key::from_base(BaseKey::F(1)).to_escape_sequence(false), b"\x1bOP".to_vec());
        assert_eq!(Key::shift(BaseKey::Tab).to_escape_sequence(false), b"\x1b[Z".to_vec());
    }

    #[test]
    fn test_alt_char() {
        let key = Key::alt(BaseKey::Char('a'));
        let seq = key.to_escape_sequence(false);
        assert_eq!(seq, vec![0x1b, b'a']);
    }

    #[test]
    fn test_display() {
        assert_eq!(Key::from_str("Ctrl+C").unwrap().to_string(), "Ctrl+C");
        assert_eq!(Key::from_str("Ctrl+Alt+F1").unwrap().to_string(), "Ctrl+Alt+F1");
    }
}
