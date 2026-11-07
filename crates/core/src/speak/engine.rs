//! Making the voice say something: the model, loaded once, and the pipeline that turns text into
//! 24 kHz mono audio at the pace each voice is calibrated to.
//!
//! This is the pipeline the Mac runs, moved here so a phone can run it too. The two frontends still
//! differ where they must — the Mac plays through CoreAudio, the phone through Apple's own audio
//! engine — but the words said, the silence trimmed, the pace applied and the gaps capped are
//! decided in one place, for both.
//!
//! It is behind the `synth` feature because it pulls in the speech model's runtime, which the web
//! build and the plain reader have no use for.

use std::path::{Path, PathBuf};

use ptts::checkpoint::{Checkpoint, ResolveOptions};
use ptts::preprocess::Lang;
use ptts::synth::{DeviceKind, Quant, SpeechOptions, Synth};

use crate::speak::{VOICES, Voice, bundle, cap_gaps, stretch, trim_silence};

/// The model and the voices, open and ready to speak.
pub struct Speech {
    tts: Synth,
    rate: u32,
}

impl Speech {
    /// Opens the model in `dir`. Fails with words a reader can act on when the voice is not there
    /// or cannot start, because that message is what the app shows.
    pub fn open(dir: &Path) -> Result<Self, String> {
        let dir_str = dir.to_str().ok_or("the voice folder's path is not UTF-8")?.to_string();
        let checkpoint = Checkpoint::locate(&dir_str, ResolveOptions { quant: Quant::Q80, weights: None })
            .map_err(|e| format!("the voice model could not be opened: {e}"))?;
        let mut builder = checkpoint.builder(Lang::En).device(DeviceKind::Cpu);
        for voice in VOICES {
            builder = builder.add_voice(voice.name, voice_file(dir, voice));
        }
        let mut tts = builder.build().map_err(|e| format!("the voice could not start: {e}"))?;
        checkpoint.register_voices(&mut tts);
        let rate = tts.sample_rate();
        Ok(Self { tts, rate })
    }

    /// The rate the samples come out at, which playback must match.
    pub fn sample_rate(&self) -> u32 {
        self.rate
    }

    /// `text` in `voice`, at the approved pace. Blocking: a verse takes about a fifth of a second
    /// to make, so callers run it off their drawing thread.
    pub fn say(&mut self, text: &str, voice: Voice) -> Result<Vec<f32>, String> {
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
        let trimmed = trim_silence(&pcm, self.rate);
        let stretched = stretch(&trimmed, self.rate, voice.tempo());
        // The voice also pauses at punctuation for up to a second, three times the breath we ask for
        // between sentences, which stops the reading dead mid-paragraph. Shorten those to a
        // narrator's pause so a passage runs at one pace.
        Ok(cap_gaps(&stretched, self.rate))
    }
}

fn voice_file(dir: &Path, voice: Voice) -> PathBuf {
    dir.join(bundle::file_for(voice.name))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The only test that opens the real model, and it is skipped unless a voice is on this machine:
    /// CI has no 66 MB download and should not grow one.
    fn installed_dir() -> Option<PathBuf> {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/speech");
        bundle::complete(&dir).then_some(dir)
    }

    #[test]
    fn a_verse_comes_out_as_audio_at_the_model_rate() {
        let Some(dir) = installed_dir() else {
            eprintln!("no voice on this machine, so nothing to say");
            return;
        };
        let mut speech = Speech::open(&dir).expect("the voice opens");
        let rate = speech.sample_rate();
        assert!(rate >= 16_000, "a voice at {rate} Hz is not a voice");
        let voice = VOICES[0];
        let pcm = speech.say("In the beginning God created the heaven and the earth.", voice)
            .expect("the voice speaks");
        // A sentence of nine words cannot be a tenth of a second, nor a minute.
        let seconds = pcm.len() as f32 / rate as f32;
        assert!((1.0..20.0).contains(&seconds), "one sentence came out as {seconds} seconds");
        assert!(pcm.iter().all(|s| s.is_finite()), "the samples are not all finite");
    }

    #[test]
    fn a_folder_without_a_voice_fails_with_words_a_reader_can_use() {
        let empty = std::env::temp_dir().join("bible-rs-no-voice-at-all");
        let _ = std::fs::remove_dir_all(&empty);
        std::fs::create_dir_all(&empty).expect("a scratch directory");
        match Speech::open(&empty) {
            Ok(_) => panic!("an empty folder opened as a voice"),
            Err(why) => assert!(why.contains("voice"), "unhelpful message: {why}"),
        }
        let _ = std::fs::remove_dir_all(&empty);
    }
}
