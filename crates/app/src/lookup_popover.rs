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

/// What the popover shows: the lines to scroll, the dictionary line pinned at its foot, and — when
/// neither dictionary has the word — the word to search the web for.
struct Body {
    lines: Vec<(String, bool)>,
    pinned: Option<String>,
    search_for: Option<String>,
}

impl LookUp {
    pub fn new(word: String, reference: String) -> Self {
        Self { word, reference }
    }

    /// The lookup that the selection or the last right-click calls for, if there is one.
    pub fn here(reader: &Reader, library: &Library, settings: &Settings) -> Option<Self> {
        match chosen(reader, library) {
            Chosen::Words(word, verse) => return Some(Self::new(word, verse.to_string())),
            Chosen::TooLong => return None,
            Chosen::NoSuchWords => {}
        }
        let at = reader.clicked_text()?;
        let word = word_at(reader, library, settings, at)?;
        Some(Self::new(word, at.verse.to_string()))
    }

    /// The popover's body: WordNet's senses, or Easton's opening sentences for a Bible word, or a
    /// plain "no definition" with a way to look further.
    ///
    /// This is where the bundled dictionaries are first parsed, so nothing is read until a word is
    /// actually asked about — the startup path never touches them.
    fn body(&self) -> Body {
        let found = bible_core::lexicon::define(&self.word);
        if found.is_empty() {
            return Body {
                lines: vec![("No definition found".to_string(), false)],
                pinned: None,
                search_for: Some(self.word.clone()),
            };
        }
        let mut lines = Vec::new();
        // The KJV's spelling is not always the dictionary's headword, and the reader deserves to
        // know why they are being shown a different word.
        if let Some(headword) = &found.shown_as {
            lines.push((format!("“{}” — shown as {}", self.word, headword), true));
        }
        let one_source = found.entries.iter().all(|e| e.source == found.entries[0].source);
        let mut pinned = None;
        for entry in &found.entries {
            if one_source {
                pinned = Some(entry.source.label().to_string());
            }
            for paragraph in entry.body.split('\n').filter(|p| !p.trim().is_empty()) {
                lines.push((paragraph.trim().to_string(), false));
            }
            if !one_source {
                lines.push((entry.source.label().to_string(), true));
            }
        }
        Body { lines, pinned, search_for: None }
    }

    /// Draws the popover hanging from the looked-up words; returns `true` to close.
    pub fn show(&mut self, ctx: &egui::Context, p: &Palette, reader: &Reader, _actions: &mut Vec<Action>) -> bool {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
            return true;
        }
        let screen = ctx.content_rect();
        let anchor = anchor(reader).unwrap_or_else(|| crate::note_popover::fallback_anchor(screen));
        let body = self.body();
        // A dictionary line at the foot takes a little padding, whatever the entry's length.
        let foot = if body.pinned.is_some() { 24.0 } else { 0.0 };
        // A search button, when there is nothing to show.
        let search_height = if body.search_for.is_some() { 42.0 } else { 0.0 };
        // Wrapped text, so a long entry is measured rather than counted in fixed-height lines.
        let wrap = WIDTH - 32.0;
        let measured: Vec<(std::sync::Arc<egui::Galley>, bool)> = body
            .lines
            .iter()
            .map(|(line, is_source)| {
                let font = if *is_source { theme::ui_font(11.5) } else { theme::ui_font(13.0) };
                (ctx.fonts_mut(|f| f.layout(line.clone(), font, p.ink, wrap)), *is_source)
            })
            .collect();
        let mut text_height = 0.0;
        for (i, (galley, is_source)) in measured.iter().enumerate() {
            if *is_source {
                text_height += 4.0;
            }
            text_height += galley.size().y + 4.0;
            let next_starts_an_entry = measured.get(i + 1).is_some_and(|(_, next)| !*next);
            if *is_source && next_starts_an_entry {
                text_height += 10.0;
            }
        }
        // A long entry scrolls rather than growing the popover off the screen.
        let cap = (screen.height() * 0.42).max(80.0);
        let body_height = text_height.min(cap);
        let chrome = 64.5 + 14.0 + 14.0 + foot + search_height;
        let height = chrome + body_height;

        let below = screen.bottom() - (anchor.bottom() + 3.0 + BEAK.y) - 16.0;
        let above = anchor.top() - 3.0 - BEAK.y - (screen.top() + 52.0) - 8.0;
        let downward = below >= height || below >= above;
        let left =
            (anchor.center().x - WIDTH / 2.0).clamp(screen.left() + 12.0, screen.right() - 12.0 - WIDTH);
        let top = if downward { anchor.bottom() + 3.0 + BEAK.y } else { anchor.top() - 3.0 - BEAK.y - height };
        let rect = Rect::from_min_size(pos2(left, top), vec2(WIDTH, height));

        let mut close = false;
        let area = egui::Area::new(egui::Id::new("lookup-popover"))
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

                let body_rect = Rect::from_min_max(
                    pos2(rect.left() + 16.0, rect.top() + 64.5 + 12.0),
                    pos2(rect.right() - 16.0, rect.bottom() - 14.0 - foot - search_height),
                );
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body_rect));
                child.set_clip_rect(body_rect.intersect(ctx.content_rect()));
                child.spacing_mut().item_spacing.y = 4.0;
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(&mut child, |ui| {
                    for (galley, is_source) in &measured {
                        if *is_source {
                            ui.add_space(0.0);
                            let text = egui::WidgetText::Galley(galley.clone());
                            ui.add(egui::Label::new(text.color(p.ink_2)).wrap());
                            ui.add_space(10.0);
                        } else {
                            let text = egui::WidgetText::Galley(galley.clone());
                            ui.add(egui::Label::new(text).wrap());
                        }
                    }
                });
                // Neither dictionary has the word: offer to search the web. The app itself makes no
                // network request — it hands the word to the reader's own browser.
                if let Some(word) = &body.search_for {
                    let button = Rect::from_min_size(
                        pos2(rect.left() + 16.0, rect.bottom() - 14.0 - foot - 34.0),
                        vec2(widgets::secondary_button_width(ui, "Search the web"), 28.0),
                    );
                    if widgets::secondary_button(ui, p, button, "Search the web", ui.id().with("lookup-web"))
                        .clicked()
                    {
                        crate::app::open_url(&wiktionary_url(word));
                    }
                }
                // The dictionary's name sits at the foot, so a long entry scrolling past does not
                // take the provenance with it. Deliberately no rule above it: a full-width line at
                // the bottom edge reads as a bar with more content behind it to scroll to, which is
                // a lie when the entry is short. Quiet text in the padding reads as a footnote.
                if let Some(label) = &body.pinned {
                    text(
                        &painter,
                        x,
                        rect.bottom() - 15.0,
                        11.0,
                        &format!("Source: {label}"),
                        egui::FontId::new(11.5, theme::family(theme::SERIF_ITALIC)),
                        p.ink_2,
                        false,
                    );
                }
            });
        // A click anywhere outside closes it, as the note popover already behaves.
        close || area.response.clicked_elsewhere()
    }
}

/// A Wiktionary search for a word — the one case where neither dictionary has it. The URL is handed
/// to the reader's browser; the app never makes a network request of its own.
fn wiktionary_url(word: &str) -> String {
    let mut url = String::from("https://en.wiktionary.org/wiki/");
    for byte in word.trim().bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => url.push(byte as char),
            b' ' => url.push('_'),
            _ => url.push_str(&format!("%{byte:02X}")),
        }
    }
    url
}

/// What a lookup at `pt` would ask about, or `None` — and then the menu leaves the item out.
pub fn target(reader: &Reader, library: &Library, settings: &Settings, pt: egui::Pos2) -> Option<String> {
    match chosen(reader, library) {
        Chosen::Words(word, _) => Some(word),
        Chosen::TooLong => None,
        Chosen::NoSuchWords => word_at(reader, library, settings, reader.text_pos(pt)?),
    }
}

/// What the words dragged over amount to.
enum Chosen {
    /// Nothing is selected, so a lookup falls back to the word under the pointer.
    NoSuchWords,
    /// Words are selected, but too many to be a phrase: nothing is asked, and no fallback is made.
    TooLong,
    /// The words to ask about, and the verse they are in.
    Words(String, VerseRef),
}

/// The words dragged over, when they are few enough to be a lookup rather than a whole verse.
fn chosen(reader: &Reader, library: &Library) -> Chosen {
    let Some((span, id)) = reader.text_span() else { return Chosen::NoSuchWords };
    if span.first != span.last {
        return Chosen::TooLong;
    }
    let Ok(t) = library.load(id) else { return Chosen::NoSuchWords };
    let Some(v) = t.find(span.first) else { return Chosen::NoSuchWords };
    let chars: Vec<char> = v.text.chars().collect();
    let start = span.start.min(chars.len());
    let end = span.end.max(start).min(chars.len());
    let word: String = chars[start..end].iter().collect();
    let word = word.trim().to_string();
    if word.is_empty() {
        return Chosen::NoSuchWords;
    }
    if word.split_whitespace().count() > MAX_WORDS {
        return Chosen::TooLong;
    }
    Chosen::Words(word, span.first)
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
