//! Turning scripture into something worth hearing.
//!
//! Reading aloud is not the same as reading. A narrator announces what is coming, breathes between
//! verses, and never says "colon". This module is that judgement, kept out of the audio path so it
//! can be tested exactly, and shared by every device.

/// One thing to say, and how long to be quiet afterwards.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Utterance {
    pub text: String,
    pub pause_ms: u32,
}

/// A breath between verses. Long enough to separate thoughts, short enough not to feel broken.
pub const VERSE_PAUSE_MS: u32 = 380;
/// After the announcement, so "Hebrews, chapter seven" lands before the text starts.
pub const ANNOUNCE_PAUSE_MS: u32 = 700;

/// A chapter as the reader hears it: the announcement, then every verse in order.
pub fn chapter(book: &str, number: u32, verses: &[(u32, String)]) -> Vec<Utterance> {
    let mut said = Vec::with_capacity(verses.len() + 1);
    said.push(Utterance {
        text: format!("{}, chapter {}", book, words(number as u64)),
        pause_ms: ANNOUNCE_PAUSE_MS,
    });
    for (_, text) in verses {
        let spoken = speakable(text);
        if spoken.is_empty() {
            continue;
        }
        said.push(Utterance { text: spoken, pause_ms: VERSE_PAUSE_MS });
    }
    said
}

/// One verse, ready to be spoken: divine names in normal case, dashes turned into pauses, digits
/// read as words, and nothing a narrator would not say out loud.
pub fn speakable(text: &str) -> String {
    let mut working = text
        .replace('\u{2014}', ", ")
        .replace('\u{2013}', ", ")
        .replace("&c.", "and so on")
        .replace("&c", "and so on")
        .replace("LORD", "Lord")
        .replace("GOD", "God")
        .replace("JEHOVAH", "Jehovah");
    working = working.trim_start_matches(|c: char| c.is_ascii_digit() || c == ' ').to_string();
    let mut said = String::with_capacity(working.len());
    for token in working.split_whitespace() {
        if !said.is_empty() {
            said.push(' ');
        }
        match spoken_number(token) {
            Some(number) => said.push_str(&number),
            None => said.push_str(token),
        }
    }
    said
}

/// A token that is only a number, or a number with punctuation attached, said as words.
fn spoken_number(token: &str) -> Option<String> {
    let head: String = token.chars().take_while(|c| c.is_ascii_digit()).collect();
    if head.is_empty() {
        return None;
    }
    let tail: String = token.chars().skip(head.len()).collect();
    let lower = tail.to_ascii_lowercase();
    let ordinal = ["st", "nd", "rd", "th"].iter().any(|ending| lower.starts_with(ending));
    let value: u64 = head.parse().ok()?;
    let rest = if ordinal { &tail[2..] } else { &tail[..] };
    if !rest.chars().all(|c| c.is_ascii_punctuation()) {
        return None;
    }
    let spoken = if ordinal { ordinal_words(value) } else { words(value) };
    Some(format!("{spoken}{rest}"))
}

/// Ordinals, for the rare "23rd" that turns up in a reference rather than a verse.
fn ordinal_words(value: u64) -> String {
    const ORDINALS: [&str; 20] = [
        "zeroth", "first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth",
        "ninth", "tenth", "eleventh", "twelfth", "thirteenth", "fourteenth", "fifteenth",
        "sixteenth", "seventeenth", "eighteenth", "nineteenth",
    ];
    const TENS: [&str; 8] = [
        "twentieth", "thirtieth", "fortieth", "fiftieth", "sixtieth", "seventieth", "eightieth",
        "ninetieth",
    ];
    match value {
        0..=19 => ORDINALS[value as usize].to_string(),
        20..=99 if value % 10 == 0 => TENS[(value / 10 - 2) as usize].to_string(),
        20..=99 => format!("{}-{}", words(value / 10 * 10), ordinal_words(value % 10)),
        other => words(other),
    }
}

/// Integer to English words, which is all a chapter or verse number ever needs.
pub fn words(value: u64) -> String {
    const UNITS: [&str; 20] = [
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
        "eleven", "twelve", "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen",
        "nineteen",
    ];
    const TENS: [&str; 8] =
        ["twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety"];
    match value {
        0..=19 => UNITS[value as usize].to_string(),
        20..=99 => {
            let tens = TENS[(value / 10 - 2) as usize];
            match value % 10 {
                0 => tens.to_string(),
                rest => format!("{tens}-{}", UNITS[rest as usize]),
            }
        }
        100..=999 => {
            let hundreds = format!("{} hundred", UNITS[(value / 100) as usize]);
            match value % 100 {
                0 => hundreds,
                rest => format!("{hundreds} {}", words(rest)),
            }
        }
        _ => value.to_string(),
    }
}
