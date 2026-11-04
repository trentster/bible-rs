//! Fetches the read-aloud voice into a fresh data folder and says what happened.
//!
//! ```text
//! BIBLE_RS_HOME=/tmp/fresh cargo run --release --example voice-check
//! ```
//!
//! This is the end-to-end check the blocked-network test cannot make: it needs the real download,
//! and with it whatever TLS stack that download uses.

use std::time::{Duration, Instant};

use bible_rs::read_aloud::install;

fn main() {
    let dir = install::dir();
    println!("data folder: {}", dir.display());
    println!("installed before: {}", install::installed());
    let i = install::shared();
    i.start();
    let started = Instant::now();
    loop {
        match i.status() {
            install::Status::Ready => {
                println!("READY in {:.1}s", started.elapsed().as_secs_f32());
                break;
            }
            install::Status::Failed(e) => {
                println!("FAILED after {:.1}s: {e}", started.elapsed().as_secs_f32());
                std::process::exit(1);
            }
            install::Status::Fetching { done, of } => {
                println!("fetching {done}/{of}");
                std::thread::sleep(Duration::from_millis(700));
            }
            install::Status::Missing => {
                std::thread::sleep(Duration::from_millis(200));
            }
        }
        if started.elapsed() > Duration::from_secs(600) {
            println!("TIMED OUT");
            std::process::exit(2);
        }
    }
    let complete = install::dir();
    let mut found = 0;
    if !complete.exists() {
        println!("no voice folder was created");
        std::process::exit(3);
    }
    for entry in std::fs::read_dir(&complete).expect("listing the voice folder") {
        let p = entry.expect("entry").path();
        if p.is_file() {
            found += 1;
            println!("  {} {} bytes", p.file_name().unwrap().to_string_lossy(), p.metadata().unwrap().len());
        }
    }
    println!("files in the folder: {found}");
    println!("installed() says: {}", install::installed());
}
