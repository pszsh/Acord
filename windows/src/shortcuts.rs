use winit::keyboard::{Key, ModifiersState, SmolStr};

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub enum MenuAction {
    NewNote,
    Open,
    Save,
    SaveAs,
    Quit,
    Bold,
    Italic,
    InsertTable,
    Evaluate,
    LiveMode,
    EditorMode,
    ViewMode,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Find,
    Settings,
    ExportCrate,
    Print,
    ToggleBrowser,
}

pub fn match_shortcut(modifiers: ModifiersState, key: &Key) -> Option<MenuAction> {
    if modifiers.alt_key() && !modifiers.control_key() && !modifiers.super_key() {
        if let Key::Character(s) = key {
            if ascii_lower(s) == 'b' {
                return Some(MenuAction::ToggleBrowser);
            }
        }
    }

    if !modifiers.control_key() {
        return None;
    }
    let shift = modifiers.shift_key();

    match key {
        Key::Character(s) => match (shift, ascii_lower(s)) {
            (false, 'n') => Some(MenuAction::NewNote),
            (false, 'o') => Some(MenuAction::Open),
            (false, 's') => Some(MenuAction::Save),
            (true,  's') => Some(MenuAction::SaveAs),
            (false, 'q') => Some(MenuAction::Quit),
            (false, 'b') => Some(MenuAction::Bold),
            (false, 'i') => Some(MenuAction::Italic),
            (false, 't') => Some(MenuAction::InsertTable),
            (false, 'f') => Some(MenuAction::Find),
            (false, 'e') => Some(MenuAction::Evaluate),
            (true,  'e') => Some(MenuAction::ExportCrate),
            (false, 'p') => Some(MenuAction::Print),
            (false, ',') => Some(MenuAction::Settings),
            (false, '=') | (false, '+') => Some(MenuAction::ZoomIn),
            (false, '-') => Some(MenuAction::ZoomOut),
            (true,  '0') => Some(MenuAction::ZoomReset),
            _ => None,
        },
        _ => None,
    }
}

fn ascii_lower(s: &SmolStr) -> char {
    s.chars().next().map(|c| c.to_ascii_lowercase()).unwrap_or('\0')
}
