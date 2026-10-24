//! One chapter being read out: a thread that speaks verse by verse and obeys the reader.
//!
//! Each verse is made just before it is played, so the first words arrive in about a tenth of a
//! second rather than after the whole chapter is synthesised. Between verses the thread holds the
//! pause the script asks for, and that silence goes through the same device, so pausing during a
//! breath works the way a reader expects.

use std::sync::{Arc, Mutex};

use bible_core::speak::{Session, Sleep, Voice};

use super::engine;
use super::player::{self, Controls};

/// The 24 kHz the model speaks at.
const RATE: u32 = 24_000;
/// How long the last verse fades when the sleep timer ends the reading.
const FADE_SECONDS: f32 = 0.9;

/// What the reading bar shows, kept up to date by the reading thread.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReadingState {
    pub verse: Option<u8>,
    pub speaking: bool,
    pub finished: bool,
    pub remaining: Option<f32>,
    /// Something the reader should know, such as a voice that would not start.
    pub note: Option<String>,
}

/// A chapter being read aloud, and the reader's control over it.
pub struct Reading {
    controls: Arc<Controls>,
    session: Arc<Mutex<Session>>,
    state: Arc<Mutex<ReadingState>>,
}

impl Reading {
    /// Opens a reading without speaking: the reader gets the controls, then presses play.
    pub fn open(mut session: Session, voice: Voice) -> Reading {
        session.pause();
        let controls = Arc::new(Controls::default());
        controls.set_paused(true);
        let session = Arc::new(Mutex::new(session));
        let state = Arc::new(Mutex::new(ReadingState::default()));
        {
            let session = session.clone();
            let state = state.clone();
            let controls = controls.clone();
            std::thread::spawn(move || speak(session, controls, state, voice));
        }
        Reading { controls, session, state }
    }

    /// Starts speaking `session` in `voice` on its own thread.
    pub fn start(session: Session, voice: Voice) -> Reading {
        let reading = Reading::open(session, voice);
        reading.resume();
        reading
    }

    pub fn pause(&self) {
        self.controls.set_paused(true);
        if let Ok(mut session) = self.session.lock() {
            session.pause();
        }
        self.update(|state| state.speaking = false);
    }

    pub fn resume(&self) {
        self.controls.set_paused(false);
        if let Ok(mut session) = self.session.lock() {
            session.play();
        }
        self.update(|state| {
            state.speaking = true;
            state.finished = false;
        });
    }

    pub fn stop(&self) {
        self.controls.stop();
        if let Ok(mut session) = self.session.lock() {
            session.stop();
        }
        self.update(|state| {
            state.speaking = false;
            state.finished = true;
        });
    }

    pub fn set_sleep(&self, sleep: Sleep) {
        if let Ok(mut session) = self.session.lock() {
            session.set_sleep(sleep);
            let remaining = session.remaining();
            self.update(|state| state.remaining = remaining);
        }
    }

    pub fn sleep(&self) -> Sleep {
        self.session.lock().map(|s| s.sleep()).unwrap_or_default()
    }

    pub fn state(&self) -> ReadingState {
        self.state.lock().map(|s| s.clone()).unwrap_or_default()
    }

    fn update(&self, change: impl FnOnce(&mut ReadingState)) {
        if let Ok(mut state) = self.state.lock() {
            change(&mut state);
        }
    }
}

/// The reading thread: take the next utterance, make it hearable, play it, count the seconds.
fn speak(session: Arc<Mutex<Session>>, controls: Arc<Controls>, state: Arc<Mutex<ReadingState>>, voice: Voice) {
    let outcome = run(&session, &controls, &state, voice);
    if let Ok(mut state) = state.lock() {
        state.speaking = false;
        state.finished = true;
        if let Err(e) = outcome {
            state.note = Some(e);
        }
    }
}

fn run(
    session: &Arc<Mutex<Session>>,
    controls: &Arc<Controls>,
    state: &Arc<Mutex<ReadingState>>,
    voice: Voice,
) -> Result<(), String> {
    let mut stopped_by_timer = false;
    loop {
        if controls.stopped() {
            break;
        }
        if controls.paused() {
            std::thread::sleep(std::time::Duration::from_millis(40));
            continue;
        }
        let utterance = {
            let mut guard = session.lock().map_err(|_| "the reading was lost".to_string())?;
            guard.next()
        };
        let Some(utterance) = utterance else {
            break;
        };
        let mut pcm = {
            let engine = engine()?;
            let mut engine = engine.lock().map_err(|_| "the voice was lost".to_string())?;
            engine.speak(&utterance.text, voice)?
        };
        let verse_seconds = pcm.len() as f32 / RATE as f32;
        // The sleep timer finishes the verse in progress, softly: if this verse is the last one it
        // covers, its tail fades so no one is cut off mid-word.
        let last = match session.lock() {
            Ok(guard) => guard.remaining().is_some_and(|left| left <= verse_seconds),
            Err(_) => false,
        };
        if last {
            let pcm = Arc::make_mut(&mut pcm);
            let total = pcm.len();
            let fade = ((FADE_SECONDS * RATE as f32) as usize).min(total);
            for (i, sample) in pcm.iter_mut().enumerate().skip(total - fade) {
                let left = (total - i) as f32 / fade as f32;
                *sample *= left;
            }
            stopped_by_timer = true;
        }
        // The breath after the verse rides along, so pause and stop apply to it too.
        let breath = (utterance.pause_ms as u64 * RATE as u64 / 1000) as usize;
        if breath > 0 {
            Arc::make_mut(&mut pcm).extend(std::iter::repeat_n(0.0, breath));
        }
        if let Ok(mut state) = state.lock() {
            state.verse = utterance.verse;
            state.speaking = true;
            state.remaining = session.lock().ok().and_then(|s| s.remaining());
        }
        let heard = player::play(&pcm, RATE, controls)?;
        let expired = match session.lock() {
            Ok(mut guard) => guard.spoke(heard),
            Err(_) => true,
        };
        if let Ok(mut state) = state.lock() {
            state.remaining = session.lock().ok().and_then(|s| s.remaining());
        }
        if expired || stopped_by_timer || controls.stopped() {
            break;
        }
    }
    controls.stop();
    Ok(())
}
