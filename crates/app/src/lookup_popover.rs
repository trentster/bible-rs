//! Looking a word up: what to ask about, where the popover hangs, and the popover itself.
//!
//! The whole feature lives here, so `app.rs` only hands over: it says "look this up" and draws
//! whatever comes back. Two ways in, one rule — the selected words if there are any (they may be a
//! phrase with its own entry), otherwise the single word under the pointer.
//!
//! Anchored to the words or the click, like Compare and the note popover, so the page underneath
//! never moves. Each entry names the dictionary that answered, and a word no dictionary holds says
//! so plainly instead of showing an empty frame.

use eframe::egui::{self, Key, Order, Rect, Sense, Stroke, pos2, vec2};

use bible_core::VerseRef;
use bible_core::library::Library;

use crate::actions::Action;
use crate::app::Settings;
use crate::icons;
use crate::pages::text;
use crate::reader::{Reader, TextPos};
use crate::theme::{self, Palette};
use crate::widgets;

const WIDTH: f32 = 320.0;
const BEAK: egui::Vec2 = vec2(18.0, 9.0);
/// How many words one lookup may cover: a phrase, never a whole verse.
const MAX_WORDS: usize = 4;

pub struct LookUp {
    /// The words being looked up, as the text has them.
    pub word: String,
    /// The reference they were chosen at, for the popover's second line.
    pub reference: String,
}

impl LookUp {
    pub fn new(word: String, reference: String) -> Self {
        Self { word, reference }
    }

    /// The lookup that the selection or the last right-click calls for, if there is one.
    pub fn here(reader: &Reader, library: &Library, settings: &Settings) -> Option<Self> {
        if let Some((word, verse)) = chosen(reader, library) {
            return Some(Self::new(word, verse.to_string()));
        }
        let at = reader.clicked_text()?;
        let word = word_at(reader, library, settings, at)?;
        Some(Self::new(word, at.verse.to_string()))
    }

    /// The popover's lines under the heading. The dictionaries fill this in next; until then it
    /// says so plainly rather than showing an empty frame.
    fn body(&self) -> Vec<(String, bool)> {
        vec![("No definition yet".to_string(), false)]
    }

    /// Draws the popover hanging from the looked-up words; returns `true` to close.
    pub fn show(&mut self, ctx: &egui::Context, p: &Palette, reader: &Reader, _actions: &mut Vec<Action>) -> bool {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
            return true;
        }
        let screen = ctx.content_rect();
        let anchor = anchor(reader).unwrap_or_else(|| crate::note_popover::fallback_anchor(screen));
        let body = self.body();
        let chrome = 64.5 + 14.0 + 16.0;
        let height = chrome + body.len() as f32 * 18.0;

        let below = screen.bottom() - (anchor.bottom() + 3.0 + BEAK.y) - 16.0;
        let above = anchor.top() - 3.0 - BEAK.y - (screen.top() + 52.0) - 8.0;
        let downward = below >= height || below >= above;
        let left =
            (anchor.center().x - WIDTH / 2.0).clamp(screen.left() + 12.0, screen.right() - 12.0 - WIDTH);
        let top = if downward { anchor.bottom() + 3.0 + BEAK.y } else { anchor.top() - 3.0 - BEAK.y - height };
        let rect = Rect::from_min_size(pos2(left, top), vec2(WIDTH, height));

        let mut close = false;
        egui::Area::new(egui::Id::new("lookup-popover"))
            .order(Order::Foreground)
            .fixed_pos(rect.min)
            .show(ctx, |ui| {
                ui.allocate_rect(Rect::from_min_size(rect.min, rect.size()), Sense::hover());
                let painter = ui.painter().clone();
                p.shadows.popover.paint(&painter, rect, 16.0);
                painter.rect_filled(rect, 16.0, p.surface);
                // The beak, toward the word.
                let tip_x = (rect.left() + 60.0).max(anchor.left() + 24.0).min(rect.right() - 24.0);
                let (edge, dir) =
                    if downward { (rect.top() + 0.5, -1.0) } else { (rect.bottom() - 0.5, 1.0) };
                let tip = pos2(tip_x, edge + dir * BEAK.y);
                let l = pos2(tip_x - BEAK.x / 2.0, edge);
                let r = pos2(tip_x + BEAK.x / 2.0, edge);
                painter.add(egui::Shape::convex_polygon(vec![l, tip, r], p.surface, Stroke::NONE));
                painter.add(egui::Shape::line(vec![l, tip, r], Stroke::new(0.75, p.border)));

                let x = rect.left() + 16.0;
                text(&painter, x, rect.top() + 14.0, 20.0, &self.word, theme::ui_semibold(20.0), p.ink, false);
                text(&painter, x, rect.top() + 40.0, 12.0, &self.reference, theme::ui_font(12.0), p.ink_2, false);
                let close_r =
                    Rect::from_min_size(pos2(rect.right() - 12.0 - 28.0, rect.top() + 16.0), vec2(28.0, 28.0));
                if widgets::round_button(ui, p, close_r, icons::close, ui.id().with("lookup-close")).clicked() {
                    close = true;
                }
                painter.hline(rect.x_range(), rect.top() + 64.5, Stroke::new(1.0, p.separator));

                let mut y = rect.top() + 64.5 + 14.0;
                for (line, is_body) in &body {
                    let (font, color) = if *is_body { (theme::ui_font(13.0), p.ink) } else { (theme::ui_font(11.5), p.ink_2) };
                    text(&painter, x, y, if *is_body { 13.0 } else { 11.5 }, line, font, color, false);
                    y += 18.0;
                }
            });
        close
    }
}

/// What a lookup at `pt` would ask about: the selected words, else the word under the pointer.
/// `None` when neither gives anything worth looking up, and the menu then leaves the item out.
pub fn target(reader: &Reader, library: &Library, settings: &Settings, pt: egui::Pos2) -> Option<String> {
    if let Some((word, _)) = chosen(reader, library) {
        return Some(word);
    }
    word_at(reader, library, settings, reader.text_pos(pt)?)
}

/// The words dragged over, when they are few enough to be a lookup rather than a whole verse.
fn chosen(reader: &Reader, library: &Library) -> Option<(String, VerseRef)> {
    let (span, id) = reader.text_span()?;
    if span.first != span.last {
        return None;
    }
    let t = library.load(id).ok()?;
    let v = t.find(span.first)?;
    let chars: Vec<char> = v.text.chars().collect();
    let start = span.start.min(chars.len());
    let end = span.end.max(start).min(chars.len());
    let word: String = chars[start..end].iter().collect();
    let word = word.trim().to_string();
    let few = word.split_whitespace().count() <= MAX_WORDS;
    (few && !word.is_empty()).then_some((word, span.first))
}

/// The word around a place in the text, trimmed of punctuation.
fn word_at(reader: &Reader, library: &Library, settings: &Settings, at: TextPos) -> Option<String> {
    let chosen = reader.words_in();
    let id = if chosen.is_empty() { settings.translation.as_str() } else { chosen };
    let t = library.load(id).ok()?;
    let v = t.find(at.verse)?;
    let chars: Vec<char> = v.text.chars().collect();
    let word_char = |c: char| c.is_alphanumeric() || c == '\'' || c == '-';
    let mut start = at.offset.min(chars.len());
    let mut end = start;
    while start > 0 && word_char(chars[start - 1]) {
        start -= 1;
    }
    while end < chars.len() && word_char(chars[end]) {
        end += 1;
    }
    let word: String = chars[start..end].iter().collect();
    (!word.is_empty()).then_some(word)
}

/// Where the popover hangs from: the chosen words, or the word last right-clicked.
fn anchor(reader: &Reader) -> Option<Rect> {
    if reader.text_span().is_some()
        && let Some(r) = reader.selection_rect
    {
        return Some(r);
    }
    let pt = reader.clicked_text().and_then(|at| reader.point_at(at))?;
    Some(Rect::from_min_size(pt - vec2(0.0, 9.0), vec2(2.0, 18.0)))
}
