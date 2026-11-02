//! The related-verses card: the verses the reading tradition links to the one the reader chose.
//!
//! It behaves like the word popover beside it — the same width, the same beak toward the text, the
//! same rule that it lives exactly as long as the verse it was opened on is on the page — because it
//! is the same kind of thing: a card about one spot in the text, closed the moment that spot goes.

use bible_core::model::Translation;
use bible_core::{VerseRef, related};
use eframe::egui::{self, Rect, pos2, vec2};

use crate::actions::Action;
use crate::icons;
use crate::reader::{Reader, TextPos};
use crate::theme::{self, Palette};

/// The popover's width, the same as the word popover's: they never appear together, and a reader
/// should not have to learn two sizes.
const WIDTH: f32 = 320.0;
const BEAK: egui::Vec2 = vec2(14.0, 7.0);
/// Title, reference, and the rule under them.
const HEAD: f32 = 64.5;

/// One verse offered: its reference, the first words of it, and where to go.
struct Row {
    label: String,
    snippet: String,
    to: VerseRef,
}

pub struct Related {
    /// The verse the card belongs to.
    verse: VerseRef,
    /// The verse the card was opened on, for the line under its title.
    reference: String,
    /// Where that verse sits on the page, so the card can follow the text and leave with it.
    at_word: Option<TextPos>,
    rows: Vec<Row>,
    /// Read once, on the first frame: a card that stays open is drawn sixty times a second.
    read: bool,
}

impl Related {
    pub fn new(verse: VerseRef, reference: String, at_word: Option<TextPos>) -> Self {
        Self { verse, reference, at_word, rows: Vec::new(), read: false }
    }

    /// The verse's related verses, read the first time the card is drawn. The links are already
    /// best-first; the words come from whichever translation the reader is in.
    fn read(&mut self, verse: VerseRef, translation: Option<&Translation>) {
        if self.read {
            return;
        }
        self.read = true;
        self.rows = related::of(verse.book, verse.chapter, verse.verse)
            .iter()
            .map(|link| Row {
                label: link.label(),
                snippet: translation
                    .and_then(|t| {
                        t.find(VerseRef { book: link.book, chapter: link.chapter, verse: link.verse })
                    })
                    .map(|v| first_words(&v.plain_text()))
                    .unwrap_or_default(),
                to: VerseRef { book: link.book, chapter: link.chapter, verse: link.verse },
            })
            .collect();
    }

    /// Returns `true` when the card should close.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        p: &Palette,
        reader: &Reader,
        translation: Option<&Translation>,
        actions: &mut Vec<Action>,
    ) -> bool {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            return true;
        }
        let screen = ctx.content_rect();
        // It belongs to its verse: it lives exactly as long as that verse is on the page.
        if let Some(at) = self.at_word {
            let page = Rect::from_min_max(pos2(screen.left(), screen.top() + 52.0), screen.max);
            if !reader.point_at(at).is_some_and(|p| page.contains(p)) {
                return true;
            }
        }
        self.read(self.verse, translation);
        if self.rows.is_empty() {
            return true;
        }
        // Hangs from the verse's own line, beside the text where the word popover goes.
        let anchor = self
            .at_word
            .and_then(|at| reader.point_at(at))
            .map(|point| Rect::from_min_size(pos2(point.x - 4.0, point.y - 4.0), vec2(8.0, 8.0)))
            .unwrap_or_else(|| crate::note_popover::fallback_anchor(screen));

        let wrap = WIDTH - 32.0;
        let measured: Vec<(std::sync::Arc<egui::Galley>, std::sync::Arc<egui::Galley>, f32)> = self
            .rows
            .iter()
            .map(|row| {
                let label =
                    ctx.fonts_mut(|f| f.layout(row.label.clone(), theme::ui_semibold(12.5), p.ink, wrap));
                let snippet = ctx.fonts_mut(|f| {
                    let mut job = egui::text::LayoutJob::simple(
                        row.snippet.clone(),
                        egui::FontId::new(13.0, theme::family(theme::SERIF)),
                        p.ink_2,
                        wrap,
                    );
                    // Two lines, then an ellipsis: enough to judge a link without taking it.
                    job.wrap.max_rows = 2;
                    job.wrap.break_anywhere = false;
                    job.wrap.overflow_character = Some('…');
                    f.layout_job(job)
                });
                let height = 8.0 + label.size().y + 3.0 + snippet.size().y + 8.0;
                (label, snippet, height)
            })
            .collect();
        let rows_height: f32 = measured.iter().map(|(_, _, h)| *h).sum();
        let foot = 30.0;
        let chrome = HEAD + 12.0 + 14.0 + foot;
        // A long list scrolls inside the card rather than growing past the screen edge, where its
        // last link could not be reached.
        let below = screen.bottom() - (anchor.bottom() + 3.0 + BEAK.y) - 16.0;
        let above = anchor.top() - 3.0 - BEAK.y - (screen.top() + 52.0) - 8.0;
        let room = below.max(above) - chrome;
        let cap = (screen.height() * 0.62).min(room).max(80.0);
        let body = rows_height.min(cap);
        let height = chrome + body;
        let downward = below >= height || below >= above;
        let left =
            (anchor.center().x - WIDTH / 2.0).clamp(screen.left() + 12.0, screen.right() - 12.0 - WIDTH);
        let top =
            if downward { anchor.bottom() + 3.0 + BEAK.y } else { anchor.top() - 3.0 - BEAK.y - height };
        let rect = Rect::from_min_size(pos2(left, top), vec2(WIDTH, height));

        let mut goto: Option<VerseRef> = None;
        let mut close = false;
        let area = egui::Area::new(egui::Id::new("related-popover"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .show(ctx, |ui| {
                ui.allocate_rect(Rect::from_min_size(rect.min, rect.size()), egui::Sense::hover());
                let painter = ui.painter().clone();
                p.shadows.popover.paint(&painter, rect, 16.0);
                painter.rect_filled(rect, 16.0, p.surface);
                // The beak, toward the verse.
                let tip_x = (rect.left() + 60.0).max(anchor.left() + 24.0).min(rect.right() - 24.0);
                let (edge, dir) =
                    if downward { (rect.top() + 0.5, -1.0) } else { (rect.bottom() - 0.5, 1.0) };
                let tip = pos2(tip_x, edge + dir * BEAK.y);
                let l = pos2(tip_x - BEAK.x / 2.0, edge);
                let r = pos2(tip_x + BEAK.x / 2.0, edge);
                painter.add(egui::Shape::convex_polygon(vec![l, tip, r], p.surface, egui::Stroke::NONE));
                painter.add(egui::Shape::line(vec![l, tip, r], egui::Stroke::new(0.75, p.border)));

                let x = rect.left() + 16.0;
                label(&painter, x, rect.top() + 13.0, "Related verses", theme::ui_semibold(15.0), p.ink);
                label(&painter, x, rect.top() + 37.0, &self.reference, theme::ui_font(12.0), p.ink_2);
                let close_r = Rect::from_min_size(
                    pos2(rect.right() - 12.0 - 28.0, rect.top() + 16.0),
                    vec2(28.0, 28.0),
                );
                if crate::widgets::round_button(ui, p, close_r, icons::close, ui.id().with("related-close"))
                    .clicked()
                {
                    close = true;
                }
                painter.hline(rect.x_range(), rect.top() + HEAD, egui::Stroke::new(1.0, p.separator));

                let body_rect = Rect::from_min_max(
                    pos2(rect.left() + 16.0, rect.top() + HEAD + 10.0),
                    pos2(rect.right() - 16.0, rect.bottom() - 12.0 - foot),
                );
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body_rect));
                child.set_clip_rect(body_rect.intersect(ctx.content_rect()));
                child.spacing_mut().item_spacing.y = 0.0;
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(&mut child, |ui| {
                    for (i, (label_galley, snippet_galley, height)) in measured.iter().enumerate() {
                        let (row, response) =
                            ui.allocate_exact_size(vec2(ui.available_width(), *height), egui::Sense::click());
                        if response.hovered() {
                            ui.painter().rect_filled(row, 6.0, p.fill);
                        }
                        let text_left = row.left() + 4.0;
                        ui.painter().galley(pos2(text_left, row.top() + 8.0), label_galley.clone(), p.ink);
                        ui.painter().galley(
                            pos2(text_left, row.top() + 8.0 + label_galley.size().y + 3.0),
                            snippet_galley.clone(),
                            p.ink_2,
                        );
                        if response.clicked() {
                            goto = Some(self.rows[i].to);
                        }
                    }
                });
                // The credit rides at the foot, so a long list scrolling does not take the
                // provenance with it.
                label(
                    &painter,
                    x,
                    rect.bottom() - 17.0,
                    related::credit(),
                    egui::FontId::new(10.5, theme::family(theme::SERIF_ITALIC)),
                    p.ink_3,
                );
            });

        if let Some(to) = goto {
            actions.push(Action::Jump { chapter: to.chapter_idx(), verse: Some(to.verse), flash: true });
            return true;
        }
        close || area.response.clicked_elsewhere()
    }
}

/// The opening words of a verse, enough for a snippet: the card clamps it to two lines anyway, and
/// a long verse should not be copied whole just to be cut.
fn first_words(text: &str) -> String {
    const LIMIT: usize = 180;
    let text = text.trim();
    if text.chars().count() <= LIMIT {
        return text.to_string();
    }
    let mut out: String = text.chars().take(LIMIT).collect();
    out.push('…');
    out
}

/// One line of text at a fixed top, in the palette's ink.
fn label(painter: &egui::Painter, x: f32, top: f32, text: &str, font: egui::FontId, ink: egui::Color32) {
    let galley = painter.layout_no_wrap(text.to_string(), font, ink);
    painter.galley(pos2(x, top), galley, ink);
}
