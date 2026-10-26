//! Lists the words most likely to be mispronounced: words the old translations use far more than
//! modern English does, and the proper names.
//!
//! The reference for "modern English" is the WEB, which is bundled: a word common in KJV and rare or
//! absent in WEB is either archaic or a name, and both are where a narrator goes wrong.

use bible_core::books::ChapterIdx;
use std::collections::HashMap;

fn counts(id: &str) -> HashMap<String, u64> {
    let Ok(text) = bible_core::bundled::load(id) else {
        return HashMap::new();
    };
    let mut counts: HashMap<String, u64> = HashMap::new();
    for chapter in ChapterIdx::all() {
        for verse in text.verses_in(chapter) {
            for word in verse.text.split(|c: char| !c.is_alphabetic()) {
                if word.is_empty() {
                    continue;
                }
                *counts.entry(word.to_lowercase()).or_default() += 1;
            }
        }
    }
    counts
}

fn main() {
    let kjv = counts("kjv");
    let web = counts("web");
    // Ordinary English that needs no help; the point is to catch the rest.
    let common = [
        "the", "and", "of", "to", "that", "in", "he", "shall", "unto", "for", "i", "his", "a", "they", "be",
        "him", "not", "them", "it", "with", "all", "thou", "thy", "thee", "ye", "as", "was", "their",
        "which", "but", "is", "me", "my", "when", "this", "are", "so", "out", "up", "into", "have", "will",
        "from", "by", "her", "she", "we", "your", "us", "who", "upon", "at", "there", "then", "if", "now",
        "also", "or", "one", "no", "may", "than", "were", "an", "did",
    ];
    let mut archaic: Vec<(&String, &u64)> = kjv
        .iter()
        .filter(|(word, count)| {
            **count >= 40
                && !common.contains(&word.as_str())
                && word.chars().all(|c| c.is_alphabetic())
                && web.get(*word).copied().unwrap_or(0) * 4 < **count
        })
        .collect();
    archaic.sort_by(|a, b| b.1.cmp(a.1));
    println!("== said far more in KJV than in WEB (candidate archaic words)");
    for (word, count) in archaic.iter().take(40) {
        println!("{count:>5}  {word}");
    }
    println!("\n== totals: KJV {} distinct, WEB {} distinct", kjv.len(), web.len());

    // Proper names: capitalised in the middle of a sentence, which is what a name looks like in this
    // text and what "The" or "And" never do in the middle of one.
    let Ok(text) = bible_core::bundled::load("kjv") else {
        return;
    };
    let mut names: HashMap<String, u64> = HashMap::new();
    for chapter in ChapterIdx::all() {
        for verse in text.verses_in(chapter) {
            let mut after_stop = true;
            for raw in verse.text.split_whitespace() {
                let word: String = raw.chars().filter(|c| c.is_alphabetic() || *c == '\'').collect();
                let opens = after_stop;
                after_stop = raw.ends_with(['.', '!', '?', ':', ';', ',']);
                if word.is_empty() {
                    continue;
                }
                let mut chars = word.chars();
                let capital = chars.next().is_some_and(|c| c.is_uppercase());
                if capital && !opens && word.chars().count() > 2 {
                    *names.entry(word).or_default() += 1;
                }
            }
        }
    }
    let mut ranked: Vec<(&String, &u64)> = names.iter().filter(|(_, n)| **n >= 8).collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1));
    println!("\n== proper names, most used first ({} with 8 or more)", ranked.len());
    for (name, count) in ranked.iter().take(60) {
        print!("{name} {count} · ");
    }
    println!();
}
