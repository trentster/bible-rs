//! Says a list of words out loud, one after another, so a person can hear which ones the narrator
//! gets wrong and say so by number. Writes a WAV.
//!
//! ```text
//! cargo run --release -p bible-rs --example audition -- out.wav hath shalt saith spake
//! ```

use bible_core::speak::Voice;

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "audition.wav".to_string());
    let words: Vec<String> = args.collect();
    if words.is_empty() {
        eprintln!("usage: audition <out.wav> <word>...");
        return;
    }
    let voice = Voice::by_name("Spencer").expect("spencer");
    let mut all: Vec<f32> = Vec::new();
    let mut rate = 24_000u32;
    for (i, word) in words.iter().enumerate() {
        println!("{:>3}  {word}", i + 1);
        match bible_rs::read_aloud::render(word, voice) {
            Ok((pcm, r)) => {
                rate = r;
                all.extend_from_slice(&pcm);
                // A beat between words, so they do not run together.
                all.extend(std::iter::repeat_n(0.0, (rate as f32 * 0.6) as usize));
            }
            Err(e) => {
                eprintln!("{word}: {e}");
                return;
            }
        }
    }
    let mut file = std::io::BufWriter::new(std::fs::File::create(&out).expect("create"));
    let data = (all.len() * 2) as u32;
    use std::io::Write;
    let _ = file.write_all(b"RIFF");
    let _ = file.write_all(&(36 + data).to_le_bytes());
    let _ = file.write_all(b"WAVEfmt ");
    let _ = file.write_all(&16u32.to_le_bytes());
    let _ = file.write_all(&1u16.to_le_bytes());
    let _ = file.write_all(&1u16.to_le_bytes());
    let _ = file.write_all(&rate.to_le_bytes());
    let _ = file.write_all(&(rate * 2).to_le_bytes());
    let _ = file.write_all(&2u16.to_le_bytes());
    let _ = file.write_all(&16u16.to_le_bytes());
    let _ = file.write_all(b"data");
    let _ = file.write_all(&data.to_le_bytes());
    for sample in &all {
        let v = (sample.clamp(-1.0, 1.0) * 32767.0) as i16;
        let _ = file.write_all(&v.to_le_bytes());
    }
    println!("\nwrote {out}");
}
