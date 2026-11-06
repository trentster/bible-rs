//! What the native menu bar's items do, shared by the macOS and Windows menus
//! (`macos_menu.rs`, `windows_menu.rs`): each item's id becomes the same [`Action`] the keyboard
//! path produces.

use std::sync::Mutex;

use eframe::egui;

use crate::actions::{Action, ExportKind, SettingsTab};
use crate::theme::Theme;

/// The items chosen since the last frame.
///
/// A menu click arrives on the platform's own thread, outside any frame, and an idle egui only
/// draws when something asks it to. So the handler wakes the app as well as queueing the item:
/// without the wake, a click on an idle app sits in the queue until something else repaints, and
/// Settings takes as long to open as the reader takes to move.
static CHOSEN: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Queues a chosen item for the next frame, and makes sure there is one.
pub fn chosen(id: String, ctx: &egui::Context) {
    CHOSEN.lock().unwrap_or_else(|e| e.into_inner()).push(id);
    ctx.request_repaint();
}

/// The action of the item with this id.
pub fn action(id: &str) -> Option<Action> {
    Some(match id {
        "settings" => Action::OpenSettings(SettingsTab::Reading),
        "about" => Action::OpenSettings(SettingsTab::About),
        "import_translation" => Action::ImportTranslation,
        "translations" => Action::OpenSettings(SettingsTab::Translations),
        "export_md" => Action::Export(ExportKind::Markdown),
        "export_json" => Action::Export(ExportKind::Json),
        "import_notes" => Action::ImportStudy,
        "theme_light" => Action::SetTheme(Theme::Light),
        "theme_sepia" => Action::SetTheme(Theme::Sepia),
        "theme_dark" => Action::SetTheme(Theme::Dark),
        "theme_cycle" => Action::CycleTheme,
        "bigger" => Action::FontSize(1.0),
        "smaller" => Action::FontSize(-1.0),
        "actual" => Action::FontReset,
        "text" => Action::TextFormat,
        "mode" => Action::ToggleMode,
        "red" => Action::ToggleRedLetters,
        "parallel" => Action::ToggleParallel,
        "sidebar" => Action::ToggleSidebar,
        "focus" => Action::ToggleFocus,
        "jump" => Action::OpenPalette,
        "search" => Action::OpenSearch(None),
        "back" => Action::Back,
        "forward" => Action::Forward,
        "prev" => Action::PrevChapter,
        "next" => Action::NextChapter,
        "bookmark" => Action::ToggleBookmark,
        "compare" => Action::Compare,
        "shortcuts" => Action::ShowShortcuts,
        "quit" => Action::Quit,
        _ => return None,
    })
}

/// Drains menu clicks into this frame's actions (macOS; Windows queues them itself to wake egui).
pub fn poll(actions: &mut Vec<Action>) {
    let chosen = std::mem::take(&mut *CHOSEN.lock().unwrap_or_else(|e| e.into_inner()));
    actions.extend(chosen.iter().filter_map(|id| action(id)));
}
