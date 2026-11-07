//! Read Aloud on the phone: what the voice is made of, how to check it is all there, and making it
//! speak.
//!
//! The split is the same one the Mac makes, with one difference that matters. The Mac fetches the
//! voice with `ureq`, which means it carries TLS of its own. The phone fetches with Apple's
//! `URLSession` instead, so the iOS build ships no encryption code it has to explain to a reviewer.
//! Both sides fetch the same eight files and check the same eight digests, out of [`bundle`], and
//! both hand the samples to a platform audio stack — the pipeline that makes them is [`Speech`],
//! here, once.
//!
//! Nothing in this module is on the launch path. The model is opened when a reader presses Listen,
//! not before: it is 66 MB of weights and it wants a second or two.

use std::path::PathBuf;
use std::sync::Mutex;

use bible_core::speak::{VOICES, bundle, engine::Speech as Engine};

use crate::BibleError;

/// One of the voices that ships with the model.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct VoiceInfo {
    /// The name the model knows it by: "russell", "harper".
    pub name: String,
    /// "Russell" — the name as a reader should see it.
    pub label: String,
    /// "US" or "UK".
    pub accent: String,
    pub female: bool,
    /// Words per minute this voice reads at untouched. Measured, not guessed.
    pub native_wpm: u16,
}

/// One file the voice is made of, and the digest it must have. The phone fetches it with
/// `URLSession` and checks the digest before it counts as arriving.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct VoiceFile {
    pub name: String,
    pub sha256: String,
}

/// One thing Read Aloud says: a sentence or a clause, how long to breathe after it, and the verse
/// it belongs to (nothing for the opening announcement).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SpokenUtterance {
    pub text: String,
    pub pause_ms: u32,
    pub verse: Option<u8>,
}

/// The voices that ship with the model.
#[uniffi::export]
pub fn voices() -> Vec<VoiceInfo> {
    VOICES
        .iter()
        .map(|voice| VoiceInfo {
            name: voice.name.to_string(),
            label: voice.name.to_string(),
            accent: match voice.accent {
                bible_core::speak::Accent::UnitedStates => "US".to_string(),
                bible_core::speak::Accent::UnitedKingdom => "UK".to_string(),
            },
            female: voice.female,
            native_wpm: voice.native_wpm,
        })
        .collect()
}

/// Every file the voice needs, with its digest.
#[uniffi::export]
pub fn voice_files() -> Vec<VoiceFile> {
    bundle::FILES
        .iter()
        .map(|(name, sha256)| VoiceFile { name: (*name).to_string(), sha256: (*sha256).to_string() })
        .collect()
}

/// Where the files come from, before a name is appended.
#[uniffi::export]
pub fn voice_bundle_url() -> String {
    bundle::BUNDLE_URL.to_string()
}

/// What the download weighs in total, for a progress figure a reader can judge.
#[uniffi::export]
pub fn voice_total_bytes() -> u64 {
    bundle::TOTAL_BYTES
}

/// Whether every file is in `dir`. All eight, or it is not there.
#[uniffi::export]
pub fn voice_complete(dir: String) -> bool {
    bundle::complete(&PathBuf::from(dir))
}

/// The model, open and ready to speak. Holds 66 MB of weights, so it is made when a reader asks to
/// be read to and kept for the session.
#[derive(uniffi::Object)]
pub struct Speech {
    engine: Mutex<Engine>,
    rate: u32,
}

#[uniffi::export]
impl Speech {
    /// Opens the voice in `dir`. The message that comes back is the one the reader sees, so it says
    /// what a person can do rather than what a pointer was null.
    #[uniffi::constructor]
    pub fn load(dir: String) -> Result<std::sync::Arc<Self>, BibleError> {
        let engine = Engine::open(&PathBuf::from(dir)).map_err(|why| BibleError::Failed { message: why })?;
        let rate = engine.sample_rate();
        Ok(std::sync::Arc::new(Self { engine: Mutex::new(engine), rate }))
    }

    /// The rate the samples come out at. Playback must match it.
    pub fn sample_rate(&self) -> u32 {
        self.rate
    }

    /// Says `text` in `voice_name` and hands back 24 kHz mono samples at the approved pace.
    /// Blocking: about a fifth of a second for a verse, so call it off the drawing thread.
    pub fn say(&self, text: String, voice_name: String) -> Result<Vec<f32>, BibleError> {
        // The names are capitalised ("Harper"), and a caller may not be: the lookup ignores case
        // rather than making every frontend remember which it is.
        let voice = bible_core::speak::Voice::by_name(&voice_name).ok_or_else(|| {
            BibleError::Failed { message: format!("there is no voice called {voice_name}") }
        })?;
        let mut engine = self
            .engine
            .lock()
            .map_err(|_| BibleError::Failed { message: "the voice was lost".to_string() })?;
        engine.say(&text, voice).map_err(|why| BibleError::Failed { message: why })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The list the phone downloads from must be the list `xtask` pins, or a phone would fetch a
    /// voice the Mac does not have.
    #[test]
    fn the_download_list_is_the_pinned_list() {
        let files = voice_files();
        assert_eq!(files.len(), 8);
        assert_eq!(files[0].name, "model.q8.gguf");
        assert!(files.iter().all(|f| f.sha256.len() == 64));
        assert!(voice_bundle_url().starts_with("https://github.com/trentster/lamplit/"));
        assert!(voice_total_bytes() > 60_000_000);
    }

    #[test]
    fn every_shipped_voice_is_offered_with_a_readable_name() {
        let voices = voices();
        assert_eq!(voices.len(), VOICES.len());
        let harper = voices.iter().find(|v| v.name == "Harper").expect("Harper");
        assert_eq!(harper.label, "Harper");
        assert!(harper.native_wpm > 100, "a voice at {} wpm is not a voice", harper.native_wpm);
        assert!(voices.iter().all(|v| v.accent == "US" || v.accent == "UK"));
    }

    #[test]
    fn an_empty_folder_is_not_an_installed_voice() {
        let empty = std::env::temp_dir().join("bible-rs-ffi-no-voice");
        let _ = std::fs::remove_dir_all(&empty);
        std::fs::create_dir_all(&empty).expect("a scratch directory");
        assert!(!voice_complete(empty.to_string_lossy().to_string()));
        assert!(Speech::load(empty.to_string_lossy().to_string()).is_err());
        let _ = std::fs::remove_dir_all(&empty);
    }

    /// Skipped unless a voice is on this machine: CI has no 66 MB download and should not grow one.
    #[test]
    fn the_phone_side_can_speak_through_the_shared_engine() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/speech");
        if !bundle::complete(&dir) {
            eprintln!("no voice on this machine, so nothing to say");
            return;
        }
        let speech = Speech::load(dir.to_string_lossy().to_string()).expect("the voice opens");
        assert!(speech.sample_rate() >= 16_000);
        let pcm = speech.say("The Lord is my shepherd.".to_string(), "russell".to_string()).expect("speech");
        assert!(!pcm.is_empty());
        assert!(pcm.iter().all(|s| s.is_finite()));
    }

    /// The announcement, then the verses in order, carrying their numbers — what the phone's player
    /// steps through.
    #[test]
    fn a_chapter_is_spoken_from_its_first_word_with_each_verse_marked() {
        let dir = std::env::temp_dir().join("bible-rs-ffi-spoken");
        let _ = std::fs::remove_dir_all(&dir);
        let bible = crate::Bible::new(dir.to_string_lossy().to_string()).expect("a bible");
        let john = crate::books().into_iter().find(|b| b.name == "John").expect("John");
        let index = u16::from(john.first_chapter) + 2; // John 3
        let said = bible.spoken_chapter("kjv".to_string(), index).expect("John 3");
        assert!(said.len() > 30, "John 3 came back as {} utterances", said.len());
        assert!(said[0].verse.is_none(), "the first utterance should be the announcement");
        assert!(said[0].text.contains("John"), "the announcement does not name the book: {}", said[0].text);
        let numbered: Vec<u8> = said.iter().filter_map(|u| u.verse).collect();
        assert_eq!(numbered.first().copied(), Some(1), "the reading does not start at verse 1");
        assert!(numbered.windows(2).all(|w| w[0] <= w[1]), "the verses are out of order");
        assert!(said.iter().all(|u| !u.text.trim().is_empty()), "an empty utterance would be silence");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unknown_voice_is_refused_by_name() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/speech");
        if !bundle::complete(&dir) {
            eprintln!("no voice on this machine, so nothing to say with");
            return;
        }
        let speech = Speech::load(dir.to_string_lossy().to_string()).expect("the voice opens");
        let refused = speech.say("hello".to_string(), "nobody".to_string());
        assert!(refused.is_err(), "an invented voice was allowed to speak");
    }
}
