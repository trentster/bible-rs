//! Checks the reading thread against a real chapter: it should speak verse by verse, obey pause,
//! resume and stop, and report where it is.
//!
//! ```text
//! cargo run --example read -- heb7-verses.txt
//! ```
//!
//! `READ_ROLL_ON=1` gives the reading a way into the next chapter, and `READ_SLEEP_SECONDS` arms the
//! timer: together they are how the carry-on-into-the-next-chapter is proved rather than assumed.

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
    // `READ_LIMIT` shortens the chapter, so a roll-over can be reached in a few seconds.
    if let Ok(limit) = std::env::var("READ_LIMIT") {
        if let Ok(limit) = limit.parse::<usize>() {
            verses.truncate(limit.max(1));
        }
    }
    println!("{} verses", verses.len());

    // Untimed, as the application opens a reading: a timer arrives through `arm_for` below. Leaving a
    // 30-minute timer baked in here made the roll-over test pass for the wrong reason.
    let mut session = Session::new("Hebrews", 7, &verses, Sleep::default());
    session.play();
    session.seek_to_verse(6);
    let voice = Voice::by_name("Spencer").unwrap();
    let started = Instant::now();
    let at =
        bible_core::books::ChapterIdx::new(bible_core::books::BookId::from_code("HEB").expect("hebrews"), 7)
            .expect("hebrews 7");
    // With a timer running, the reading carries on into the next chapter. Without one asked for here,
    // it stops at the end of the chapter as it always did.
    let next: Option<bible_rs::read_aloud::NextChapter> = if std::env::var("READ_ROLL_ON").is_ok() {
        let kjv = std::sync::Arc::new(bible_core::bundled::load("kjv").expect("kjv"));
        Some(std::sync::Arc::new(move |after| {
            let (next_at, verses) = bible_rs::read_aloud::next_chapter_text(&kjv, after)?;
            let mut session =
                Session::new(next_at.book().name(), next_at.chapter() as u16, &verses, Sleep::default());
            session.play();
            Some((next_at, session))
        }))
    } else {
        None
    };
    // `READ_READY_PATH=1` does what the application does: open the reading paused, pick a timer while
    // it is still waiting, then press play. Opening and arming in a different order is how a bug hid
    // from the first test.
    let reading = if std::env::var("READ_READY_PATH").is_ok() {
        let reading = bible_rs::read_aloud::Reading::open(session, voice, at, next);
        reading.choose(Sleep::Minutes(2));
        reading.resume();
        reading
    } else {
        let reading = bible_rs::read_aloud::Reading::start(session, voice, at, next);
        if let Ok(seconds) = std::env::var("READ_SLEEP_SECONDS") {
            if let Ok(seconds) = seconds.parse::<u64>() {
                reading.arm_for(seconds);
                println!("timer armed for {} s on the clock", seconds);
            }
        }
        reading
    };
    // The timer runs on the clock, so a short one proves the deadline ends the reading where it is.
    if let Ok(seconds) = std::env::var("READ_SLEEP_SECONDS") {
        if let Ok(seconds) = seconds.parse::<u64>() {
            reading.arm_for(seconds);
            println!("timer armed for {seconds} s on the clock");
        }
    }

    let mut seen = None;
    let mut paused_at = None;
    let mut resumed = false;
    let watch: u64 = std::env::var("READ_SECONDS").ok().and_then(|v| v.parse().ok()).unwrap_or(14);
    while started.elapsed() < Duration::from_secs(watch) {
        std::thread::sleep(Duration::from_millis(200));
        let state = reading.state();
        if state.finished && seen.is_some() {
            println!("reading ended after {:.1} s", started.elapsed().as_secs_f32());
            break;
        }
        if state.verse != seen {
            seen = state.verse;
            println!(
                "{:5.1} s  {:?} verse {:?}  speaking {}  left {:?}",
                started.elapsed().as_secs_f32(),
                state.chapter.map(|c| format!("{} {}", c.book().name(), c.chapter())),
                state.verse,
                state.speaking,
                reading.left().map(|r| r.round())
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
        let until_stop: u64 =
            std::env::var("READ_STOP_SECONDS").ok().and_then(|v| v.parse().ok()).unwrap_or(11);
        if started.elapsed() > Duration::from_secs(until_stop) {
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
