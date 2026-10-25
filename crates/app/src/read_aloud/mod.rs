//! Read aloud: the on-device voice.
//!
//! The words live in `bible_core::speak`; this module owns the sound — the model, the audio device
//! and the tempo the reader approved. Nothing here touches the network: the weights are bundled
//! beside the app and every sample is made on this machine.

mod player;
mod reading;
mod ui;

pub use reading::{NextChapter, Reading, ReadingState};
pub use ui::Listen;

use bible_core::books::{ChapterIdx, VerseRef};

/// What Listen reads: the passage the reader has selected, or else the chapter at the top of the
/// view. A selection is the reader saying "here", and reading the chapter they scrolled past
/// instead is the app ignoring them.
pub fn chapter_to_read(selection: Option<(VerseRef, VerseRef)>, location: ChapterIdx) -> ChapterIdx {
    selection.map_or(location, |(from, _)| from.chapter_idx())
}

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use bible_core::speak::Voice;
use ptts::checkpoint::{Checkpoint, ResolveOptions};
use ptts::preprocess::Lang;
use ptts::synth::{DeviceKind, Quant, SpeechOptions, Synth};

/// The line a voice says when the reader presses play in Settings: familiar, and public domain.
pub const SAMPLE: &str = "In the beginning God created the heaven and the earth.";

/// Where the model lives: packaged beside the app, or the repository's folder when developing.
fn model_dir() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(dir) = std::env::var("BIBLE_RS_SPEECH") {
        candidates.push(PathBuf::from(dir));
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        candidates.push(dir.join("speech"));
        candidates.push(dir.join("../Resources/speech"));
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/speech"));
    candidates.into_iter().find(|dir| dir.join("model.q8.gguf").exists())
}

fn voice_path(dir: &std::path::Path, voice: Voice) -> PathBuf {
    dir.join(format!("{}.safetensors", voice.name.to_ascii_lowercase()))
}

/// The chapter after `from`, with what it says. How a reading carries on when the timer still has time
/// on it: chapters a translation does not have are passed over, and `None` means there is no next one.
pub fn next_chapter_text(
    text: &bible_core::Translation,
    from: ChapterIdx,
) -> Option<(ChapterIdx, Vec<(u32, String)>)> {
    let mut at = from.next()?;
    loop {
        let verses: Vec<(u32, String)> =
            text.verses_in(at).map(|v| (v.number as u32, v.text.to_string())).collect();
        if !verses.is_empty() {
            return Some((at, verses));
        }
        at = at.next()?;
    }
}

/// Loads the model once, with every shipped voice registered, and keeps the samples it has made.
pub(super) struct Engine {
    tts: Synth,
    rate: u32,
    samples: HashMap<&'static str, Arc<Vec<f32>>>,
}

static ENGINE: OnceLock<Mutex<Engine>> = OnceLock::new();

fn engine() -> Result<&'static Mutex<Engine>, String> {
    if let Some(engine) = ENGINE.get() {
        return Ok(engine);
    }
    let dir = model_dir().ok_or_else(|| "the voice model is not installed".to_string())?;
    let dir_str = dir.to_str().ok_or("the model path is not UTF-8")?.to_string();
    let checkpoint = Checkpoint::locate(&dir_str, ResolveOptions { quant: Quant::Q80, weights: None })
        .map_err(|e| format!("the voice model could not be opened: {e}"))?;
    let mut builder = checkpoint.builder(Lang::En).device(DeviceKind::Cpu);
    for voice in bible_core::speak::VOICES {
        builder = builder.add_voice(voice.name, voice_path(&dir, voice));
    }
    let mut tts = builder.build().map_err(|e| format!("the voice could not start: {e}"))?;
    checkpoint.register_voices(&mut tts);
    let rate = tts.sample_rate();
    let engine = Engine { tts, rate, samples: HashMap::new() };
    Ok(ENGINE.get_or_init(|| Mutex::new(engine)))
}

/// One voice saying the sample, at the pace that voice is calibrated to, cached after the first go.
pub fn sample(voice: Voice) -> Result<Arc<Vec<f32>>, String> {
    let engine = engine()?;
    let mut engine = engine.lock().map_err(|_| "the voice stopped".to_string())?;
    if let Some(pcm) = engine.samples.get(voice.name) {
        return Ok(pcm.clone());
    }
    let pcm = engine.speak(SAMPLE, voice)?;
    engine.samples.insert(voice.name, pcm.clone());
    Ok(pcm)
}

impl Engine {
    /// Speech for `text` in `voice`, at the approved pace. Blocking; a verse takes about a fifth of
    /// a second to make, which is why playback runs on its own thread.
    fn speak(&mut self, text: &str, voice: Voice) -> Result<Arc<Vec<f32>>, String> {
        let options = SpeechOptions::default().voice(voice.name);
        let mut pcm = Vec::new();
        let stream = self
            .tts
            .stream_with(text, &options)
            .map_err(|e| format!("the voice could not speak: {e}"))?;
        for chunk in stream {
            pcm.extend_from_slice(&chunk.map_err(|e| format!("the voice stopped early: {e}"))?);
        }
        // The model leaves most of a second of quiet after its last word. Trim it, or every pause
        // the reader asks for is really that pause plus the tail, and nothing can be tuned.
        let trimmed = bible_core::speak::trim_silence(&pcm, self.rate);
        let stretched = bible_core::speak::stretch(&trimmed, self.rate, voice.tempo());
        // The voice also pauses at punctuation for up to a second, three times the breath we ask for
        // between sentences, which stops the reading dead mid-paragraph. Shorten those to a narrator's
        // pause so a passage runs at one pace.
        Ok(Arc::new(bible_core::speak::cap_gaps(&stretched, self.rate)))
    }
}

/// Says the sample through the default output device, off the UI thread.
pub fn play_sample(voice: Voice) {
    std::thread::spawn(move || match sample(voice) {
        Ok(pcm) => {
            let controls = Arc::new(player::Controls::default());
            if let Err(e) = player::play(&pcm, 24_000, &controls) {
                eprintln!("read aloud: {e}");
            }
        }
        Err(e) => eprintln!("read aloud: {e}"),
    });
}

/// Plays 24 kHz mono audio through the default output device, blocking until it has. The Settings
/// button uses `play_sample`; this exists so a check can prove the device path without a mouse.
pub fn play_for_test(pcm: &[f32]) -> Result<(), String> {
    let pcm = Arc::new(pcm.to_vec());
    let controls = Arc::new(player::Controls::default());
    player::play(&pcm, 24_000, &controls).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bible_core::books::BookId;

    fn kjv() -> bible_core::Translation {
        bible_core::bundled::load("kjv").expect("kjv")
    }

    #[test]
    fn the_next_chapter_follows_the_reading() {
        let (at, verses) = next_chapter_text(&kjv(), ChapterIdx::new(BookId::from_code("MAT").unwrap(), 4).unwrap())
            .expect("matthew 5");
        assert_eq!(at.chapter(), 5);
        assert_eq!(verses.first().map(|(n, _)| *n), Some(1));
        assert!(verses.len() > 40, "Matthew 5 has {} verses", verses.len());
    }

    #[test]
    fn the_next_chapter_crosses_into_the_next_book() {
        let (at, verses) = next_chapter_text(&kjv(), ChapterIdx::new(BookId::from_code("MAL").unwrap(), 4).unwrap())
            .expect("matthew 1");
        assert_eq!(at.book().name(), "Matthew", "the reading carries on past the end of a book");
        assert_eq!(at.chapter(), 1);
        assert!(verses[0].1.contains("generation") || verses[0].1.contains("book"), "{}", verses[0].1);
    }

    #[test]
    fn the_last_chapter_of_the_bible_has_nothing_after_it() {
        assert!(next_chapter_text(&kjv(), ChapterIdx::LAST).is_none());
    }
}
