//! Read aloud: the on-device voice.
//!
//! The words live in `bible_core::speak`; this module owns the sound — the model, the audio device
//! and the tempo the reader approved. Nothing here touches the network: the weights are bundled
//! beside the app and every sample is made on this machine.

mod player;

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

fn voice_path(dir: &PathBuf, voice: Voice) -> PathBuf {
    dir.join(format!("{}.safetensors", voice.name.to_ascii_lowercase()))
}

/// Loads the model once, with every shipped voice registered, and keeps the samples it has made.
struct Engine {
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
    let options = SpeechOptions::default().voice(voice.name);
    let mut pcm = Vec::new();
    let stream = engine
        .tts
        .stream_with(SAMPLE, &options)
        .map_err(|e| format!("the voice could not speak: {e}"))?;
    for chunk in stream {
        pcm.extend_from_slice(&chunk.map_err(|e| format!("the voice stopped early: {e}"))?);
    }
    let pcm = Arc::new(bible_core::speak::stretch(&pcm, engine.rate, voice.tempo()));
    engine.samples.insert(voice.name, pcm.clone());
    Ok(pcm)
}

/// Says the sample through the default output device, off the UI thread.
pub fn play_sample(voice: Voice) {
    std::thread::spawn(move || match sample(voice) {
        Ok(pcm) => {
            if let Err(e) = player::play(&pcm, 24_000) {
                eprintln!("read aloud: {e}");
            }
        }
        Err(e) => eprintln!("read aloud: {e}"),
    });
}

/// Plays 24 kHz mono audio through the default output device, blocking until it has. The Settings
/// button uses `play_sample`; this exists so a check can prove the device path without a mouse.
pub fn play_for_test(pcm: &[f32]) -> Result<(), String> {
    player::play(pcm, 24_000)
}
