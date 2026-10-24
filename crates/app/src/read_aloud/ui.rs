//! The Listen control and the reading bar.
//!
//! The reader is the product, so nothing here is permanent furniture. The toolbar speaker turns a
//! quiet navy while a chapter is being read; pressing it raises a floating bar for a few seconds so
//! the reader sees the result where they clicked; after that the bar settles into a hairline along
//! the bottom edge with two pieces of small grey text. Touching the line brings the controls back in
//! the same place, so nothing jumps.

use eframe::egui::{self, Align2, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use bible_core::speak::{Session, Sleep, Voice};

use super::reading::{Reading, ReadingState};
use crate::theme::Palette;

/// How long the floating bar stays up after the reader touches something.
const BAR_SECONDS: f64 = 3.0;
/// The status line's height, and the expanded bar's.
const LINE_HEIGHT: f32 = 40.0;
const BAR_HEIGHT: f32 = 52.0;
const INSET: f32 = 18.0;
/// The floating bar's width when the controls are up.
const BAR_WIDTH: f32 = 340.0;

/// What is being read, and how the reader is shown it.
pub struct Listen {
    reading: Option<Reading>,
    /// "Psalms 23" — what the bar calls the chapter.
    label: String,
    /// When the floating bar gives way to the status line, on the app's clock.
    bar_until: f64,
    timer_open: bool,
    /// Where the stopwatch was drawn last frame, so a press on it does not close its own menu.
    timer_anchor: Rect,
    /// Where the menu was drawn, for telling a press inside it from one outside.
    menu_rect: Rect,
    /// A state painted without any sound, so the bar can be photographed in the harness.
    preview: Option<ReadingState>,
    /// A countdown painted without a reading behind it, so the harness can photograph the bar.
    preview_left: Option<f32>,
}

impl Listen {
    pub fn new() -> Listen {
        Listen {
            reading: None,
            label: String::new(),
            bar_until: 0.0,
            timer_open: false,
            timer_anchor: Rect::NOTHING,
            menu_rect: Rect::NOTHING,
            preview: None,
            preview_left: None,
        }
    }

    /// Whether a chapter is being read, or about to be.
    pub fn active(&self) -> bool {
        self.reading.is_some() || self.preview.is_some()
    }

    /// The verse being spoken, for the reader's margin mark.
    pub fn verse(&self) -> Option<u8> {
        self.state().verse
    }

    pub fn state(&self) -> ReadingState {
        match (&self.preview, &self.reading) {
            (Some(preview), _) => preview.clone(),
            (None, Some(reading)) => reading.state(),
            (None, None) => ReadingState::default(),
        }
    }

    /// Opens the controls for a chapter without speaking: the reader presses play when ready.
    pub fn begin(&mut self, session: Session, voice: Voice, label: impl Into<String>, now: f64) {
        self.stop();
        self.preview = None;
        self.label = label.into();
        self.reading = Some(Reading::open(session, voice));
        self.preview_left = None;
        self.touched(now);
    }

    /// Paints the bar without speaking, for the screenshot harness.
    pub fn show_preview(&mut self, verse: u8, left: f32, label: impl Into<String>, now: f64) {
        self.label = label.into();
        self.preview = Some(ReadingState { verse: Some(verse), speaking: true, ..Default::default() });
        self.preview_left = Some(left);
        self.touched(now);
    }

    pub fn stop(&mut self) {
        if let Some(reading) = self.reading.take() {
            reading.stop();
        }
        self.preview = None;
        self.preview_left = None;
        self.timer_open = false;
    }

    /// The reader did something: show the controls again, then let them fade.
    pub fn touched(&mut self, now: f64) {
        self.bar_until = now + BAR_SECONDS;
    }

    pub fn pause(&mut self, now: f64) {
        if let Some(reading) = &self.reading {
            reading.pause();
        }
        self.touched(now);
    }

    pub fn resume(&mut self, now: f64) {
        if let Some(reading) = &self.reading {
            // Resuming puts the countdown back to the time it was holding, so a pause costs
            // nothing but the pause.
            reading.resume();
        }
        self.touched(now);
    }

    pub fn set_sleep(&mut self, sleep: Sleep, now: f64) {
        if let Some(reading) = &self.reading {
            reading.choose(sleep);
        }
        self.touched(now);
    }

    /// What is left on the timer, if one is armed: straight off the clock.
    fn left(&self) -> Option<f32> {
        self.preview_left
            .or_else(|| self.reading.as_ref().and_then(|reading| reading.left()))
    }

    pub fn sleep(&self) -> Sleep {
        self.reading.as_ref().map(|r| r.sleep()).unwrap_or_default()
    }

    /// Draws whatever the reader should see, and handles the controls.
    ///
    /// Returns false once the reading is over and there is nothing left to show.
    pub fn show(&mut self, ui: &mut Ui, p: &Palette, area: Rect, now: f64) -> bool {
        if !self.active() {
            return false;
        }
        let state = self.state();
        // A failure is worth saying even though the reading has already stopped: silence would read
        // as a dead button.
        if let Some(note) = &state.note {
            let text = format!("Read aloud: {note}");
            let rect = Rect::from_min_size(
                Pos2::new(area.center().x - 220.0, area.bottom() - LINE_HEIGHT - INSET),
                Vec2::new(440.0, LINE_HEIGHT),
            );
            ui.painter().rect_filled(rect, 8.0, p.paper);
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                text,
                FontId::proportional(12.0),
                p.ink_2,
            );
            if state.finished {
                self.reading = None;
            }
            return false;
        }
        if state.finished && self.preview.is_none() {
            self.reading = None;
            return false;
        }
        // The bar falls back to the hairline only while it is actually speaking: that is the point
        // at which the reader's eyes want the page back. A bar waiting for a press — Ready, or
        // Paused — stays where it is, because nothing explains where it went if it slides away.
        let expanded = now < self.bar_until || !state.speaking;
        // The bar lives in the foreground layer: in the reader's layer a click on a button also
        // reaches the verse underneath and selects text behind the controls.
        egui::Area::new(egui::Id::new("read-aloud-bar"))
            .order(egui::Order::Foreground)
            .fixed_pos(area.min)
            .show(ui.ctx(), |bar| {
                if expanded {
                    self.pill(bar, p, area, now, &state);
                } else {
                    self.line(bar, p, area, now, &state);
                }
            });
        // Only what moves asks for the next frame: a countdown running down, and the fall to the
        // hairline while that is still to come. A bar waiting on the reader wakes for nothing.
        if state.speaking || self.left().is_some() || now < self.bar_until {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(120));
        }
        true
    }

    /// The floating bar: what the reader gets when they press something.
    fn pill(&mut self, ui: &mut Ui, p: &Palette, area: Rect, now: f64, state: &ReadingState) {
        let rect = Rect::from_center_size(
            Pos2::new(area.center().x, area.bottom() - INSET - BAR_HEIGHT / 2.0),
            Vec2::new(BAR_WIDTH, BAR_HEIGHT),
        );
        // While the pointer is on the bar it stays up: letting it fall to the hairline under the
        // cursor and spring back a frame later is the flicker the reader sees.
        if self.timer_open
            || ui.ctx().pointer_hover_pos().is_some_and(|pt| rect.expand(8.0).contains(pt))
        {
            self.bar_until = now + BAR_SECONDS;
        }
        // The bar swallows clicks anywhere on itself: a press on its body should not select the
        // verse behind it. Its buttons are registered after this, so they win over it.
        let _ = ui.interact(rect, ui.id().with("read-aloud-pill"), Sense::click_and_drag());
        p.shadows.panel.paint(ui.painter(), rect, BAR_HEIGHT / 2.0);
        ui.painter().rect_filled(rect, BAR_HEIGHT / 2.0, p.paper);
        ui.painter().rect_stroke(
            rect,
            BAR_HEIGHT / 2.0,
            Stroke::new(0.5, p.border),
            egui::StrokeKind::Inside,
        );
        self.controls(ui, p, rect, now, state, true);
        if self.timer_open {
            self.timer_menu(ui, p, rect, now);
        }
    }

    /// The hairline the bar fades into: two pieces of small grey text and nothing else.
    fn line(&mut self, ui: &mut Ui, p: &Palette, area: Rect, now: f64, state: &ReadingState) {
        let rect = Rect::from_min_size(
            Pos2::new(area.left(), area.bottom() - LINE_HEIGHT),
            Vec2::new(area.width(), LINE_HEIGHT),
        );
        let resp = ui.interact(rect, ui.id().with("read-aloud-line"), Sense::click());
        if resp.hovered() {
            // A pointer, so the line is known to be pressable — but nothing moves and nothing
            // opens while the pointer merely crosses it.
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() || self.timer_open {
            // A press brings the controls back in the same place, so nothing moves.
            self.bar_until = now + BAR_SECONDS;
            ui.painter().rect_filled(rect, 0.0, p.fill);
            ui.painter().hline(
                rect.x_range(),
                rect.top(),
                Stroke::new(0.5, p.separator),
            );
            self.controls(ui, p, rect, now, state, false);
            if self.timer_open {
                self.timer_menu(ui, p, rect, now);
            }
            return;
        }
        // Paper under the hairline: text scrolling beneath must not run through it.
        ui.painter().rect_filled(rect, 0.0, p.paper);
        ui.painter().hline(rect.x_range(), rect.top(), Stroke::new(0.5, p.separator));
        let cy = rect.center().y;
        let speaker = Rect::from_center_size(Pos2::new(rect.left() + INSET + 7.0, cy), Vec2::splat(15.0));
        crate::icons::speaker(ui.painter(), speaker, p.accent_quiet);
        ui.painter().text(
            Pos2::new(rect.left() + INSET + 22.0, cy),
            Align2::LEFT_CENTER,
            format!("Listening · {}", self.where_is(state)),
            FontId::proportional(11.5),
            p.ink_2,
        );
        ui.painter().text(
            Pos2::new(rect.right() - INSET, cy),
            Align2::RIGHT_CENTER,
            self.right_text(state),
            FontId::proportional(11.5),
            p.ink_3,
        );
    }

    /// Pause, where it is, the sleep timer and a close — laid out either in the pill or in the line.
    fn controls(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        rect: Rect,
        now: f64,
        state: &ReadingState,
        floating: bool,
    ) {
        let cy = rect.center().y;
        let speaking = state.speaking;
        // Pause / resume, on the left in the pill and on the right in the line.
        let play_center = if floating {
            Pos2::new(rect.left() + 32.0, cy)
        } else {
            Pos2::new(rect.right() - INSET - 130.0, cy)
        };
        let play_rect = Rect::from_center_size(play_center, Vec2::splat(if floating { 40.0 } else { 34.0 }));
        let play = ui.interact(play_rect, ui.id().with("read-aloud-play"), Sense::click());
        if floating {
            p.shadows.control.paint(ui.painter(), play_rect, 20.0);
            ui.painter().circle_filled(play_rect.center(), 20.0, p.accent_quiet);
        } else if play.hovered() {
            ui.painter().circle_filled(play_rect.center(), 17.0, p.fill_strong);
        }
        let glyph = if speaking { p.on_accent } else { p.accent_quiet };
        let glyph = if floating { p.on_accent } else { glyph };
        if speaking {
            let w = if floating { 3.0 } else { 2.5 };
            let h = if floating { 11.0 } else { 9.0 };
            for dx in [-2.5, 2.5] {
                ui.painter().rect_filled(
                    Rect::from_center_size(
                        Pos2::new(play_rect.center().x + if floating { dx } else { dx * 0.8 }, play_rect.center().y),
                        Vec2::new(w, h),
                    ),
                    1.0,
                    glyph,
                );
            }
        } else {
            crate::icons::play(ui.painter(), Rect::from_center_size(play_rect.center(), Vec2::splat(14.0)), glyph);
        }
        if play.clicked() {
            if speaking {
                self.pause(now);
            } else {
                self.resume(now);
            }
        }

        let text = self.where_is(state);
        if floating {
            ui.painter().text(
                Pos2::new(rect.left() + 58.0, cy - 7.0),
                Align2::LEFT_CENTER,
                text,
                FontId::proportional(13.0),
                p.ink,
            );
            ui.painter().text(
                Pos2::new(rect.left() + 58.0, cy + 8.0),
                Align2::LEFT_CENTER,
                Self::listening_text(state),
                FontId::proportional(10.5),
                p.ink_2,
            );
        } else {
            ui.painter().text(
                Pos2::new(rect.left() + INSET + 8.0, cy),
                Align2::LEFT_CENTER,
                format!("Listening · {text}"),
                FontId::proportional(12.0),
                p.ink,
            );
        }

        // From the right: the close, a gap, then the time and the stopwatch that sets it. Laying
        // them out from the right keeps the countdown clear of the X however wide it gets.
        let close_rect =
            Rect::from_center_size(Pos2::new(rect.right() - INSET - 14.0, cy), Vec2::splat(28.0));
        let close = ui.interact(close_rect, ui.id().with("read-aloud-close"), Sense::click());
        if close.hovered() {
            ui.painter().circle_filled(close_rect.center(), 14.0, p.fill_strong);
        }
        crate::icons::cross(ui.painter(), close_rect, p.ink_2);
        if close.clicked() {
            self.stop();
        }

        // The stopwatch and its countdown are one control: a 14 pt glyph alone is a small target
        // for someone reading in bed, and the time reads as the button's own label.
        // The countdown gets a fixed slot, and digits that all share one width: measured from the
        // number itself, the stopwatch shuffled sideways as the time changed, which reads as the
        // whole bar twitching once a second.
        let clock = self.clock_text(state);
        let clock_font = FontId::monospace(12.0);
        let widest = ui.painter().layout_no_wrap("88:88".to_string(), clock_font.clone(), p.ink_2).size().x;
        let clock_width = if clock.is_empty() { 0.0 } else { widest };
        let group_right = close_rect.left() - 20.0;
        // Room for the glyph, a gap, then the time — the glyph must be inside the box that reacts,
        // or pressing the stopwatch itself does nothing.
        let icon_space = 26.0;
        let gap = if clock_width > 0.0 { 6.0 } else { 0.0 };
        let group_left = group_right - clock_width - gap - icon_space;
        let group = Rect::from_min_max(
            Pos2::new(group_left, cy - 15.0),
            Pos2::new(group_right, cy + 15.0),
        );
        self.timer_anchor = group;
        let timer = ui.interact(group, ui.id().with("read-aloud-timer"), Sense::click());
        if timer.hovered() || self.timer_open {
            ui.painter().rect_filled(group, 9.0, p.fill_strong);
        }
        let icon = Rect::from_center_size(Pos2::new(group_left + 13.0, cy), Vec2::splat(30.0));
        // Grey until a time has been set, then the quiet navy: the same rule as the toolbar
        // speaker — tinted means there is something in effect.
        let armed = !clock.is_empty();
        crate::icons::stopwatch(ui.painter(), icon, if armed { p.accent_quiet } else { p.ink_2 });
        if !clock.is_empty() {
            ui.painter().text(Pos2::new(group_right, cy), Align2::RIGHT_CENTER, clock, clock_font, p.ink_2);
        }
        if timer.clicked() {
            self.timer_open = !self.timer_open;
            self.touched(now);
        }
    }
}

impl Listen {
    /// "Listening", "Paused" or "Ready": what is actually happening, because a bar that says
    /// Listening while silent is a lie. The voice is not named here — it is set in Settings, and
    /// repeating it beside the reader's own reading is one more thing to read and nothing more.
    fn listening_text(state: &ReadingState) -> String {
        match (state.speaking, state.verse) {
            (true, _) => "Listening".to_string(),
            (false, Some(_)) => "Paused".to_string(),
            (false, None) => "Ready".to_string(),
        }
    }

    /// "Psalms 23:2" — where the reading has got to.
    fn where_is(&self, state: &ReadingState) -> String {
        match state.verse {
            Some(verse) => format!("{}:{}", self.label, verse),
            None => self.label.clone(),
        }
    }

    /// Beside the stopwatch: what is left of the sleep timer, as a clock.
    fn clock_text(&self, _state: &ReadingState) -> String {
        self.left().map(clock).unwrap_or_default()
    }

    /// The right-hand side of the hairline: the timer choice and what is left of it, or nothing at
    /// all when no timer is armed — a reading with no timer has nothing to count down.
    fn right_text(&self, _state: &ReadingState) -> String {
        match self.left() {
            Some(left) => format!("{} · {} left", sleep_label(self.sleep()), clock(left)),
            None => String::new(),
        }
    }

    /// The five ways the reading can end itself.
    fn timer_menu(&mut self, ui: &mut Ui, p: &Palette, anchor: Rect, now: f64) {
        // A menu closes when the reader presses anywhere else, or presses Escape. The stopwatch
        // itself is excluded: it toggles the menu, and closing it here would fight that.
        let outside = ui
            .ctx()
            .pointer_latest_pos()
            .is_some_and(|pt| !self.menu_rect.contains(pt) && !self.timer_anchor.contains(pt));
        let pressed = ui.ctx().input(|i| i.pointer.any_pressed());
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) || (pressed && outside) {
            self.timer_open = false;
            return;
        }
        let choices = [
            Sleep::Minutes(15),
            Sleep::Minutes(30),
            Sleep::Minutes(45),
            Sleep::Minutes(60),
            Sleep::Off,
        ];
        let width = 236.0;
        let row = 32.0;
        let height = row * choices.len() as f32 + 34.0;
        let rect = Rect::from_min_size(
            Pos2::new(anchor.right() - INSET - width, anchor.top() - height - 8.0),
            Vec2::new(width, height),
        );
        self.menu_rect = rect;
        p.shadows.popover.paint(ui.painter(), rect, 12.0);
        ui.painter().rect_filled(rect, 12.0, p.paper);
        ui.painter().rect_stroke(rect, 12.0, Stroke::new(0.5, p.border), egui::StrokeKind::Inside);
        ui.painter().text(
            Pos2::new(rect.left() + 14.0, rect.top() + 17.0),
            Align2::LEFT_CENTER,
            "STOP READING AFTER",
            FontId::proportional(10.5),
            p.ink_3,
        );
        let chosen = self.sleep();
        for (i, sleep) in choices.iter().enumerate() {
            let row_rect = Rect::from_min_size(
                Pos2::new(rect.left() + 6.0, rect.top() + 30.0 + i as f32 * row),
                Vec2::new(width - 12.0, row),
            );
            let resp = ui.interact(row_rect, ui.id().with(("read-aloud-sleep", i)), Sense::click());
            let on = *sleep == chosen;
            if on {
                ui.painter().rect_filled(row_rect, 8.0, p.accent_soft);
            } else if resp.hovered() {
                ui.painter().rect_filled(row_rect, 8.0, p.fill);
            }
            ui.painter().text(
                Pos2::new(row_rect.left() + 10.0, row_rect.center().y),
                Align2::LEFT_CENTER,
                sleep_label(*sleep),
                FontId::proportional(13.0),
                if on { p.accent } else { p.ink },
            );
            if on {
                crate::icons::tick(ui.painter(), row_rect, p.accent);
            }
            if resp.clicked() {
                self.set_sleep(*sleep, now);
                self.timer_open = false;
            }
        }
    }
}

/// What the bar calls a sleep choice.
/// Seconds as a clock: `14:52`, or `1:04:10` past an hour.
fn clock(seconds: f32) -> String {
    let total = seconds.max(0.0).round() as u32;
    let (hours, minutes, secs) = (total / 3600, (total % 3600) / 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes}:{secs:02}")
    }
}

fn sleep_label(sleep: Sleep) -> String {
    match sleep {
        Sleep::Minutes(minutes) => format!("{minutes} min"),
        Sleep::Off => "End of chapter".to_string(),
    }
}


#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn the_sleep_choices_are_worded_for_the_bar() {
        assert_eq!(sleep_label(Sleep::Minutes(30)), "30 min");
        assert_eq!(sleep_label(Sleep::Off), "End of chapter");
    }

    #[test]
    fn where_the_reading_is_names_the_chapter() {
        let mut listen = Listen::new();
        listen.label = "Psalms 23".to_string();
        let state = ReadingState { verse: Some(2), ..Default::default() };
        assert_eq!(listen.where_is(&state), "Psalms 23:2");
        let before = ReadingState::default();
        assert_eq!(listen.where_is(&before), "Psalms 23");
    }


}

