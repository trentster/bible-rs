//! Fetches the voice exactly the way the app does, into whatever data directory is in force, and
//! reports what arrived.
//!
//! ```text
//! BIBLE_RS_HOME=/tmp/voice-test cargo run -p bible-rs --example fetch_voice
//! ```

use std::time::{Duration, Instant};

use bible_rs::read_aloud::install::{self, Status};

fn main() {
    let dir = install::dir();
    println!("voice directory: {}", dir.display());
    println!("already installed: {}", install::installed());
    if install::installed() {
        return;
    }
    let fetch = install::shared();
    let started = Instant::now();
    fetch.start();
    let mut last = 0u64;
    loop {
        match fetch.status() {
            Status::Fetching { done, of } => {
                // Report every ten per cent, so the line reads like a download rather than a stall.
                if done.saturating_sub(last) >= of / 10 {
                    println!(
                        "  {:>3}%  {:.1} MB of {:.1} MB  ({:.1} s)",
                        done * 100 / of.max(1),
                        done as f64 / 1e6,
                        of as f64 / 1e6,
                        started.elapsed().as_secs_f64()
                    );
                    last = done;
                }
            }
            Status::Ready => {
                println!("fetched in {:.1} s", started.elapsed().as_secs_f64());
                println!("installed: {}", install::installed());
                let mut names: Vec<String> = std::fs::read_dir(&dir)
                    .map(|entries| {
                        entries
                            .flatten()
                            .map(|e| {
                                format!(
                                    "{} {}",
                                    e.metadata().map(|m| m.len()).unwrap_or(0),
                                    e.file_name().to_string_lossy()
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                names.sort();
                for line in names {
                    println!("  {line}");
                }
                return;
            }
            Status::Failed(why) => {
                println!("failed after {:.1} s: {why}", started.elapsed().as_secs_f64());
                std::process::exit(1);
            }
            Status::Missing => {}
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}
