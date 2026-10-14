//! Alternative keymaps and the user's own key bindings: shortcuts that
//! replace the built-in ones before they are looked up. A preset
//! (`keymap = "vscode"`, `"eclipse"`, `"netbeans"`) brings the bindings
//! users of those editors know; `[keybindings]` in the settings adds or
//! overrides single chords (`"cmd+shift+p" = "FindAction"`, or `"none"`
//! to unbind).
//!
//! A chord is `+`-joined modifiers (`cmd`, `ctrl`, `alt`/`opt`, `shift`,
//! and `mod`: `cmd` on macOS, `ctrl` elsewhere) and one key: a character
//! or a named key (`f12`, `enter`, `up`, `tab`, `space`, `` ` ``).

use super::Cmd;
use iced::keyboard::{Key, Modifiers};
use std::collections::HashMap;
use std::sync::OnceLock;

/// A normalized key press: modifier bits and the lowercased key name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Chord {
    /// `cmd`, `ctrl`, `alt`, `shift` as bits 0–3.
    mods: u8,
    /// Lowercased character or named key (`f12`, `arrowup`).
    key: String,
}

/// Modifier bits.
const CMD: u8 = 1;
/// Control.
const CTRL: u8 = 2;
/// Option / Alt.
const ALT: u8 = 4;
/// Shift.
const SHIFT: u8 = 8;

/// Short names accepted for named keys, as iced spells them lowercased.
const KEY_ALIASES: &[(&str, &str)] = &[
    ("up", "arrowup"),
    ("down", "arrowdown"),
    ("left", "arrowleft"),
    ("right", "arrowright"),
    ("esc", "escape"),
    ("return", "enter"),
    ("del", "delete"),
    ("pageup", "pageup"),
    ("pagedown", "pagedown"),
];

impl Chord {
    /// The chord of a key press.
    pub fn of(key: &Key, m: Modifiers) -> Option<Self> {
        let name = match key.as_ref() {
            Key::Character(c) => c.to_lowercase(),
            Key::Named(named) => format!("{named:?}").to_lowercase(),
            Key::Unidentified => return None,
        };
        let mut mods = 0;
        for (on, bit) in [
            (m.logo(), CMD),
            (m.control(), CTRL),
            (m.alt(), ALT),
            (m.shift(), SHIFT),
        ] {
            if on {
                mods |= bit;
            }
        }
        // Shift turns letters upper case; the chord names the letter.
        Some(Self { mods, key: name })
    }

    /// Parses `cmd+shift+p`; `None` when a part is unknown.
    pub fn parse(text: &str) -> Option<Self> {
        let parts: Vec<String> = text.split('+').map(|p| p.trim().to_lowercase()).collect();
        // `cmd++` names the plus key.
        let (key, mods) = match parts.split_last()? {
            (k, rest) if k.is_empty() && text.ends_with("++") => {
                ("+".to_string(), &rest[..rest.len().saturating_sub(1)])
            }
            (k, rest) => (k.clone(), rest),
        };
        if key.is_empty() {
            return None;
        }
        let mut bits = 0;
        for m in mods {
            bits |= match m.as_str() {
                "cmd" | "command" | "meta" | "super" => CMD,
                "ctrl" | "control" => CTRL,
                "alt" | "opt" | "option" => ALT,
                "shift" => SHIFT,
                "mod" if cfg!(target_os = "macos") => CMD,
                "mod" => CTRL,
                _ => return None,
            };
        }
        let key = KEY_ALIASES
            .iter()
            .find(|(alias, _)| *alias == key)
            .map_or(key.clone(), |(_, name)| (*name).to_string());
        Some(Self { mods: bits, key })
    }
}

/// VS Code's bindings.
const VSCODE: &[(&str, &str)] = &[
    ("mod+shift+p", "FindAction"),
    ("mod+p", "GotoFile"),
    ("mod+t", "GotoSymbol"),
    ("mod+shift+o", "FileStructure"),
    ("f12", "GotoDeclaration"),
    ("shift+f12", "FindUsages"),
    ("f2", "Rename"),
    ("mod+.", "QuickFix"),
    ("ctrl+g", "GotoLine"),
    ("mod+shift+f", "FindInPath"),
    ("mod+b", "ToggleSidebar"),
    ("ctrl+`", "ToggleTerminal"),
    ("alt+up", "MoveLineUp"),
    ("alt+down", "MoveLineDown"),
    ("shift+alt+down", "Duplicate"),
    ("mod+shift+k", "DeleteLine"),
    ("shift+alt+f", "Reformat"),
    ("mod+d", "SelectNextOccurrence"),
    ("mod+shift+l", "SelectAllOccurrences"),
    ("f8", "NextError"),
    ("shift+f8", "PreviousError"),
    ("ctrl+-", "Back"),
    ("ctrl+shift+-", "Forward"),
    ("mod+space", "Complete"),
];

/// Eclipse's bindings.
const ECLIPSE: &[(&str, &str)] = &[
    ("mod+shift+r", "GotoFile"),
    ("mod+shift+t", "GotoClass"),
    ("f3", "GotoDeclaration"),
    ("mod+shift+g", "FindUsages"),
    ("alt+mod+r", "Rename"),
    ("mod+1", "QuickFix"),
    ("mod+l", "GotoLine"),
    ("ctrl+h", "FindInPath"),
    ("mod+d", "DeleteLine"),
    ("alt+mod+down", "Duplicate"),
    ("alt+up", "MoveLineUp"),
    ("alt+down", "MoveLineDown"),
    ("mod+shift+f", "Reformat"),
    ("mod+o", "FileStructure"),
    ("mod+shift+o", "OptimizeImports"),
    ("mod+3", "FindAction"),
    ("mod+.", "NextError"),
    ("mod+,", "PreviousError"),
    ("mod+[", "Back"),
    ("mod+]", "Forward"),
    ("ctrl+space", "Complete"),
];

/// NetBeans' bindings.
const NETBEANS: &[(&str, &str)] = &[
    ("mod+shift+o", "GotoFile"),
    ("mod+o", "GotoClass"),
    ("mod+b", "GotoDeclaration"),
    ("alt+f7", "FindUsages"),
    ("mod+r", "Rename"),
    ("alt+enter", "QuickFix"),
    ("mod+g", "GotoLine"),
    ("mod+shift+f", "FindInPath"),
    ("mod+e", "DeleteLine"),
    ("mod+shift+down", "Duplicate"),
    ("alt+shift+up", "MoveLineUp"),
    ("alt+shift+down", "MoveLineDown"),
    ("alt+shift+f", "Reformat"),
    ("mod+shift+i", "OptimizeImports"),
    ("mod+f12", "FileStructure"),
    ("ctrl+space", "Complete"),
];

/// The bindings of preset `name`; none for `default` and unknown names.
pub fn preset(name: &str) -> &'static [(&'static str, &'static str)] {
    match name.to_ascii_lowercase().as_str() {
        "vscode" | "vs code" | "code" => VSCODE,
        "eclipse" => ECLIPSE,
        "netbeans" => NETBEANS,
        _ => &[],
    }
}

/// The command named `name` (as in the bindings: `FindAction`).
pub fn command_named(name: &str) -> Option<Cmd> {
    Cmd::ALL
        .into_iter()
        .find(|c| format!("{c:?}").eq_ignore_ascii_case(name))
}

/// Chords bound by `preset_name` then `custom` (later wins), each to a
/// command or to nothing (`none` unbinds the built-in shortcut).
pub fn build(preset_name: &str, custom: &[(String, String)]) -> HashMap<Chord, Option<Cmd>> {
    let mut map = HashMap::new();
    let pairs = preset(preset_name)
        .iter()
        .map(|(c, n)| ((*c).to_string(), (*n).to_string()))
        .chain(custom.iter().cloned());
    for (chord, name) in pairs {
        let Some(chord) = Chord::parse(&chord) else {
            crate::log::warn(format_args!("ignoring key binding `{chord}`"));
            continue;
        };
        if name.eq_ignore_ascii_case("none") {
            map.insert(chord, None);
        } else if let Some(cmd) = command_named(&name) {
            map.insert(chord, Some(cmd));
        } else {
            crate::log::warn(format_args!("ignoring unknown command `{name}`"));
        }
    }
    map
}

/// The bindings from the user's settings, built once.
pub fn overrides() -> &'static HashMap<Chord, Option<Cmd>> {
    static MAP: OnceLock<HashMap<Chord, Option<Cmd>>> = OnceLock::new();
    MAP.get_or_init(|| {
        let settings = crate::settings::settings();
        let custom: Vec<(String, String)> = settings
            .keybindings
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        build(&settings.keymap, &custom)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_should_name_real_commands() {
        for table in [VSCODE, ECLIPSE, NETBEANS] {
            for (chord, name) in table {
                assert!(Chord::parse(chord).is_some(), "{chord}");
                assert!(command_named(name).is_some(), "{name}");
            }
        }
    }

    #[test]
    fn chords_should_match_key_presses() {
        let parsed = Chord::parse("cmd+shift+p").unwrap();
        let pressed = Chord::of(
            &Key::Character("P".into()),
            Modifiers::LOGO | Modifiers::SHIFT,
        )
        .unwrap();
        assert_eq!(parsed, pressed);
    }

    #[test]
    fn named_keys_should_accept_short_names() {
        let parsed = Chord::parse("alt+up").unwrap();
        let pressed = Chord::of(
            &Key::Named(iced::keyboard::key::Named::ArrowUp),
            Modifiers::ALT,
        )
        .unwrap();
        assert_eq!(parsed, pressed);
    }

    #[test]
    fn custom_bindings_should_override_and_unbind() {
        let map = build(
            "vscode",
            &[
                ("f12".into(), "none".into()),
                ("cmd+shift+u".into(), "ToggleCase".into()),
            ],
        );
        assert_eq!(
            (
                map.get(&Chord::parse("f12").unwrap()),
                map.get(&Chord::parse("cmd+shift+u").unwrap())
            ),
            (Some(&None), Some(&Some(Cmd::ToggleCase)))
        );
    }
}
