//! Counts how often a set of words appears in each bundled translation, so pronunciation work is
//! aimed at words that actually occur — in every translation the app ships, not just one.

use bible_core::books::ChapterIdx;
use std::collections::HashMap;

const WATCH: &[&str] = &[
    "ye",
    "yea",
    "thee",
    "thou",
    "thy",
    "thine",
    "hath",
    "saith",
    "doth",
    "dost",
    "shew",
    "shewed",
    "shewbread",
    "unto",
    "verily",
    "brethren",
    "wist",
    "wot",
];

fn main() {
    for id in bible_core::bundled::IDS {
        let Ok(text) = bible_core::bundled::load(id) else {
            continue;
        };
        let mut counts: HashMap<String, u64> = HashMap::new();
        let mut total = 0u64;
        for chapter in ChapterIdx::all() {
            for verse in text.verses_in(chapter) {
                for word in verse.text.split(|c: char| !c.is_alphabetic()) {
                    let w = word.to_lowercase();
                    if w.is_empty() {
                        continue;
                    }
                    *counts.entry(w).or_default() += 1;
                    total += 1;
                }
            }
        }
        let said: Vec<String> = WATCH
            .iter()
            .map(|w| format!("{w} {}", counts.get(*w).copied().unwrap_or(0)))
            .filter(|s| !s.ends_with(" 0"))
            .collect();
        println!("{id:4} {:>7} words: {}", total, said.join(", "));
    }
}
