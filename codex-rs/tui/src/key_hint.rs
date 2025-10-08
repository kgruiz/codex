use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use crossterm::event::KeyModifiers;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Span;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct KeyBinding {
    key: KeyCode,
    modifiers: KeyModifiers,
}

impl KeyBinding {
    pub(crate) const fn new(key: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { key, modifiers }
    }

    pub fn is_press(&self, event: KeyEvent) -> bool {
        self.key == event.code
            && self.modifiers == event.modifiers
            && (event.kind == KeyEventKind::Press || event.kind == KeyEventKind::Repeat)
    }
}

pub(crate) const fn plain(key: KeyCode) -> KeyBinding {
    KeyBinding::new(key, KeyModifiers::NONE)
}

pub(crate) const fn alt(key: KeyCode) -> KeyBinding {
    KeyBinding::new(key, KeyModifiers::ALT)
}

pub(crate) const fn shift(key: KeyCode) -> KeyBinding {
    KeyBinding::new(key, KeyModifiers::SHIFT)
}

pub(crate) const fn ctrl(key: KeyCode) -> KeyBinding {
    KeyBinding::new(key, KeyModifiers::CONTROL)
}

fn modifiers_to_string(modifiers: KeyModifiers) -> String {
    if modifiers.is_empty() {
        return String::new();
    }

    let mut parts: Vec<&'static str> = Vec::new();

    if cfg!(target_os = "macos") {
        if modifiers.contains(KeyModifiers::CONTROL) {
            parts.push("Control ⌃");
        }
        if modifiers.contains(KeyModifiers::ALT) {
            parts.push("Option ⌥");
        }
        if modifiers.contains(KeyModifiers::SHIFT) {
            parts.push("Shift ⇧");
        }
        if modifiers.contains(KeyModifiers::SUPER) {
            parts.push("Command ⌘");
        }
    } else {
        if modifiers.contains(KeyModifiers::CONTROL) {
            parts.push("Ctrl");
        }
        if modifiers.contains(KeyModifiers::ALT) {
            parts.push("Alt");
        }
        if modifiers.contains(KeyModifiers::SHIFT) {
            parts.push("Shift");
        }
        if modifiers.contains(KeyModifiers::SUPER) {
            parts.push("Windows");
        }
    }

    if parts.is_empty() {
        String::new()
    } else {
        format!("{} + ", parts.join(" + "))
    }
}

fn macos_fn_note(key: KeyCode) -> Option<&'static str> {
    if !cfg!(target_os = "macos") {
        return None;
    }

    match key {
        KeyCode::PageUp => Some("Fn+↑"),
        KeyCode::PageDown => Some("Fn+↓"),
        KeyCode::Home => Some("Fn+←"),
        KeyCode::End => Some("Fn+→"),
        _ => None,
    }
}

fn key_to_string(key: KeyCode) -> String {
    let base = match key {
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Char(' ') => "Space".to_string(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Up => "↑".to_string(),
        KeyCode::Down => "↓".to_string(),
        KeyCode::Left => "←".to_string(),
        KeyCode::Right => "→".to_string(),
        KeyCode::PageUp => "Page Up".to_string(),
        KeyCode::PageDown => "Page Down".to_string(),
        KeyCode::Home => "Home".to_string(),
        KeyCode::End => "End".to_string(),
        _ => format!("{key}").to_ascii_lowercase(),
    };

    if let Some(note) = macos_fn_note(key) {
        format!("{base} ({note})")
    } else {
        base
    }
}

impl From<KeyBinding> for Span<'static> {
    fn from(binding: KeyBinding) -> Self {
        (&binding).into()
    }
}
impl From<&KeyBinding> for Span<'static> {
    fn from(binding: &KeyBinding) -> Self {
        let KeyBinding { key, modifiers } = binding;
        let modifiers = modifiers_to_string(*modifiers);
        let key = key_to_string(*key);
        Span::styled(format!("{modifiers}{key}"), key_hint_style())
    }
}

fn key_hint_style() -> Style {
    Style::default().dim()
}
