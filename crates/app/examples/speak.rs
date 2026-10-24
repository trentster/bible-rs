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

    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/speech");
    let voice = dir.join("edwin.safetensors");
    let dir_str = dir.to_str().context("model path is not UTF-8")?.to_string();

    let started = Instant::now();
    let checkpoint = Checkpoint::locate(&dir_str, ResolveOptions { quant: Quant::Q80, weights: None })?;
    let mut tts = checkpoint.builder(Lang::En).device(DeviceKind::Cpu).add_voice("Edwin", &voice).build()?;
    checkpoint.register_voices(&mut tts);
    let load = started.elapsed().as_secs_f64();
    let rate = tts.sample_rate();

    let options = SpeechOptions::default().voice("Edwin");
    let synth = Instant::now();
    let mut first = None;
    let mut pcm = Vec::new();
    for chunk in tts.stream_with(text.trim(), &options)? {
        let chunk = chunk?;
        if first.is_none() {
            first = Some(synth.elapsed().as_secs_f64());
        }
        pcm.extend_from_slice(&chunk);
    }
    let work = synth.elapsed().as_secs_f64();
    // 0.80 of the model's own pace: 190 wpm is brisk for scripture, 153 reads like an audiobook.
    let speed: f32 = std::env::var("SPEAK_TEMPO").ok().and_then(|v| v.parse().ok()).unwrap_or(0.80);
    let stretched = bible_core::speak::stretch(&pcm, rate, speed);
    let pcm = stretched;
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
