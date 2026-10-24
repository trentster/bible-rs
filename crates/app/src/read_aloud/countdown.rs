//! The sleep timer's countdown: what the reading bar shows of the time left.
//!
//! The reading thread reports what is left once per verse — after the work of making the next verse
//! hearable. Followed naively, the number would sit still and then drop four or five seconds at a
//! time. So between reports it is followed against the audio the speakers have actually taken, not
//! against the clock on the wall: that keeps running while a verse is being made, with no voice
//! playing, which put the number ahead of the truth and made it step backwards when the next report
//! arrived.

use super::ReadingState;

/// The number on the bar, and how it stays honest.
#[derive(Default)]
pub struct Countdown {
    /// What the reading last reported, so a fresh number is told from the one being followed.
    reported: Option<f32>,
    /// What is left, brought down with the audio heard since that report.
    left: Option<f32>,
    /// How much of the voice had been heard when `left` was taken from a report.
    heard_at_report: f32,
}

impl Countdown {
    pub fn new() -> Countdown {
        Countdown::default()
    }

    /// Starts again from scratch. A new reading, or a timer just armed, is allowed to put the number
    /// up; a report arriving mid-reading is not.
    pub fn rebase(&mut self) {
        *self = Countdown::new();
    }

    /// Takes a report from the reading, and follows it down with the audio heard since.
    pub fn follow(&mut self, state: &ReadingState, heard: f32) {
        if state.remaining != self.reported {
            let report = state.remaining;
            self.left = match (self.left, report) {
                // A clock that steps up is a clock nobody trusts: a report may only take the number
                // down, and rebasing is the one way back up.
                (Some(shown), Some(fresh)) => Some(shown.min(fresh)),
                (_, fresh) => fresh,
            };
            self.reported = report;
            self.heard_at_report = heard;
        }
        if let Some(base) = self.left {
            // Only the audio heard since the last look: counting the total since the report would
            // take the same seconds off twice.
            let spoken = (heard - self.heard_at_report).max(0.0);
            self.left = Some((base - spoken).max(0.0));
            self.heard_at_report = self.heard_at_report.max(heard);
        }
    }

    /// What is left to show: the followed number, or the raw report when none is being followed.
    pub fn left(&self, state: &ReadingState) -> Option<f32> {
        self.left.or(state.remaining)
    }

    /// What is left, as a clock: `14:52`, or nothing at all when no timer is armed.
    pub fn text(&self, state: &ReadingState) -> String {
        self.left(state).map(clock).unwrap_or_default()
    }
}

/// Seconds as a clock: `14:52`, or `1:04:10` past an hour.
pub fn clock(seconds: f32) -> String {
    let total = seconds.max(0.0).round() as u32;
    let (hours, minutes, secs) = (total / 3600, (total % 3600) / 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes}:{secs:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(remaining: Option<f32>, speaking: bool) -> ReadingState {
        ReadingState { remaining, speaking, ..Default::default() }
    }

    #[test]
    fn a_clock_reads_as_minutes_and_seconds() {
        assert_eq!(clock(892.0), "14:52");
        assert_eq!(clock(9.4), "0:09");
        assert_eq!(clock(3842.0), "1:04:02");
        assert_eq!(clock(-5.0), "0:00");
    }

    #[test]
    fn the_countdown_runs_down_between_verse_reports() {
        let mut countdown = Countdown::new();
        let mut reading = state(Some(60.0), true);
        countdown.follow(&reading, 0.0);
        assert_eq!(countdown.text(&reading), "1:00");
        countdown.follow(&reading, 1.0);
        assert_eq!(countdown.text(&reading), "0:59");
        countdown.follow(&reading, 2.0);
        assert_eq!(countdown.text(&reading), "0:58");
        // A verse is being made, so no audio has played: the number holds rather than running on.
        countdown.follow(&reading, 2.0);
        assert_eq!(countdown.text(&reading), "0:58");
        // The next report is the truth and agrees: it may not lift the number back to 59.
        reading.remaining = Some(58.4);
        countdown.follow(&reading, 2.0);
        assert_eq!(countdown.text(&reading), "0:58");
        // A report can only take it down.
        reading.remaining = Some(51.0);
        countdown.follow(&reading, 9.0);
        assert_eq!(countdown.text(&reading), "0:51");
        // Paused: no audio plays, so the number stops with the voice.
        reading.speaking = false;
        countdown.follow(&reading, 9.0);
        assert_eq!(countdown.text(&reading), "0:51");
    }

    #[test]
    fn arming_a_timer_may_put_the_countdown_up() {
        let mut countdown = Countdown::new();
        let reading = state(Some(60.0), true);
        countdown.follow(&reading, 0.0);
        assert_eq!(countdown.text(&reading), "1:00");
        countdown.rebase();
        let longer = state(Some(1800.0), true);
        countdown.follow(&longer, 0.0);
        assert_eq!(countdown.text(&longer), "30:00");
    }

    #[test]
    fn no_timer_means_no_number() {
        let countdown = Countdown::new();
        assert_eq!(countdown.text(&state(None, true)), "");
        assert_eq!(countdown.left(&state(None, true)), None);
    }
}
