//! Translating key presses into the bytes a terminal program expects.

use iced::keyboard::{Key, Modifiers, key::Named};

/// Bytes for a key press, or `None` when the key sends nothing (modifier
/// alone, IDE shortcut). `app_cursor` is the terminal's application cursor
/// mode, which changes the arrow-key sequences (used by vim, less, fzf).
pub fn encode(
    key: &Key,
    modifiers: Modifiers,
    text: Option<&str>,
    app_cursor: bool,
) -> Option<Vec<u8>> {
    // ⌘ combinations belong to the IDE (copy, paste, tabs), not the shell.
    if modifiers.logo() {
        return None;
    }
    let arrow = |c: u8| -> Vec<u8> {
        if app_cursor {
            vec![0x1b, b'O', c]
        } else {
            vec![0x1b, b'[', c]
        }
    };
    let bytes = match key.as_ref() {
        Key::Named(Named::Enter) => vec![b'\r'],
        Key::Named(Named::Backspace) if modifiers.alt() => vec![0x1b, 0x7f],
        Key::Named(Named::Backspace) => vec![0x7f],
        Key::Named(Named::Tab) if modifiers.shift() => b"\x1b[Z".to_vec(),
        Key::Named(Named::Tab) => vec![b'\t'],
        Key::Named(Named::Escape) => vec![0x1b],
        Key::Named(Named::ArrowUp) => arrow(b'A'),
        Key::Named(Named::ArrowDown) => arrow(b'B'),
        Key::Named(Named::ArrowRight) if modifiers.alt() => b"\x1bf".to_vec(),
        Key::Named(Named::ArrowLeft) if modifiers.alt() => b"\x1bb".to_vec(),
        Key::Named(Named::ArrowRight) => arrow(b'C'),
        Key::Named(Named::ArrowLeft) => arrow(b'D'),
        Key::Named(Named::Home) => b"\x1b[H".to_vec(),
        Key::Named(Named::End) => b"\x1b[F".to_vec(),
        Key::Named(Named::Delete) => b"\x1b[3~".to_vec(),
        Key::Named(Named::PageUp) => b"\x1b[5~".to_vec(),
        Key::Named(Named::PageDown) => b"\x1b[6~".to_vec(),
        Key::Named(Named::Space) if modifiers.control() => vec![0],
        Key::Character(c) if modifiers.control() => {
            // Ctrl+A..Z -> 0x01..0x1A; Ctrl+[ \ ] -> ESC, FS, GS.
            let ch = c.chars().next()?.to_ascii_lowercase();
            match ch {
                'a'..='z' => vec![ch as u8 - b'a' + 1],
                '[' => vec![0x1b],
                '\\' => vec![0x1c],
                ']' => vec![0x1d],
                _ => return None,
            }
        }
        _ => {
            let text = text.filter(|t| !t.chars().any(char::is_control))?;
            let mut out = Vec::with_capacity(text.len() + 1);
            if modifiers.alt() {
                out.push(0x1b);
            }
            out.extend_from_slice(text.as_bytes());
            out
        }
    };
    Some(bytes)
}

/// Wraps pasted text in bracketed-paste markers when the program asked for
/// them, so shells do not run pasted lines one by one.
pub fn paste(text: &str, bracketed: bool) -> Vec<u8> {
    let body = text.replace("\r\n", "\r").replace('\n', "\r");
    if bracketed {
        [b"\x1b[200~".as_slice(), body.as_bytes(), b"\x1b[201~"].concat()
    } else {
        body.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_should_map_control_letters_and_arrows() {
        let ctrl = Modifiers::CTRL;
        assert_eq!(
            encode(&Key::Character("c".into()), ctrl, None, false),
            Some(vec![3])
        );
        assert_eq!(
            encode(&Key::Named(Named::ArrowUp), Modifiers::empty(), None, false),
            Some(b"\x1b[A".to_vec())
        );
        assert_eq!(
            encode(&Key::Named(Named::ArrowUp), Modifiers::empty(), None, true),
            Some(b"\x1bOA".to_vec())
        );
    }

    #[test]
    fn encode_should_send_text_and_leave_command_shortcuts_to_the_ide() {
        let plain = Modifiers::empty();
        assert_eq!(
            encode(&Key::Character("é".into()), plain, Some("é"), false),
            Some("é".as_bytes().to_vec())
        );
        assert_eq!(
            encode(
                &Key::Character("v".into()),
                Modifiers::LOGO,
                Some("v"),
                false
            ),
            None
        );
    }

    #[test]
    fn paste_should_bracket_when_requested() {
        assert_eq!(paste("a\nb", true), b"\x1b[200~a\rb\x1b[201~".to_vec());
        assert_eq!(paste("a\nb", false), b"a\rb".to_vec());
    }
}
