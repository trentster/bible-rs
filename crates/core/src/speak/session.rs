//! A reading session: where we are, whether we are still speaking, and how the reader wants it to end.
//!
//! No audio and no clock live here. The sleep timer is the reader's *choice* — fifteen minutes, or the
//! end of the chapter — and the caller holds the deadline on its own clock, so every rule is testable
//! without waiting for anything.

use super::text::{Utterance, chapter as script_of};

/// When the reading stops by itself.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sleep {
    /// Read the chapter to its end and stop, which is what reading always does.
    #[default]
    Off,
    /// Stop at this time on the clock, fading the verse in progress.
    Minutes(u32),
}

impl Sleep {
    /// The choices the menu offers, in the order it shows them.
    pub const CHOICES: [Sleep; 5] = [
        Sleep::Minutes(15),
        Sleep::Minutes(30),
        Sleep::Minutes(45),
        Sleep::Minutes(60),
        Sleep::Off,
    ];

    pub fn label(self) -> String {
        match self {
            Sleep::Off => "End of chapter".to_string(),
            Sleep::Minutes(m) => format!("{m} minutes"),
        }
    }
}

/// One chapter being read: the script, the place in it, and the timer.
pub struct Session {
    pub book: String,
    pub chapter: u16,
    script: Vec<Utterance>,
    at: usize,
    playing: bool,
    sleep: Sleep,
}

impl Session {
    pub fn new(book: &str, chapter: u16, verses: &[(u32, String)], sleep: Sleep) -> Self {
        Self {
            book: book.to_string(),
            chapter,
            script: script_of(book, chapter as u32, verses),
            at: 0,
            playing: false,
            sleep,
        }
    }

    pub fn utterances(&self) -> &[Utterance] {
        &self.script
    }

    /// How many utterances have been handed out.
    pub fn position(&self) -> usize {
        self.at
    }

    /// The verse being spoken, for the reading bar and for remembering where we stopped.
    pub fn verse(&self) -> Option<u8> {
        self.script.get(self.at.saturating_sub(1)).and_then(|u| u.verse)
    }

    pub fn playing(&self) -> bool {
        self.playing
    }

    pub fn sleep(&self) -> Sleep {
        self.sleep
    }

    pub fn set_sleep(&mut self, sleep: Sleep) {
        self.sleep = sleep;
    }

    /// The next thing to say, or `None` once the chapter is finished.
    pub fn next_utterance(&mut self) -> Option<Utterance> {
        if !self.playing {
            return None;
        }
        let utterance = self.script.get(self.at).cloned()?;
        self.at += 1;
        Some(utterance)
    }

    pub fn play(&mut self) {
        self.playing = true;
    }

    pub fn pause(&mut self) {
        self.playing = false;
    }

    /// Back to the top of the chapter, paused.
    pub fn stop(&mut self) {
        self.playing = false;
        self.at = 0;
    }

    /// Every verse has been said.
    pub fn finished(&self) -> bool {
        self.at >= self.script.len()
    }

    /// Begins at a verse the reader was up to: the announcement is kept, earlier verses skipped.
    pub fn seek_to_verse(&mut self, verse: u8) {
        let from = self
            .script
            .iter()
            .position(|u| matches!(u.verse, Some(v) if v >= verse))
            .unwrap_or(self.script.len());
        self.at = from;
    }
}
