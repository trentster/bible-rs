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
use bible_core::original;

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
/// The row at the foot that offers the verse's own words, and the rows in the list behind it.
const OFFER_ROW: f32 = 32.0;
const WORD_ROW: f32 = 30.0;

pub struct LookUp {
    /// The words being looked up, as the text has them.
    pub word: String,
    /// The reference they were chosen at, for the popover's second line.
    pub reference: String,
    /// The verse the words are in, when the reference names one.
    at: Option<VerseRef>,
    /// The verse's own words, Hebrew, Aramaic or Greek. Read once, on the first frame the popover
    /// opens, and kept: a lookup that stays open is drawn sixty times a second.
    origin: Option<original::Verse>,
    origin_read: bool,
    /// Which face of the popover is showing.
    view: View,
    /// The dictionary entry behind one of those words, once the reader asks for it.
    entry: Option<original::Entry>,
}

/// The popover's three faces: the English definition, the verse's original words, and one entry.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum View {
    Definition,
    Words,
    Entry,
}

/// How one line of the body reads: prose, a dictionary's name, or the headword of an entry.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Line {
    Text,
    Source,
    Head,
}

/// What the popover shows: the lines to scroll, the dictionary line pinned at its foot, and — when
/// neither dictionary has the word — the word to search the web for.
struct Body {
    lines: Vec<(String, Line)>,
    pinned: Option<String>,
    search_for: Option<String>,
}

impl LookUp {
    pub fn new(word: String, reference: String) -> Self {
        let at = bible_core::reference::parse(&reference).map(|r| r.verse_ref());
        Self { word, reference, at, origin: None, origin_read: false, view: View::Definition, entry: None }
    }

    /// The verse's original words, read the first time anyone looks at this word. Nothing is
    /// unpacked for a lookup that never opens, and 18 MB is not touched until one does.
    fn read_origin(&mut self) {
        if self.origin_read {
            return;
        }
        self.origin_read = true;
        self.origin = self.at.and_then(|at| original::verse(at.book, at.chapter, at.verse));
    }

    /// Opens the verse's original words straight away: a harness step, so a screenshot can show
    /// the list without scripting a click on the row.
    pub fn show_words(&mut self) {
        self.read_origin();
        if self.origin.is_some() {
            self.view = View::Words;
        }
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
                lines: vec![("No definition found".to_string(), Line::Text)],
                pinned: None,
                search_for: Some(self.word.clone()),
            };
        }
        let mut lines = Vec::new();
        // The KJV's spelling is not always the dictionary's headword, and the reader deserves to
        // know why they are being shown a different word.
        if let Some(headword) = &found.shown_as {
            lines.push((format!("“{}” — shown as {}", self.word, headword), Line::Source));
        }
        let one_source = found.entries.iter().all(|e| e.source == found.entries[0].source);
        let mut pinned = None;
        for entry in &found.entries {
            if one_source {
                pinned = Some(entry.source.label().to_string());
            }
            for paragraph in entry.body.split('\n').filter(|p| !p.trim().is_empty()) {
                lines.push((paragraph.trim().to_string(), Line::Text));
            }
            if !one_source {
                lines.push((entry.source.label().to_string(), Line::Source));
            }
        }
        Body { lines, pinned, search_for: None }
    }

    /// The entry face: the word itself, what it is, then the definition line by line.
    fn entry_body(&self) -> Body {
        let Some(entry) = &self.entry else {
            return Body { lines: Vec::new(), pinned: None, search_for: None };
        };
        let mut lines = Vec::new();
        lines.push((entry.lemma.clone(), Line::Head));
        let mut meta = entry.id.clone();
        if !entry.translit.is_empty() {
            meta.push_str(" · ");
            meta.push_str(&entry.translit);
        }
        if !entry.pos.is_empty() {
            meta.push_str(" · ");
            meta.push_str(&entry.pos);
        }
        lines.push((meta, Line::Source));
        if !entry.short.is_empty() {
            lines.push((entry.short.clone(), Line::Text));
        }
        for line in entry.lines() {
            lines.push((line.to_string(), Line::Text));
        }
        Body { lines, pinned: Some(original::credit().to_string()), search_for: None }
    }

    /// Draws the popover hanging from the looked-up words; returns `true` to close.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        p: &Palette,
        reader: &Reader,
        _actions: &mut Vec<Action>,
    ) -> bool {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
            return true;
        }
        let screen = ctx.content_rect();
        let anchor = anchor(reader).unwrap_or_else(|| crate::note_popover::fallback_anchor(screen));
        // Which face of the popover this frame shows. The verse's own words are read once, and
        // only when the definition face is the one on screen: nothing is unpacked for a lookup that
        // never gets that far.
        if self.view == View::Definition {
            self.read_origin();
        }
        let showing = self.view;
        let body = match showing {
            View::Entry => self.entry_body(),
            // A list of the verse's own words is not the dictionary's work: it carries the
            // original-language credit instead.
            View::Words => Body { pinned: Some(original::credit().to_string()), ..self.body() },
            View::Definition => self.body(),
        };
        // The row that offers the verse's original words, or leads back out of them.
        let offer = if self.origin.is_some() { OFFER_ROW + 1.0 } else { 0.0 };
        // A dictionary line at the foot takes a little padding, whatever the entry's length.
        let foot = if body.pinned.is_some() { 24.0 } else { 0.0 };
        // A search button, when there is nothing to show.
        let search_height = if body.search_for.is_some() { 42.0 } else { 0.0 };
        // Wrapped text, so a long entry is measured rather than counted in fixed-height lines.
        let wrap = WIDTH - 32.0;
        let measured: Vec<(std::sync::Arc<egui::Galley>, Line)> = body
            .lines
            .iter()
            .map(|(line, style)| {
                let font = match style {
                    Line::Source => theme::ui_font(11.5),
                    Line::Head => egui::FontId::new(19.0, theme::family(theme::SERIF)),
                    Line::Text => theme::ui_font(13.0),
                };
                (ctx.fonts_mut(|f| f.layout(line.clone(), font, p.ink, wrap)), *style)
            })
            .collect();
        let mut text_height = 0.0;
        for (i, (galley, style)) in measured.iter().enumerate() {
            if *style != Line::Text {
                text_height += 4.0;
            }
            text_height += galley.size().y + 4.0;
            let next_starts_an_entry = measured.get(i + 1).is_some_and(|(_, next)| *next == Line::Text);
            if *style == Line::Source && next_starts_an_entry {
                text_height += 10.0;
            }
        }
        // A long entry scrolls rather than growing the popover off the screen. A list of words is
        // allowed more of the screen: it is a list, and half a verse's words is worse than none.
        let share = if showing == View::Words { 0.62 } else { 0.42 };
        let cap = (screen.height() * share).max(80.0);
        let list_height = self.origin.as_ref().map_or(0.0, |v| v.words.len() as f32 * WORD_ROW);
        let natural = if showing == View::Words { list_height } else { text_height };
        let body_height = natural.min(cap);
        let chrome = 64.5 + 14.0 + 14.0 + foot + search_height + offer;
        let height = chrome + body_height;

        let below = screen.bottom() - (anchor.bottom() + 3.0 + BEAK.y) - 16.0;
        let above = anchor.top() - 3.0 - BEAK.y - (screen.top() + 52.0) - 8.0;
        let downward = below >= height || below >= above;
        let left =
            (anchor.center().x - WIDTH / 2.0).clamp(screen.left() + 12.0, screen.right() - 12.0 - WIDTH);
        let top =
            if downward { anchor.bottom() + 3.0 + BEAK.y } else { anchor.top() - 3.0 - BEAK.y - height };
        let rect = Rect::from_min_size(pos2(left, top), vec2(WIDTH, height));

        // Taken out of `self` for the duration of the frame, so the closure can write to `self`
        // when a row is clicked without fighting the borrow it would otherwise hold.
        let origin = self.origin.take();
        let mut opened: Option<usize> = None;
        let mut go: Option<View> = None;
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
                text(
                    &painter,
                    x,
                    rect.top() + 14.0,
                    20.0,
                    &self.word,
                    theme::ui_semibold(20.0),
                    p.ink,
                    false,
                );
                text(
                    &painter,
                    x,
                    rect.top() + 40.0,
                    12.0,
                    &self.reference,
                    theme::ui_font(12.0),
                    p.ink_2,
                    false,
                );
                let close_r = Rect::from_min_size(
                    pos2(rect.right() - 12.0 - 28.0, rect.top() + 16.0),
                    vec2(28.0, 28.0),
                );
                if widgets::round_button(ui, p, close_r, icons::close, ui.id().with("lookup-close")).clicked()
                {
                    close = true;
                }
                painter.hline(rect.x_range(), rect.top() + 64.5, Stroke::new(1.0, p.separator));

                let body_rect = Rect::from_min_max(
                    pos2(rect.left() + 16.0, rect.top() + 64.5 + 12.0),
                    pos2(rect.right() - 16.0, rect.bottom() - 14.0 - foot - search_height - offer),
                );
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body_rect));
                child.set_clip_rect(body_rect.intersect(ctx.content_rect()));
                child.spacing_mut().item_spacing.y = 4.0;
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(&mut child, |ui| {
                    if showing == View::Words
                        && let Some(verse) = &origin
                    {
                        // The rows are placed by their own height: the scroll area's gap between
                        // items would push the last word of a psalm past the clip.
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for (i, word) in verse.words.iter().enumerate() {
                            let (row, response) =
                                ui.allocate_exact_size(vec2(ui.available_width(), WORD_ROW), Sense::click());
                            if response.hovered() {
                                ui.painter().rect_filled(row, 4.0, p.fill);
                            }
                            let base = row.top() + 7.0;
                            let mut x = row.left();
                            x += text(
                                ui.painter(),
                                x,
                                base,
                                16.0,
                                &word.surface,
                                egui::FontId::new(16.0, theme::family(theme::SERIF)),
                                p.ink,
                                false,
                            ) + 7.0;
                            x += text(
                                ui.painter(),
                                x,
                                base + 1.0,
                                11.0,
                                &word.translit,
                                theme::ui_font(10.5),
                                p.ink_3,
                                false,
                            ) + 7.0;
                            text(
                                ui.painter(),
                                x,
                                base + 1.0,
                                11.0,
                                &word.gloss,
                                egui::FontId::new(12.0, theme::family(theme::SERIF)),
                                p.ink_2,
                                false,
                            );
                            if response.clicked() {
                                opened = Some(i);
                            }
                        }
                    } else {
                        for (galley, style) in &measured {
                            if *style == Line::Source {
                                let text = egui::WidgetText::Galley(galley.clone());
                                ui.add(egui::Label::new(text.color(p.ink_2)).wrap());
                                ui.add_space(10.0);
                            } else {
                                let text = egui::WidgetText::Galley(galley.clone());
                                ui.add(egui::Label::new(text).wrap());
                            }
                        }
                    }
                });
                // Neither dictionary has the word: offer to search the web. The app itself makes no
                // network request — it hands the word to the reader's own browser.
                if let Some(word) = &body.search_for {
                    let button = Rect::from_min_size(
                        pos2(rect.left() + 16.0, rect.bottom() - 14.0 - foot - 34.0 - offer),
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
                        rect.bottom() - 15.0 - offer,
                        11.0,
                        &format!("Source: {label}"),
                        egui::FontId::new(11.5, theme::family(theme::SERIF_ITALIC)),
                        p.ink_2,
                        false,
                    );
                }
                if offer > 0.0 {
                    let top = rect.bottom() - offer;
                    painter.hline(rect.x_range(), top, Stroke::new(1.0, p.separator));
                    let row = Rect::from_min_max(
                        pos2(rect.left() + 1.0, top + 1.0),
                        pos2(rect.right() - 1.0, rect.bottom() - 1.0),
                    );
                    let response = ui.allocate_rect(row, Sense::click());
                    if std::env::var("BIBLE_RS_TRACE_POPOVER").is_ok() {
                        eprintln!(
                            "row: showing={:?} hovered={} clicked={} row_rect={:?} ptr={:?}",
                            showing,
                            response.hovered(),
                            response.clicked(),
                            row,
                            ui.ctx().input(|i| i.pointer.interact_pos())
                        );
                    }
                    if response.hovered() {
                        painter.rect_filled(row, 0.0, p.fill);
                    }
                    let (label, tail, next) = match showing {
                        View::Definition => {
                            let language = origin.as_ref().map_or("Original", |v| v.language.name());
                            let count = origin.as_ref().map_or(0, |v| v.words.len());
                            let words = if count == 1 { "word" } else { "words" };
                            (language.to_string(), format!("{count} {words}"), Some(View::Words))
                        }
                        View::Words => {
                            ("‹  English definition".to_string(), String::new(), Some(View::Definition))
                        }
                        View::Entry => {
                            let language = origin.as_ref().map_or("Original", |v| v.language.name());
                            (format!("‹  {language} words"), String::new(), Some(View::Words))
                        }
                    };
                    let mid = (row.top() + row.bottom()) / 2.0;
                    let ink = if response.hovered() { p.ink } else { p.ink_2 };
                    let mut x = row.left() + 16.0;
                    x += text(&painter, x, mid - 7.0, 14.0, &label, theme::ui_font(12.0), ink, false) + 8.0;
                    if !tail.is_empty() {
                        text(&painter, x, mid - 7.0, 14.0, &tail, theme::ui_font(12.0), p.ink_3, false);
                    }
                    text(
                        &painter,
                        row.right() - 16.0,
                        mid - 9.0,
                        16.0,
                        if showing == View::Definition { "›" } else { "" },
                        theme::ui_font(14.0),
                        if response.hovered() { p.accent } else { p.ink_3 },
                        true,
                    );
                    if response.clicked()
                        && let Some(next) = next
                    {
                        go = Some(next);
                    }
                }
            });
        self.origin = origin;
        if let Some(next) = go {
            self.view = next;
        }
        if let Some(index) = opened
            && let Some(word) = self.origin.as_ref().and_then(|v| v.words.get(index))
        {
            self.entry = original::entry(&word.strong);
            self.view = View::Entry;
        }
        // A click anywhere outside closes it, as the note popover already behaves.
        close || area.response.clicked_elsewhere()
    }
}

/// Starts unpacking the bundled dictionaries on a background thread, so the first lookup finds the
/// index already built instead of stalling on 13 MB of decompression. Called when the verse menu
/// opens with a word to look up: a reader who never looks anything up never pays for it, and one
/// thread per app is enough, so later menu opens are free.
pub fn warm_dictionary() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        std::thread::spawn(|| bible_core::lexicon::warm());
    });
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
