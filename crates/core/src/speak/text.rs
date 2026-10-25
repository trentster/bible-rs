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
    /// The verse this belongs to, so a session can say where it is; `None` for an announcement.
    pub verse: Option<u8>,
}

/// A breath where a sentence ends. Chosen against the audio, not against the model: the voice's own
/// trailing silence is trimmed first (`silence::trim_silence`), so this is the gap a listener hears.
pub const SENTENCE_PAUSE_MS: u32 = 260;
/// After the announcement, so "Hebrews, chapter seven" lands before the text starts.
pub const ANNOUNCE_PAUSE_MS: u32 = 450;
/// Where a sentence runs past what one breath of reading can hold, it is broken at a verse number
/// with a clause pause rather than a full stop. Rare — 1.4% of the King James sentences cross three
/// verses — but its longest runs 42 verses, and a bar that cannot move for two minutes is a bar that
/// is lying about where the narrator is.
pub const CLAUSE_PAUSE_MS: u32 = 110;
/// The most a single utterance carries: 60 words is about 24 seconds of reading.
pub const MAX_SENTENCE_WORDS: usize = 60;
pub const MAX_SENTENCE_VERSES: usize = 3;

/// A chapter as the reader hears it: the announcement, then every verse in order.
pub fn chapter(book: &str, number: u32, verses: &[(u32, String)]) -> Vec<Utterance> {
    let mut said = Vec::with_capacity(verses.len() + 1);
    said.push(Utterance {
        text: format!("{}, chapter {}", book, words(number as u64)),
        pause_ms: ANNOUNCE_PAUSE_MS,
        verse: None,
    });
    // Verses are not sentences. The KJV ends a verse with a semicolon or a colon wherever the
    // sentence carries on across the verse number, and a narrator who pauses at that number reads
    // one sentence as two. So verses are gathered until the sentence ends, and only then does the
    // breath fall.
    let mut sentence = String::new();
    let mut started_at: Option<u8> = None;
    let mut carried = 0usize;
    for (number, text) in verses {
        let spoken = speakable(text);
        if spoken.is_empty() {
            continue;
        }
        if !sentence.is_empty() {
            sentence.push(' ');
        }
        sentence.push_str(&spoken);
        started_at = started_at.or(Some(*number as u8));
        carried += 1;
        let running_on = runs_on(&sentence);
        let too_long = carried >= MAX_SENTENCE_VERSES
            || sentence.split_whitespace().count() >= MAX_SENTENCE_WORDS;
        if !running_on || too_long {
            said.push(Utterance {
                text: std::mem::take(&mut sentence),
                // A sentence cut short of its end gets a clause breath, not a full stop.
                pause_ms: if running_on { CLAUSE_PAUSE_MS } else { SENTENCE_PAUSE_MS },
                verse: started_at.take(),
            });
            carried = 0;
        }
    }
    if !sentence.is_empty() {
        said.push(Utterance { text: sentence, pause_ms: SENTENCE_PAUSE_MS, verse: started_at });
    }
    said
}

/// Whether the sentence carries on into the next verse. A colon, semicolon or comma at the end of a
/// verse is the text telling the narrator not to stop.
pub(crate) fn runs_on(text: &str) -> bool {
    let said = text
        .trim_end()
        .trim_end_matches(['"', '\'', '\u{201d}', '\u{2019}', ')', ']']);
    said.ends_with(',') || said.ends_with(';') || said.ends_with(':')
}

/// One verse, ready to be spoken: divine names in normal case, dashes turned into pauses, digits
/// read as words, and nothing a narrator would not say out loud.
pub fn speakable(text: &str) -> String {
    let mut working = text
        .replace(['\u{2014}', '\u{2013}'], ", ")
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
        20..=99 if value.is_multiple_of(10) => TENS[(value / 10 - 2) as usize].to_string(),
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
