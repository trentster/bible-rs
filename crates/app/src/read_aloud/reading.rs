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

use bible_core::books::ChapterIdx;
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
    /// The chapter being heard, which is not always the chapter the bar was opened on.
    pub chapter: Option<ChapterIdx>,
    pub verse: Option<u8>,
    /// The last verse of the sentence being spoken: a sentence runs on past a verse number, so the
    /// page tints all of it rather than one verse at a time.
    pub until: Option<u8>,
    pub speaking: bool,
    pub finished: bool,
    /// Something the reader should know, such as a voice that would not start.
    pub note: Option<String>,
}

/// When the reading ends by itself, on the machine's clock. Shared with the bar, so the countdown it
/// paints and the moment the voice stops are the same moment.
type Deadline = Arc<Mutex<Option<Instant>>>;

/// Gives the reading the chapter after one that has just ended, with its session already playing.
///
/// A timer of fifteen minutes on a four-minute chapter used to stop after four minutes, which made the
/// timer a cap rather than a sleep timer: someone falling asleep asked to be read to for fifteen minutes
/// and got four. With a timer still running, the reading now carries on into the next chapter until the
/// clock says stop. Without a timer there is nothing to carry on for, so the chapter's end is the end.
pub type NextChapter = Arc<dyn Fn(ChapterIdx) -> Option<(ChapterIdx, Session)> + Send + Sync>;

/// A chapter being read aloud, and the reader's control over it.
pub struct Reading {
    controls: Arc<Controls>,
    session: Arc<Mutex<Session>>,
    state: Arc<Mutex<ReadingState>>,
    deadline: Deadline,
    /// What was left on the timer when the reader paused. A pause is a pause for the clock too, so
    /// the countdown waits on this and the deadline is put back when play comes.
    held: Arc<Mutex<Option<f32>>>,
    /// The reader's choice from the timer menu, kept here rather than on a chapter's session: a reading
    /// can walk into the next chapter, and the menu must not forget the timer on the way.
    sleep: Mutex<Sleep>,
}

impl Reading {
    /// Opens a reading without speaking: the reader gets the controls, then presses play.
    pub fn open(session: Session, voice: Voice, at: ChapterIdx, next: Option<NextChapter>) -> Reading {
        let mut session = session;
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
            std::thread::spawn(move || speak(session, controls, state, deadline, voice, at, next));
        }
        Reading { controls, session, state, deadline, held, sleep: Mutex::new(Sleep::default()) }
    }

    /// Starts speaking `session` in `voice` on its own thread.
    pub fn start(session: Session, voice: Voice, at: ChapterIdx, next: Option<NextChapter>) -> Reading {
        let reading = Reading::open(session, voice, at, next);
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
        if let Ok(mut chosen) = self.sleep.lock() {
            *chosen = sleep;
        }
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
            let run_out = deadline.lock().ok().and_then(|at| *at).is_some_and(|at| at <= Instant::now());
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
        self.sleep.lock().map(|chosen| *chosen).unwrap_or_default()
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

/// The reading thread: make the chapter hearable slightly ahead of time and play it, until the chapter
/// ends or the clock says so.
fn speak(
    session: Arc<Mutex<Session>>,
    controls: Arc<Controls>,
    state: Arc<Mutex<ReadingState>>,
    deadline: Deadline,
    voice: Voice,
    at: ChapterIdx,
    next: Option<NextChapter>,
) {
    let outcome = run(&session, &controls, &state, &deadline, voice, at, next.as_ref());
    if let Ok(mut state) = state.lock() {
        state.speaking = false;
        state.finished = true;
        if let Err(e) = outcome {
            state.note = Some(e);
        }
    }
}

/// How many pieces are made ahead of the one being heard. One is enough in theory — while a piece
/// plays, the next is being made — but a long passage can take longer to make than the piece before it
/// takes to hear (measured: 2.4 s to make 12.9 s of speech), so three are kept ready. Without any, the
/// reading stopped dead between pieces for as long as the voice took to make the next one: measured
/// from a recording, 2.8 seconds of silence in the middle of Matthew 15. Traced after the change, the
/// player waits only for the very first piece, 745 ms.
const AHEAD: usize = 3;

/// A piece of the chapter on its way to being heard.
struct Heard {
    pcm: Arc<Vec<f32>>,
    chapter: ChapterIdx,
    verse: Option<u8>,
    until: Option<u8>,
    breath: usize,
}

fn run(
    session: &Arc<Mutex<Session>>,
    controls: &Arc<Controls>,
    state: &Arc<Mutex<ReadingState>>,
    deadline: &Deadline,
    voice: Voice,
    at: ChapterIdx,
    next: Option<&NextChapter>,
) -> Result<(), String> {
    let (made, heard) = std::sync::mpsc::sync_channel::<Result<Heard, String>>(AHEAD);
    std::thread::scope(|scope| {
        let making = Making { session, controls, state, deadline, voice, at, next };
        let maker = scope.spawn(move || make(making, made));
        let played = play(controls, state, deadline, &heard);
        controls.stop();
        // Dropping the listening end is what unblocks a maker waiting to hand a piece over.
        drop(heard);
        let _ = maker.join();
        played
    })
}

/// Makes the pieces ahead of the reader's ears, so that playback never waits for the voice.
/// What the making thread works from: the chapter, the reader's controls, and the way forward from it.
struct Making<'a> {
    session: &'a Arc<Mutex<Session>>,
    controls: &'a Arc<Controls>,
    state: &'a Arc<Mutex<ReadingState>>,
    deadline: &'a Deadline,
    voice: Voice,
    at: ChapterIdx,
    next: Option<&'a NextChapter>,
}

fn make(making: Making<'_>, out: std::sync::mpsc::SyncSender<Result<Heard, String>>) {
    let Making { session, controls, state, deadline, voice, mut at, next } = making;
    loop {
        if controls.stopped() {
            return;
        }
        // A session that has not been played yields nothing, and a paused one yields nothing either.
        // Reading that as "the chapter is over" ended the reading before a word was said, which is
        // exactly what a reading opened and not yet played looks like.
        if controls.paused() {
            std::thread::sleep(Duration::from_millis(40));
            continue;
        }
        let (next_utterance, over) = {
            let Ok(mut guard) = session.lock() else { return };
            match guard.next_utterance() {
                Some(utterance) => (Some(utterance), false),
                None => (None, guard.finished()),
            }
        };
        let Some(utterance) = next_utterance else {
            if over {
                // The chapter has been read. With time still on the timer, carry on into the next one;
                // without a timer there is nothing to carry on for.
                let Some(supplier) = next else { return };
                if left_on(deadline).is_none() {
                    return;
                }
                let Some((ahead, fresh)) = supplier(at) else { return };
                if let Ok(mut guard) = session.lock() {
                    *guard = fresh;
                }
                at = ahead;
                if let Ok(mut state) = state.lock() {
                    state.chapter = Some(ahead);
                    state.verse = None;
                }
                continue;
            }
            // Paused between the check above and the question asked here: wait for play.
            std::thread::sleep(Duration::from_millis(20));
            continue;
        };
        let spoken = match engine() {
            Ok(engine) => match engine.lock() {
                Ok(mut engine) => engine.speak(&utterance.text, voice),
                Err(_) => Err("the voice was lost".to_string()),
            },
            Err(e) => Err(e),
        };
        let failed = spoken.is_err();
        let item = spoken.map(|pcm| Heard {
            pcm,
            chapter: at,
            verse: utterance.verse,
            until: utterance.until,
            // The breath rides along, so pause and stop apply to it too.
            breath: (utterance.pause_ms as u64 * RATE as u64 / 1000) as usize,
        });
        if out.send(item).is_err() || failed {
            return;
        }
    }
}

/// Plays what was made until the chapter ends, the reader stops it, or the clock runs out.
fn play(
    controls: &Arc<Controls>,
    state: &Arc<Mutex<ReadingState>>,
    deadline: &Deadline,
    heard: &std::sync::mpsc::Receiver<Result<Heard, String>>,
) -> Result<(), String> {
    loop {
        if controls.stopped() {
            break;
        }
        // The timer stops with the voice, so a paused reading cannot run it out: this is the guard for
        // a deadline that came due as the verse was ending.
        if left_on(deadline).is_some_and(|left| left <= 0.0) {
            break;
        }
        if controls.paused() {
            std::thread::sleep(Duration::from_millis(40));
            continue;
        }
        match heard.recv() {
            Ok(Ok(piece)) => {
                if play_piece(piece, controls, state, deadline)? {
                    break;
                }
            }
            Ok(Err(e)) => return Err(e),
            // Nothing left to make: the chapter has been read.
            Err(_) => break,
        }
    }
    controls.stop();
    Ok(())
}

/// Plays one piece, cut to the clock when the deadline falls inside it. `true` when it was the last.
fn play_piece(
    piece: Heard,
    controls: &Arc<Controls>,
    state: &Arc<Mutex<ReadingState>>,
    deadline: &Deadline,
) -> Result<bool, String> {
    let mut pcm = piece.pcm;
    let seconds = pcm.len() as f32 / RATE as f32;
    // The deadline falls inside this piece: it is said only as far as the clock allows, and its last
    // breath fades so no one is cut off mid-word.
    let last = match left_on(deadline) {
        Some(left) if left <= seconds => {
            let keep = ((left.max(0.0) * RATE as f32) as usize).min(pcm.len());
            Arc::make_mut(&mut pcm).truncate(keep);
            fade_out(&mut pcm);
            true
        }
        _ => false,
    };
    if piece.breath > 0 {
        Arc::make_mut(&mut pcm).extend(std::iter::repeat_n(0.0, piece.breath));
    }
    if let Ok(mut state) = state.lock() {
        state.chapter = Some(piece.chapter);
        state.verse = piece.verse;
        state.until = piece.until;
        state.speaking = true;
    }
    player::play(&pcm, RATE, controls)?;
    Ok(last || controls.stopped())
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
    fn pressing_play_on_an_opened_reading_speaks_rather_than_ending() {
        // The one test that needs the voice model: without it there is nothing to speak and the
        // failure it guards against cannot be told apart from a missing model.
        if super::super::engine().is_err() {
            return;
        }
        let verses = vec![(1, "For this Melchisedec, king of Salem.".to_string())];
        let session = Session::new("Hebrews", 7, &verses, Sleep::Off);
        let at = ChapterIdx::FIRST;
        let reading = Reading::open(session, Voice::by_name("Spencer").unwrap(), at, None);
        // Opening is not playing: nothing is said, and nothing is over.
        std::thread::sleep(Duration::from_millis(150));
        assert!(!reading.state().finished, "a reading nobody has played has not finished");
        // Now press play, as the toolbar does. A reading that was abandoned while it waited for play
        // ends instantly here instead of speaking.
        reading.resume();
        std::thread::sleep(Duration::from_millis(300));
        let state = reading.state();
        reading.stop();
        assert!(!state.finished, "play on an opened reading ended it instead of speaking");
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

    #[test]
    fn a_new_chapter_does_not_take_the_timer_with_it() {
        let verses = vec![(1, "For this Melchisedec, king of Salem.".to_string())];
        let session = Session::new("Hebrews", 7, &verses, Sleep::Off);
        let reading = Reading::open(session, Voice::by_name("Spencer").unwrap(), ChapterIdx::FIRST, None);
        reading.choose(Sleep::Minutes(15));
        assert_eq!(reading.sleep(), Sleep::Minutes(15));
        // A roll-over swaps the chapter's session underneath the reading, and the replacement knows
        // nothing of the reader's choice. The bar's menu reads the reading, not the chapter.
        if let Ok(mut guard) = reading.session.lock() {
            *guard = Session::new("Hebrews", 8, &verses, Sleep::Off);
        }
        assert_eq!(
            reading.sleep(),
            Sleep::Minutes(15),
            "the reading walked into a new chapter and forgot the timer"
        );
    }
}
