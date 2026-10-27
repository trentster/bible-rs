//! The book picker under the toolbar's location button: the Old and New Testament books side by
//! side, then a book's chapters. Both are grids of menu cells in the menu frame (the `menu`
//! tokens): highlights inset from the edges like menu rows, names in the text column under their
//! section header, and the book and chapter being read selected.
//!
//! Arrow keys move a highlight through the grid (the books spatially, across both testaments;
//! the chapters in order), return chooses, backspace goes back to the books and escape closes.

use bible_core::books::{BookId, ChapterIdx, FIRST_NT_BOOK};
use bible_core::model::TranslationMeta;
use eframe::egui::{self, Align2, Id, Key, Modifiers, Rect, Response, Sense, Ui, Vec2, pos2, vec2};

use crate::actions::Action;
use crate::menu::{self, Cell, ITEM_HEIGHT, PADDING_X, SEPARATOR_HEIGHT};
use crate::theme::{Palette, icon, menu as m, text};

const POPUP: &str = "book-picker";
const BOOK_COLUMNS: usize = 3;
const CHAPTER_COLUMNS: usize = 10;
/// Between the testaments: twice the menu's side padding, so the two grids read as groups while
/// the cells within each sit edge to edge like menu rows.
const GROUP_GAP: f32 = 2.0 * PADDING_X;
/// Longer names show the book's abbreviation.
const MAX_NAME: usize = 12;
/// Width the books leave spare each side of a narrow window when their cells narrow to fit.
const WINDOW_MARGIN: f32 = 8.0;
const BOOKS: usize = 66;
/// The verses of the chapter just chosen: a small cell each, ten to a row, and no more than
/// `VERSE_ROWS` rows before it scrolls — Psalm 119 holds 176 and would otherwise make a card
/// taller than the window.
const VERSE_CELL_HEIGHT: f32 = 24.0;
const VERSE_ROWS: usize = 8;
/// The verse block sits on its own faint fill, so nothing in it reads as another chapter.
const BLOCK_GAP: f32 = 6.0;
const BLOCK_INSET: f32 = 2.0;
const BLOCK_PAD: f32 = 7.0;
const BLOCK_HEADING: f32 = 17.0;
const BLOCK_RADIUS: f32 = 8.0;

/// Which grid the picker shows and where its highlight is, while it's open.
#[derive(Clone, Copy, Default)]
struct State {
    /// The book whose chapters show; `None` shows the books.
    book: Option<BookId>,
    /// The chapter whose verses show under its grid, once the reader has chosen one. Chosen is not
    /// the same as read: the jump happens on the choosing, and the verses let a verse be picked
    /// instead without hunting the page.
    chapter: Option<u8>,
    cursor: Option<Spot>,
}

#[derive(Clone, Copy, PartialEq)]
enum Spot {
    /// "Books", above the chapters.
    Back,
    /// A book (its index) or a chapter (from 0).
    Cell(usize),
    /// A verse of the chosen chapter (from 0).
    Verse(usize),
}

/// Keys the picker reads while open.
#[derive(Default)]
struct Keys {
    dx: isize,
    dy: isize,
    enter: bool,
    back: bool,
}

/// Whether the picker is open (it takes the arrow keys, return and escape then).
pub fn is_open(ctx: &egui::Context) -> bool {
    egui::Popup::is_id_open(ctx, Id::new(POPUP))
}

/// The picker, opening from `anchor`; `here` is the chapter being read.
pub fn show(
    anchor: &Response,
    p: &Palette,
    meta: Option<&TranslationMeta>,
    translation: Option<&bible_core::model::Translation>,
    here: ChapterIdx,
    actions: &mut Vec<Action>,
) {
    let state_id = Id::new(POPUP).with("state");
    let shown = menu::popup(anchor, p)
        .id(Id::new(POPUP))
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            let mut state: State = ui.data(|d| d.get_temp(state_id)).unwrap_or_default();
            let keys = ui.input_mut(|i| Keys {
                dx: i.consume_key(Modifiers::NONE, Key::ArrowRight) as isize
                    - i.consume_key(Modifiers::NONE, Key::ArrowLeft) as isize,
                dy: i.consume_key(Modifiers::NONE, Key::ArrowDown) as isize
                    - i.consume_key(Modifiers::NONE, Key::ArrowUp) as isize,
                enter: i.consume_key(Modifiers::NONE, Key::Enter),
                back: i.consume_key(Modifiers::NONE, Key::Backspace),
            });
            match state.book {
                None => books(ui, p, meta, here, &keys, &mut state, actions),
                Some(b) => chapters(ui, p, meta, translation, b, here, &keys, &mut state, actions),
            }
            ui.data_mut(|d| d.insert_temp(state_id, state));
        });
    // Closed: it opens on the books again.
    if shown.is_none() {
        anchor.ctx.data_mut(|d| d.remove::<State>(state_id));
    }
}

fn book_name(meta: Option<&TranslationMeta>, b: BookId) -> String {
    let name = meta.map_or(b.name(), |m| m.book_short(b));
    if name.chars().count() > MAX_NAME { b.info().abbrev.to_string() } else { name.to_string() }
}

/// A book's column across both testaments (0–5) and its row.
fn book_spot(i: usize) -> (usize, usize) {
    let first_nt = FIRST_NT_BOOK.index();
    let (group, k) = if i < first_nt { (0, i) } else { (1, i - first_nt) };
    (group * BOOK_COLUMNS + k % BOOK_COLUMNS, k / BOOK_COLUMNS)
}

/// The book at a column and row, if there is one.
fn book_at(col: usize, row: usize) -> Option<usize> {
    let first_nt = FIRST_NT_BOOK.index();
    let (start, end) = match col / BOOK_COLUMNS {
        0 => (0, first_nt),
        1 => (first_nt, BOOKS),
        _ => return None,
    };
    let i = start + row * BOOK_COLUMNS + col % BOOK_COLUMNS;
    (i < end).then_some(i)
}

/// Where the arrow keys take the highlight among `n` chapters: along them in order, a row up or
/// down, and up from the first row to "Books". It starts from the chapter being read.
fn chapter_step(cursor: Option<Spot>, current: Option<usize>, n: usize, keys: &Keys) -> Spot {
    let from = match (cursor, current) {
        (Some(Spot::Back), _) => return if keys.dy > 0 { Spot::Cell(0) } else { Spot::Back },
        // The verse block walks its own arrows.
        (Some(Spot::Verse(_)), _) => return Spot::Cell(current.unwrap_or(0)),
        (Some(Spot::Cell(i)), _) | (None, Some(i)) => i,
        // Nothing highlighted or selected yet: the first chapter.
        (None, None) => return Spot::Cell(0),
    };
    match from.checked_add_signed(keys.dx + keys.dy * CHAPTER_COLUMNS as isize) {
        Some(j) if j < n => Spot::Cell(j),
        None if keys.dy < 0 => Spot::Back,
        _ => Spot::Cell(from),
    }
}

/// The cell under the pointer when it moved this frame (`None` off the cells), so the pointer and
/// the keyboard share one highlight.
fn follow_pointer(ui: &Ui, state: &mut State, hovered: Option<Spot>) {
    if ui.input(|i| i.pointer.delta() != Vec2::ZERO) {
        state.cursor = hovered;
    }
}

fn cell_response(ui: &Ui, r: Rect, id: Id, label: &str, selected: bool) -> Response {
    let resp = ui.interact(r, id, Sense::CLICK);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, label));
    resp
}

/// The two testaments, three books to a row under their headers.
fn books(
    ui: &mut Ui,
    p: &Palette,
    meta: Option<&TranslationMeta>,
    here: ChapterIdx,
    keys: &Keys,
    state: &mut State,
    actions: &mut Vec<Action>,
) {
    let names: Vec<String> = (0..BOOKS).map(|i| book_name(meta, BookId(i as u8))).collect();
    let font = text::UI.font();
    let widest = names
        .iter()
        .map(|n| ui.painter().layout_no_wrap(n.clone(), font.clone(), p.ink).size().x)
        .fold(0.0, f32::max);
    // As wide as the longest name with the text inset each side. A narrow window (down to 620 pt)
    // narrows them to fit, taking from the inset after the names.
    let room = ui.ctx().content_rect().width() - 2.0 * (WINDOW_MARGIN + PADDING_X) - GROUP_GAP;
    let fit = (room / (2 * BOOK_COLUMNS) as f32).floor();
    let width = (widest + 2.0 * m::ITEM_PADDING).ceil().min(fit).max((widest + m::ITEM_PADDING).ceil());
    let cell = vec2(width, ITEM_HEIGHT);
    let group = BOOK_COLUMNS as f32 * cell.x;
    let rows = FIRST_NT_BOOK.index().div_ceil(BOOK_COLUMNS);
    let size = vec2(2.0 * group + GROUP_GAP, m::HEADER_HEIGHT + rows as f32 * cell.y);
    let (area, _) = ui.allocate_exact_size(size, Sense::hover());
    let left = |col: usize| area.left() + (col / BOOK_COLUMNS) as f32 * (group + GROUP_GAP);
    for (g, title) in ["Old Testament", "New Testament"].into_iter().enumerate() {
        let r = Rect::from_min_size(pos2(left(g * BOOK_COLUMNS), area.top()), vec2(group, m::HEADER_HEIGHT));
        menu::paint_header(ui.painter(), p, r, title);
    }

    let current = here.book().index();
    if keys.dx != 0 || keys.dy != 0 {
        let from = match state.cursor {
            Some(Spot::Cell(i)) => i,
            _ => current,
        };
        let (col, row) = book_spot(from);
        if let Some(i) = col
            .checked_add_signed(keys.dx)
            .zip(row.checked_add_signed(keys.dy))
            .and_then(|(c, r)| book_at(c, r))
        {
            state.cursor = Some(Spot::Cell(i));
        }
    }

    let cells: Vec<(Rect, Response)> = (0..BOOKS)
        .map(|i| {
            let (col, row) = book_spot(i);
            let min = pos2(
                left(col) + (col % BOOK_COLUMNS) as f32 * cell.x,
                area.top() + m::HEADER_HEIGHT + row as f32 * cell.y,
            );
            let r = Rect::from_min_size(min, cell);
            (r, cell_response(ui, r, ui.id().with(("book", i)), &names[i], i == current))
        })
        .collect();
    follow_pointer(ui, state, cells.iter().position(|(_, r)| r.hovered()).map(Spot::Cell));
    for (i, (r, resp)) in cells.iter().enumerate() {
        let cell = Cell {
            icon: None,
            label: &names[i],
            centred: false,
            selected: i == current,
            lit: state.cursor == Some(Spot::Cell(i)),
            pressed: resp.is_pointer_button_down_on(),
        };
        menu::paint_cell(ui.painter(), p, *r, cell);
    }

    let by_key = keys.enter.then_some(match state.cursor {
        Some(Spot::Cell(i)) => i,
        _ => current,
    });
    let Some(i) = cells.iter().position(|(_, r)| r.clicked()).or(by_key) else {
        return;
    };
    let b = BookId(i as u8);
    if b.chapters() == 1 {
        actions.push(Action::Jump { chapter: b.first_chapter(), verse: None, flash: false });
        ui.close();
    } else {
        state.book = Some(b);
        state.chapter = None;
        // From the keyboard, the highlight lands on the chapter being read or the first.
        let ch = if here.book() == b { here.chapter() as usize - 1 } else { 0 };
        state.cursor = by_key.map(|_| Spot::Cell(ch));
    }
}

/// "Books" and the book's name, then its chapters ten to a row.
#[allow(clippy::too_many_arguments)]
fn chapters(
    ui: &mut Ui,
    p: &Palette,
    meta: Option<&TranslationMeta>,
    translation: Option<&bible_core::model::Translation>,
    b: BookId,
    here: ChapterIdx,
    keys: &Keys,
    state: &mut State,
    actions: &mut Vec<Action>,
) {
    let n = b.chapters() as usize;
    let font = text::UI.font();
    let painter = ui.painter().clone();
    let digits = painter.layout_no_wrap("000".into(), font.clone(), p.ink).size().x;
    let cell = vec2((digits + 2.0 * m::ITEM_PADDING).ceil(), ITEM_HEIGHT);
    let books_label = painter.layout_no_wrap("Books".into(), font, p.ink).size().x;
    let back_width = (icon::MENU + m::ICON_GAP + books_label + 2.0 * m::ITEM_PADDING).ceil();
    let rows = n.div_ceil(CHAPTER_COLUMNS);

    // The verses of the chapter just chosen. Choosing jumps the reader there AND opens these, so a
    // verse can be picked instead of hunted on the page; the heading names where the jump landed,
    // because the page itself is behind the card.
    let chosen = state.chapter.filter(|&c| c > 0 && c as usize <= n);
    let verses = match (chosen, translation) {
        (Some(c), Some(t)) => ChapterIdx::new(b, c).map_or(0, |ci| t.last_verse(ci) as usize),
        _ => 0,
    };
    // Every row the chapter has: the block scrolls through them, so the content is all of
    // them while the block itself shows `VERSE_ROWS` at most.
    let all_rows = verses.div_ceil(CHAPTER_COLUMNS);
    let block = (chosen.is_some() && verses > 0).then(|| {
        vec2(
            CHAPTER_COLUMNS as f32 * cell.x - 2.0 * BLOCK_INSET,
            BLOCK_PAD + BLOCK_HEADING + verse_rows(verses) as f32 * VERSE_CELL_HEIGHT + BLOCK_PAD,
        )
    });
    let size = vec2(
        CHAPTER_COLUMNS as f32 * cell.x,
        ITEM_HEIGHT + SEPARATOR_HEIGHT + rows as f32 * cell.y + block.map_or(0.0, |b| BLOCK_GAP + b.y),
    );
    let (area, _) = ui.allocate_exact_size(size, Sense::hover());
    let back = Rect::from_min_size(area.min, vec2(back_width, ITEM_HEIGHT));
    painter.text(
        pos2(back.right() + m::ITEM_PADDING, back.center().y),
        Align2::LEFT_CENTER,
        meta.map_or(b.name(), |m| m.book_short(b)),
        text::UI_TITLE.font(),
        p.ink,
    );
    menu::paint_separator(&painter, p, area.left(), area.right(), back.bottom());
    let top = back.bottom() + SEPARATOR_HEIGHT;

    let current = (here.book() == b).then(|| here.chapter() as usize - 1);
    if (keys.dx != 0 || keys.dy != 0) && !matches!(state.cursor, Some(Spot::Verse(_))) {
        state.cursor = Some(chapter_step(state.cursor, current, n, keys));
    }

    let back_resp = cell_response(ui, back, ui.id().with("books"), "Books", false);
    let cells: Vec<(Rect, Response)> = (0..n)
        .map(|i| {
            let min = pos2(
                area.left() + (i % CHAPTER_COLUMNS) as f32 * cell.x,
                top + (i / CHAPTER_COLUMNS) as f32 * cell.y,
            );
            let r = Rect::from_min_size(min, cell);
            (r, cell_response(ui, r, ui.id().with(("chapter", i)), &(i + 1).to_string(), Some(i) == current))
        })
        .collect();
    let hovered = if back_resp.hovered() {
        Some(Spot::Back)
    } else {
        cells.iter().position(|(_, r)| r.hovered()).map(Spot::Cell)
    };
    follow_pointer(ui, state, hovered);
    let back_cell = Cell {
        icon: Some(crate::icons::chevron_left),
        label: "Books",
        centred: false,
        selected: false,
        lit: state.cursor == Some(Spot::Back),
        pressed: back_resp.is_pointer_button_down_on(),
    };
    menu::paint_cell(&painter, p, back, back_cell);
    for (i, (r, resp)) in cells.iter().enumerate() {
        let label = (i + 1).to_string();
        let cell = Cell {
            icon: None,
            label: &label,
            centred: true,
            selected: Some(i) == current,
            lit: state.cursor == Some(Spot::Cell(i)),
            pressed: resp.is_pointer_button_down_on(),
        };
        menu::paint_cell(ui.painter(), p, *r, cell);
    }

    let enter_back = keys.enter && state.cursor == Some(Spot::Back);
    if back_resp.clicked() || keys.back || enter_back {
        // From the keyboard, the highlight returns to this book.
        let by_key = keys.back || enter_back;
        *state = State { book: None, chapter: None, cursor: by_key.then_some(Spot::Cell(b.index())) };
        return;
    }
    if let (Some(Spot::Verse(i)), Some(ch)) = (state.cursor, chosen) {
        let step = keys.dx + keys.dy * CHAPTER_COLUMNS as isize;
        if step != 0 {
            let to = (i as isize + step).clamp(0, verses as isize - 1) as usize;
            state.cursor = Some(Spot::Verse(to));
        }
        if keys.back {
            state.cursor = Some(Spot::Cell(ch as usize - 1));
            return;
        }
        if keys.enter
            && let Some(ci) = ChapterIdx::new(b, ch)
        {
            actions.push(Action::Jump { chapter: ci, verse: Some(i as u8 + 1), flash: true });
            ui.close();
            return;
        }
    }

    let by_key = keys.enter.then(|| match state.cursor {
        Some(Spot::Cell(i)) => i,
        _ => current.unwrap_or(0),
    });
    if let Some(i) = cells.iter().position(|(_, r)| r.clicked()).or(by_key) {
        actions.push(Action::Jump {
            chapter: ChapterIdx::new(b, i as u8 + 1).expect("a chapter of the book"),
            verse: None,
            flash: false,
        });
        // The card stays open: the chapter is already read, and its verses are one small step on.
        state.chapter = Some(i as u8 + 1);
        if by_key.is_some() {
            state.cursor = Some(Spot::Verse(0));
        }
    }

    let Some(block) = block else { return };
    let Some(ch) = chosen else { return };
    let rect =
        Rect::from_min_size(pos2(area.left() + BLOCK_INSET, top + rows as f32 * cell.y + BLOCK_GAP), block);
    painter.rect_filled(rect, BLOCK_RADIUS, p.fill);
    painter.text(
        pos2(rect.left() + m::ITEM_PADDING, rect.top() + BLOCK_PAD),
        Align2::LEFT_TOP,
        format!("{} {ch} · VERSE", meta.map_or(b.name(), |m| m.book_short(b)).to_uppercase()),
        text::CAPTION.font(),
        p.ink_2,
    );

    // Keyboard: the arrows walk the verses once the chapter is chosen, backspace returns to the
    // chapters, return takes the verse.
    let grid = Rect::from_min_size(
        pos2(rect.left() + BLOCK_INSET, rect.top() + BLOCK_PAD + BLOCK_HEADING),
        vec2(rect.width() - 2.0 * BLOCK_INSET, block.y - BLOCK_PAD - BLOCK_HEADING - BLOCK_PAD),
    );
    let column = grid.width() / CHAPTER_COLUMNS as f32;
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(grid));
    let mut taken = None;
    egui::ScrollArea::vertical()
        .id_salt("verses")
        .max_height(grid.height())
        .auto_shrink([false, false])
        .show(&mut child, |ui| {
            let content = vec2(grid.width(), all_rows as f32 * VERSE_CELL_HEIGHT);
            let (content_rect, _) = ui.allocate_exact_size(content, Sense::hover());
            for i in 0..verses {
                let r = Rect::from_min_size(
                    pos2(
                        content_rect.left() + (i % CHAPTER_COLUMNS) as f32 * column,
                        content_rect.top() + (i / CHAPTER_COLUMNS) as f32 * VERSE_CELL_HEIGHT,
                    ),
                    vec2(column, VERSE_CELL_HEIGHT),
                );
                let resp = cell_response(ui, r, ui.id().with(("verse", i)), &(i + 1).to_string(), false);
                let lit = state.cursor == Some(Spot::Verse(i));
                let wax = resp.is_pointer_button_down_on() || lit;
                if wax {
                    ui.painter().rect_filled(r, m::ITEM_RADIUS, p.fill_strong);
                }
                ui.painter().text(
                    r.center(),
                    Align2::CENTER_CENTER,
                    (i + 1).to_string(),
                    text::SMALL.font(),
                    if wax { p.ink } else { p.ink_2 },
                );
                if resp.clicked() {
                    taken = Some(i);
                }
            }
        });
    if let (Some(i), Some(ci)) = (taken, ChapterIdx::new(b, ch)) {
        actions.push(Action::Jump { chapter: ci, verse: Some(i as u8 + 1), flash: true });
        ui.close();
    }
}

/// How many rows of verses a chosen chapter shows: all of them, up to the cap, and the rest
/// scroll. Psalm 119 is the reason for the cap — 176 verses would otherwise be an eighteen-row
/// card taller than the window.
fn verse_rows(verses: usize) -> usize {
    verses.div_ceil(CHAPTER_COLUMNS).min(VERSE_ROWS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chosen_chapter_shows_its_verses_up_to_the_cap() {
        assert_eq!(verse_rows(0), 0);
        assert_eq!(verse_rows(1), 1);
        assert_eq!(verse_rows(46), 5); // Matthew 22
        assert_eq!(verse_rows(51), 6); // Matthew 24
        assert_eq!(verse_rows(80), VERSE_ROWS); // exactly the cap
        assert_eq!(verse_rows(176), VERSE_ROWS); // Psalm 119 stops at it and scrolls
    }
}
