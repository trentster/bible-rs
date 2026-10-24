//! Spike: read a chapter out loud, with no UI and no change to the app.
//!
//! ```text
//! cargo run --release --example speak --features speak -- heb7.txt
//! cargo run --release --example speak --features speak -- heb7.txt --silent   # time it, stay quiet
//! ```

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::Context as _;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ptts::checkpoint::{Checkpoint, ResolveOptions};
use ptts::preprocess::Lang;
use ptts::synth::{DeviceKind, Quant, SpeechOptions};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let text_path = args.next().context("usage: speak <text file> [--silent]")?;
    let silent = args.next().as_deref() == Some("--silent");
    let rest: Vec<String> = std::env::args().collect();
    let out_wav = rest.iter().position(|a| a == "--out").and_then(|i| rest.get(i + 1)).cloned();
    let text = std::fs::read_to_string(&text_path).with_context(|| text_path.clone())?;

    // Lines that start with a verse number become the chapter script: announcement, then verses
    // with a breath between them. Anything else is spoken as one utterance.
    let mut verses = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        let digits: String = trimmed.chars().take_while(|c| c.is_ascii_digit()).collect();
        let rest = trimmed[digits.len()..].trim_start();
        if !digits.is_empty() && !rest.is_empty() {
            verses.push((digits.parse().unwrap_or(0), rest.to_string()));
        }
    }
    let book = std::env::var("SPEAK_BOOK").unwrap_or_else(|_| "Hebrews".to_string());
    let number: u32 = std::env::var("SPEAK_CHAPTER").ok().and_then(|v| v.parse().ok()).unwrap_or(7);
    let script = if verses.is_empty() {
        vec![bible_core::speak::Utterance { text: text.trim().to_string(), pause_ms: 0 }]
    } else {
        bible_core::speak::chapter(&book, number, &verses)
    };

    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/speech");
    let wanted = std::env::var("SPEAK_VOICE").unwrap_or_else(|_| "Russell".to_string());
    let voice = bible_core::speak::Voice::by_name(&wanted)
        .unwrap_or(bible_core::speak::VOICES[0]);
    let voice_path = dir.join(format!("{}.safetensors", voice.name.to_ascii_lowercase()));
    let dir_str = dir.to_str().context("model path is not UTF-8")?.to_string();

    let started = Instant::now();
    let checkpoint = Checkpoint::locate(&dir_str, ResolveOptions { quant: Quant::Q80, weights: None })?;
    let mut tts = checkpoint
        .builder(Lang::En)
        .device(DeviceKind::Cpu)
        .add_voice(voice.name, &voice_path)
        .build()?;
    checkpoint.register_voices(&mut tts);
    let load = started.elapsed().as_secs_f64();
    let rate = tts.sample_rate();

    let options = SpeechOptions::default().voice(voice.name);
    // Each voice has its own measured rate, so switching voice does not change the pace.
    let speed: f32 = std::env::var("SPEAK_TEMPO")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| voice.tempo());
    let synth = Instant::now();
    let mut first = None;
    let mut pcm: Vec<f32> = Vec::new();
    for utterance in &script {
        let mut spoken = Vec::new();
        for chunk in tts.stream_with(&utterance.text, &options)? {
            let chunk = chunk?;
            if first.is_none() {
                first = Some(synth.elapsed().as_secs_f64());
            }
            spoken.extend_from_slice(&chunk);
        }
        pcm.extend_from_slice(&bible_core::speak::stretch(&spoken, rate, speed));
        let silence = (rate as u64 * utterance.pause_ms as u64 / 1000) as usize;
        pcm.extend(std::iter::repeat(0.0).take(silence));
    }
    let work = synth.elapsed().as_secs_f64();
    let audio = pcm.len() as f64 / rate as f64;
    println!("model ready      {:.0} ms", load * 1000.0);
    println!("first audio      {:.0} ms", first.unwrap_or(f64::NAN) * 1000.0);
    println!("synthesised      {audio:.1} s of speech in {work:.2} s ({:.1}x realtime)", audio / work);
    if let Some(path) = &out_wav {
        ptts::wav::write_wav_file(path, &pcm, rate)?;
        println!("wrote            {path}");
        return Ok(());
    }
    if silent {
        return Ok(());
    }
    play(&pcm, rate)
}

/// Plays 24 kHz mono f32 through the default output device, resampling to whatever it wants.
fn play(pcm: &[f32], rate: u32) -> anyhow::Result<()> {
    let host = cpal::default_host();
    let device = host.default_output_device().context("no output device")?;
    let config = device.default_output_config()?;
    let out_rate = config.sample_rate().0;
    let channels = config.channels() as usize;
    println!(
        "device           {} @ {out_rate} Hz, {channels} ch, {:?}",
        device.name()?,
        config.sample_format()
    );

    let step = rate as f64 / out_rate as f64;
    let frames = (pcm.len() as f64 / step) as usize;
    let mut buffer = Vec::with_capacity(frames * channels);
    for frame in 0..frames {
        let src = frame as f64 * step;
        let low = src.floor() as usize;
        let frac = (src - low as f64) as f32;
        let a = pcm.get(low).copied().unwrap_or(0.0);
        let next = pcm.get(low + 1).copied().unwrap_or(a);
        let sample = a + (next - a) * frac;
        buffer.extend(std::iter::repeat(sample).take(channels));
    }

    let shared = Arc::new(Mutex::new((buffer, 0usize)));
    let filler = Arc::clone(&shared);
    let failed = |e| eprintln!("stream error: {e}");
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_output_stream(
            &config.into(),
            move |out: &mut [f32], _| {
                let mut guard = filler.lock().unwrap();
                let (buf, at) = &mut *guard;
                for slot in out.iter_mut() {
                    *slot = match buf.get(*at) {
                        Some(sample) => {
                            *at += 1;
                            *sample
                        }
                        None => 0.0,
                    };
                }
            },
            failed,
            None,
        )?,
        other => anyhow::bail!("the spike only handles f32 output, this device wants {other:?}"),
    };
    stream.play()?;
    loop {
        std::thread::sleep(std::time::Duration::from_millis(100));
        let guard = shared.lock().unwrap();
        if guard.1 >= guard.0.len() {
            break;
        }
    }
    Ok(())
}
