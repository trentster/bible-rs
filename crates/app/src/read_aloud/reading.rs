//! One chapter being read out: a thread that speaks verse by verse and obeys the reader.
//!
//! Each verse is made just before it is played, so the first words arrive in about a tenth of a
//! second rather than after the whole chapter is synthesised. Between verses the thread holds the
//! pause the script asks for, and that silence goes through the same device, so pausing during a
//! breath works the way a reader expects.
//!
//! The sleep timer is a deadline on the clock, not a tally of the words spoken: choose fifteen
//! minutes and the reading ends fifteen minutes later, wherever it has got to. Nothing about how
//! long a verse takes to make or to say can change it, so the bar can show a plain countdown.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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
    /// Something the reader should know, such as a voice that would not start.
    pub note: Option<String>,
}

/// When the reading ends by itself, on the machine's clock. Shared with the bar, so the countdown it
/// paints and the moment the voice stops are the same moment.
type Deadline = Arc<Mutex<Option<Instant>>>;

/// A chapter being read aloud, and the reader's control over it.
pub struct Reading {
    controls: Arc<Controls>,
    session: Arc<Mutex<Session>>,
    state: Arc<Mutex<ReadingState>>,
    deadline: Deadline,
    /// What was left on the timer when the reader paused. A pause is a pause for the clock too, so
    /// the countdown waits on this and the deadline is put back when play comes.
    held: Arc<Mutex<Option<f32>>>,
}

impl Reading {
    /// Opens a reading without speaking: the reader gets the controls, then presses play.
    pub fn open(mut session: Session, voice: Voice) -> Reading {
        session.pause();
        let controls = Arc::new(Controls::default());
        controls.set_paused(true);
        let session = Arc::new(Mutex::new(session));
        let state = Arc::new(Mutex::new(ReadingState::default()));
        let deadline: Deadline = Arc::new(Mutex::new(None));
        let held: Arc<Mutex<Option<f32>>> = Arc::new(Mutex::new(None));
        {
            let session = session.clone();
            let state = state.clone();
            let controls = controls.clone();
            let deadline = deadline.clone();
            std::thread::spawn(move || speak(session, controls, state, deadline, voice));
        }
        Reading { controls, session, state, deadline, held }
    }

    /// Starts speaking `session` in `voice` on its own thread.
    pub fn start(session: Session, voice: Voice) -> Reading {
        let reading = Reading::open(session, voice);
        reading.resume();
        reading
    }

    pub fn pause(&self) {
        // The clock stops with the voice: what is left is held, and the deadline is taken away so
        // nothing runs it down while the reader is away.
        if let Some(left) = left_on(&self.deadline) {
            if let Ok(mut held) = self.held.lock() {
                *held = Some(left);
            }
            self.disarm();
        }
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
        // Back to the time that was held, or — from a standing start — whatever the menu says.
        let held = self.held.lock().ok().and_then(|held| *held);
        let seconds = held.or_else(|| match self.sleep() {
            Sleep::Minutes(minutes) => Some(minutes as f32 * 60.0),
            Sleep::Off => None,
        });
        self.start_timer(seconds);
    }

    pub fn stop(&self) {
        self.controls.stop();
        if let Ok(mut session) = self.session.lock() {
            session.stop();
        }
        self.disarm();
        if let Ok(mut held) = self.held.lock() {
            *held = None;
        }
        self.update(|state| {
            state.speaking = false;
            state.finished = true;
        });
    }

    /// The reader's choice from the timer menu. Choosing one mid-reading starts it from now, which is
    /// what pressing a menu row means; choosing one before playing waits for play.
    pub fn choose(&self, sleep: Sleep) {
        if let Ok(mut session) = self.session.lock() {
            session.set_sleep(sleep);
        }
        if self.state().speaking {
            self.arm();
        } else {
            // Not playing yet: hold the time the choice buys, ready for play.
            if let Ok(mut held) = self.held.lock() {
                *held = match sleep {
                    Sleep::Minutes(minutes) => Some(minutes as f32 * 60.0),
                    Sleep::Off => None,
                };
            }
            self.disarm();
        }
    }

    /// Starts the reader's countdown from this moment.
    pub fn arm(&self) {
        match self.sleep() {
            Sleep::Minutes(minutes) => self.arm_for(minutes as u64 * 60),
            Sleep::Off => self.disarm(),
        }
    }

    /// Starts a countdown of `seconds` from this moment, whatever the menu's choices are: the menu
    /// deals in minutes, but the clock does not care.
    pub fn arm_for(&self, seconds: u64) {
        self.start_timer(Some(seconds as f32));
    }

    /// Starts a countdown of `seconds` from this moment, or takes the timer away for `None`.
    fn start_timer(&self, seconds: Option<f32>) {
        if let Ok(mut held) = self.held.lock() {
            *held = None;
        }
        let Some(seconds) = seconds else {
            self.disarm();
            return;
        };
        let seconds = Duration::from_secs_f32(seconds.max(0.0));
        let at = Instant::now() + seconds;
        if let Ok(mut deadline) = self.deadline.lock() {
            *deadline = Some(at);
        }
        // The reading thread cuts its verse to fit the deadline, but it cannot do that while it is
        // inside the player — paused mid-verse, say. So the clock itself stops the voice, and the
        // thread then finds the reading over.
        let controls = self.controls.clone();
        let deadline = self.deadline.clone();
        std::thread::spawn(move || {
            std::thread::sleep(seconds);
            let run_out = deadline
                .lock()
                .ok()
                .and_then(|at| *at)
                .is_some_and(|at| at <= Instant::now());
            if run_out {
                controls.stop();
            }
        });
    }

    fn disarm(&self) {
        if let Ok(mut deadline) = self.deadline.lock() {
            *deadline = None;
        }
    }

    /// What is left on the timer, or `None` when it is off. Straight off the clock, so it runs down
    /// smoothly whatever the voice is doing.
    pub fn left(&self) -> Option<f32> {
        match self.held.lock().ok().and_then(|held| *held) {
            Some(left) => Some(left),
            None => left_on(&self.deadline),
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

/// Seconds left before the deadline, or `None` when no timer is armed.
fn left_on(deadline: &Deadline) -> Option<f32> {
    let at = deadline.lock().ok().and_then(|at| *at)?;
    Some(at.saturating_duration_since(Instant::now()).as_secs_f32())
}

/// The reading thread: take the next utterance, make it hearable, play it, until the chapter ends or
/// the clock says so.
fn speak(
    session: Arc<Mutex<Session>>,
    controls: Arc<Controls>,
    state: Arc<Mutex<ReadingState>>,
    deadline: Deadline,
    voice: Voice,
) {
    let outcome = run(&session, &controls, &state, &deadline, voice);
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
    deadline: &Deadline,
    voice: Voice,
) -> Result<(), String> {
    loop {
        if controls.stopped() {
            break;
        }
        // The timer stops with the voice, so a paused reading cannot run it out: this is the guard
        // for a deadline that came due as the verse was ending.
        if left_on(deadline).is_some_and(|left| left <= 0.0) {
            break;
        }
        if controls.paused() {
            std::thread::sleep(Duration::from_millis(40));
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
        // The deadline falls inside this verse: it is said only as far as the clock allows, and its
        // last breath fades so no one is cut off mid-word.
        let last = match left_on(deadline) {
            Some(left) if left <= verse_seconds => {
                let keep = ((left.max(0.0) * RATE as f32) as usize).min(pcm.len());
                Arc::make_mut(&mut pcm).truncate(keep);
                fade_out(&mut pcm);
                true
            }
            _ => false,
        };
        // The breath after the verse rides along, so pause and stop apply to it too.
        let breath = (utterance.pause_ms as u64 * RATE as u64 / 1000) as usize;
        if breath > 0 {
            Arc::make_mut(&mut pcm).extend(std::iter::repeat_n(0.0, breath));
        }
        if let Ok(mut state) = state.lock() {
            state.verse = utterance.verse;
            state.speaking = true;
        }
        player::play(&pcm, RATE, controls)?;
        if last || controls.stopped() {
            break;
        }
    }
    controls.stop();
    Ok(())
}

/// Takes the voice down to nothing over the last `FADE_SECONDS`, so the ending is heard as an ending.
fn fade_out(pcm: &mut Arc<Vec<f32>>) {
    let pcm = Arc::make_mut(pcm);
    let total = pcm.len();
    let fade = ((FADE_SECONDS * RATE as f32) as usize).min(total);
    for (i, sample) in pcm.iter_mut().enumerate().skip(total - fade) {
        let left = (total - i) as f32 / fade.max(1) as f32;
        *sample *= left;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deadline_at(at: Option<Instant>) -> Deadline {
        Arc::new(Mutex::new(at))
    }

    #[test]
    fn no_timer_means_no_countdown() {
        assert_eq!(left_on(&deadline_at(None)), None);
    }

    #[test]
    fn the_countdown_is_the_clock_to_the_deadline() {
        let left = left_on(&deadline_at(Some(Instant::now() + Duration::from_secs(900)))).unwrap();
        assert!((left - 900.0).abs() < 1.0, "fifteen minutes, not {left}");
        // A deadline already gone reads zero rather than a negative number.
        let spent = left_on(&deadline_at(Some(Instant::now() - Duration::from_secs(5)))).unwrap();
        assert_eq!(spent, 0.0);
    }

    #[test]
    fn the_fade_leaves_the_start_and_silences_the_end() {
        let mut pcm = Arc::new(vec![1.0f32; RATE as usize * 5]);
        fade_out(&mut pcm);
        assert_eq!(pcm[0], 1.0, "the verse is not touched until its last breath");
        assert!(pcm[pcm.len() - 1].abs() < 0.01, "and ends in silence");
        let fade = (FADE_SECONDS * RATE as f32) as usize;
        assert!(pcm[pcm.len() - fade - 1] > 0.99, "the fade covers about a second");
    }
}
