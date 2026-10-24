//! Checks the reading thread against a real chapter: it should speak verse by verse, obey pause,
//! resume and stop, and report where it is.
//!
//! ```text
//! cargo run --example read -- heb7-verses.txt
//! ```

use std::time::{Duration, Instant};

use bible_core::speak::{Session, Sleep, Voice};

fn main() {
    let path = std::env::args().nth(1).expect("usage: read <verses file>");
    let text = std::fs::read_to_string(&path).expect("read the verses");
    let mut verses = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        let digits: String = trimmed.chars().take_while(|c| c.is_ascii_digit()).collect();
        let rest = trimmed[digits.len()..].trim_start();
        if !digits.is_empty() && !rest.is_empty() {
            verses.push((digits.parse().unwrap_or(0), rest.to_string()));
        }
    }
    println!("{} verses", verses.len());

    let mut session = Session::new("Hebrews", 7, &verses, Sleep::Minutes(30));
    session.play();
    session.seek_to_verse(6);
    let voice = Voice::by_name("Spencer").unwrap();
    let started = Instant::now();
    let reading = bible_rs::read_aloud::Reading::start(session, voice);

    let mut seen = None;
    let mut paused_at = None;
    let mut resumed = false;
    while started.elapsed() < Duration::from_secs(14) {
        std::thread::sleep(Duration::from_millis(200));
        let state = reading.state();
        if state.verse != seen {
            seen = state.verse;
            println!(
                "{:5.1} s  verse {:?}  speaking {}  left {:?}",
                started.elapsed().as_secs_f32(),
                state.verse,
                state.speaking,
                state.remaining.map(|r| r.round())
            );
        }
        if paused_at.is_none() && started.elapsed() > Duration::from_secs(4) {
            reading.pause();
            paused_at = Some(started.elapsed());
            println!("paused at {:.1} s; verse should hold", paused_at.unwrap().as_secs_f32());
        }
        if paused_at.is_some() && !resumed && started.elapsed() > Duration::from_secs(6) {
            reading.resume();
            resumed = true;
            println!("resumed");
        }
        if started.elapsed() > Duration::from_secs(11) {
            reading.stop();
            let state = reading.state();
            println!("stopped: speaking {} finished {}", state.speaking, state.finished);
            break;
        }
    }
    let state = reading.state();
    if let Some(note) = state.note {
        println!("note: {note}");
        std::process::exit(1);
    }
    println!("ok");
}
