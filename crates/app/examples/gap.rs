//! Builds the audio for a real chapter exactly as the reading thread does, and writes it to a WAV so
//! a change in the pauses can be heard before it is shipped.
//!
//!     cargo run -q -p bible-rs --example gap
//!
//! Two versions are rendered: what shipped before the voice's own silence was trimmed, and what ships
//! now. Same text, same voice, same pace — only the pauses differ.

use ptts::checkpoint::{Checkpoint, ResolveOptions};
use ptts::preprocess::Lang;
use ptts::synth::{DeviceKind, Quant, SpeechOptions};

/// The level of a 10 ms block below which it counts as silence, in full scale.
const QUIET: f32 = 0.004;
/// The pause that shipped before: a breath at every verse, on top of the voice's own trailing silence.
const OLD_PAUSE_MS: u32 = 380;

fn main() {
    let translation = bible_core::bundled::load("kjv").expect("kjv");
    // `BIBLE_RS_GAP="MAT 15"` and `BIBLE_RS_GAP_LAST=4` pick what to render.
    let spec = std::env::var("BIBLE_RS_GAP").unwrap_or_else(|_| "MAT 4".to_string());
    let (code, number) = spec.split_once(' ').unwrap_or(("MAT", "4"));
    let chapter = bible_core::books::ChapterIdx::new(
        bible_core::books::BookId::from_code(code).expect("book"),
        number.trim().parse().expect("chapter"),
    )
    .expect("chapter");
    let last: u32 = std::env::var("BIBLE_RS_GAP_LAST").ok().and_then(|v| v.parse().ok()).unwrap_or(8);
    let verses: Vec<(u32, String)> = translation
        .verses_in(chapter)
        .filter(|v| v.number as u32 <= last)
        .map(|v| (v.number as u32, v.text.to_string()))
        .collect();
    let said = bible_core::speak::chapter("Matthew", 4, &verses);

    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/speech");
    let checkpoint =
        Checkpoint::locate(dir.to_str().unwrap(), ResolveOptions { quant: Quant::Q80, weights: None })
            .expect("model");
    let voice = bible_core::speak::VOICES[0];
    let mut tts = checkpoint
        .builder(Lang::En)
        .device(DeviceKind::Cpu)
        .add_voice(voice.name, dir.join(format!("{}.safetensors", voice.name.to_ascii_lowercase())))
        .build()
        .expect("voice");
    checkpoint.register_voices(&mut tts);
    let rate = tts.sample_rate();

    // The voice's work is the same either way; only the trimming and the pauses differ.
    let spoken: Vec<Vec<f32>> = said
        .iter()
        .take(8)
        .map(|u| {
            let options = SpeechOptions::default().voice(voice.name);
            let mut pcm = Vec::new();
            for chunk in tts.stream_with(&u.text, &options).expect("speak") {
                pcm.extend_from_slice(&chunk.expect("chunk"));
            }
            pcm
        })
        .collect();

    let mut before = Vec::new();
    let mut after = Vec::new();
    for (u, pcm) in said.iter().zip(&spoken) {
        let stretched_raw = bible_core::speak::stretch(pcm, rate, voice.tempo());
        before.extend_from_slice(&stretched_raw);
        before.extend(std::iter::repeat_n(0.0, (OLD_PAUSE_MS as usize * rate as usize) / 1000));

        let trimmed = bible_core::speak::trim_silence(pcm, rate);
        let stretched = bible_core::speak::stretch(&trimmed, rate, voice.tempo());
        after.extend_from_slice(&bible_core::speak::cap_gaps(&stretched, rate));
        after.extend(std::iter::repeat_n(0.0, (u.pause_ms as usize * rate as usize) / 1000));
    }

    for (name, stream) in [("before", &before), ("after", &after)] {
        let path = format!("/tmp/matthew4-{name}.wav");
        write_wav(&path, stream, rate);
        let found = gaps(stream, rate);
        println!(
            "{name:>6}: {:.1} s, {} pauses, median {:.0} ms, longest {:.0} ms — {path}",
            stream.len() as f64 / rate as f64,
            found.len(),
            median(&found),
            found.iter().cloned().fold(0.0, f64::max),
        );
    }
    println!(
        "\nthis build: {:?} ms at a sentence end, {:?} ms where a long sentence is cut",
        bible_core::speak::SENTENCE_PAUSE_MS,
        bible_core::speak::CLAUSE_PAUSE_MS
    );
    for u in said.iter().take(3) {
        let start: String = u.text.chars().take(54).collect();
        println!("  verse {:>7?} pause {:>3} ms — {start}", u.verse, u.pause_ms);
    }
}

fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    sorted[sorted.len() / 2]
}

fn gaps(stream: &[f32], rate: u32) -> Vec<f64> {
    let block = (rate as usize / 100).max(1);
    let n = stream.len() / block;
    let mut found = Vec::new();
    let mut quiet = 0;
    for b in 0..n {
        let lo = b * block;
        let level = (stream[lo..lo + block].iter().map(|s| s * s).sum::<f32>() / block as f32).sqrt();
        if level < QUIET {
            quiet += 1;
        } else {
            if quiet >= 15 {
                // 150 ms or more of quiet is a pause, not the tail of a syllable.
                found.push(quiet as f64 * 10.0);
            }
            quiet = 0;
        }
    }
    found
}

fn write_wav(path: &str, pcm: &[f32], rate: u32) {
    use std::io::Write;
    let mut file = std::fs::File::create(path).expect("wav");
    let data_len = (pcm.len() * 2) as u32;
    let mut header = Vec::new();
    header.extend(b"RIFF");
    header.extend((36 + data_len).to_le_bytes());
    header.extend(b"WAVEfmt ");
    header.extend(16u32.to_le_bytes());
    header.extend(1u16.to_le_bytes()); // PCM
    header.extend(1u16.to_le_bytes()); // mono
    header.extend(rate.to_le_bytes());
    header.extend((rate * 2).to_le_bytes());
    header.extend(2u16.to_le_bytes());
    header.extend(16u16.to_le_bytes());
    header.extend(b"data");
    header.extend(data_len.to_le_bytes());
    file.write_all(&header).expect("header");
    let samples: Vec<i16> = pcm.iter().map(|s| (s.clamp(-1.0, 1.0) * 32767.0) as i16).collect();
    let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    file.write_all(&bytes).expect("samples");
}
