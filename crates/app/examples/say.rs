//! Checks the Settings sample through this crate's own read-aloud module — model lookup, synthesis
//! and tempo — without opening the app:
//!
//! ```text
//! cargo run --example say -- Spencer
//! ```

use std::time::Instant;

use bible_core::speak::Voice;

fn main() {
    let wanted = std::env::args().nth(1).unwrap_or_else(|| "Spencer".to_string());
    let Some(voice) = Voice::by_name(&wanted) else {
        eprintln!("unknown voice {wanted:?}; try Russell, Spencer, Harper or Freya");
        std::process::exit(2);
    };
    let started = Instant::now();
    match bible_rs::read_aloud::sample(voice) {
        Ok(pcm) => {
            let seconds = pcm.len() as f64 / 24_000.0;
            println!(
                "{:<8} {seconds:5.2} s of audio in {:5.2} s",
                voice.name,
                started.elapsed().as_secs_f64()
            );
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
