//! Converting Open English WordNet into the plain text the app reads.
//!
//! The 2025 edition ships as one JSON file of lexical entries per letter, and one of synsets per
//! part of speech. An entry names its senses; a sense names the synset holding the definition. The
//! conversion joins those, keeps the first few senses of each word, and writes the same
//! tab-separated lexicon format Easton's uses — plus the table of irregular forms WordNet carries,
//! so "went" and "feet" reach "go" and "foot".

use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

/// The senses kept per word: a reader asking what a word means wants the plain meaning, not every
/// shade of it. WordNet lists senses most common first.
const SENSES_PER_WORD: usize = 3;

/// The longest definition kept, so one sense stays one short line in the popover.
const LONGEST_DEFINITION: usize = 150;

/// How WordNet spells a part of speech, in the order a dictionary presents them.
fn part_of_speech(letter: &str) -> Option<&'static str> {
    Some(match letter {
        "n" => "n.",
        "v" => "v.",
        "a" | "s" => "adj.",
        "r" => "adv.",
        _ => return None,
    })
}

fn rank(letter: &str) -> usize {
    match letter {
        "n" => 0,
        "v" => 1,
        "a" | "s" => 2,
        "r" => 3,
        _ => 4,
    }
}

/// A definition cut to one line, at a word boundary where it can be.
fn shorten(definition: &str) -> String {
    let text: String = definition.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= LONGEST_DEFINITION {
        return text;
    }
    let cut: String = text.chars().take(LONGEST_DEFINITION).collect();
    match cut.rfind(' ') {
        Some(at) => format!("{}…", &cut[..at]),
        None => format!("{cut}…"),
    }
}

/// Reads the whole edition and returns `(lexicon, forms)` as the texts to write.
pub fn convert(zip: &Path) -> Result<(String, String)> {
    let file = std::fs::File::open(zip).with_context(|| format!("opening {}", zip.display()))?;
    let mut archive = zip::ZipArchive::new(file)?;
    let names: Vec<String> = archive.file_names().flatten().map(|n| n.to_string()).collect();

    // Definitions first: a sense names its synset, and the synset holds the words.
    let mut definitions: HashMap<String, String> = HashMap::new();
    for name in names.iter().filter(|n| {
        n.ends_with(".json")
            && (n.starts_with("noun.")
                || n.starts_with("verb.")
                || n.starts_with("adj.")
                || n.starts_with("adv."))
    }) {
        let text = read(&mut archive, name)?;
        let synsets: Value = serde_json::from_str(&text).with_context(|| name.clone())?;
        if let Some(map) = synsets.as_object() {
            for (id, synset) in map {
                if let Some(definition) =
                    synset.get("definition").and_then(|d| d.get(0)).and_then(Value::as_str)
                {
                    definitions.entry(id.clone()).or_insert_with(|| shorten(definition));
                }
            }
        }
    }

    // Then the words, in alphabetical order so the file is reproducible.
    let mut entries: Vec<String> =
        names.iter().filter(|n| n.starts_with("entries-") && n.ends_with(".json")).cloned().collect();
    entries.sort();
    let mut words: Vec<(String, Vec<(u8, String)>)> = Vec::new();
    let mut forms: HashMap<String, String> = HashMap::new();
    for name in &entries {
        let text = read(&mut archive, name)?;
        let letters: Value = serde_json::from_str(&text).with_context(|| name.clone())?;
        let Some(map) = letters.as_object() else { continue };
        for (lemma, by_pos) in map {
            let Some(by_pos) = by_pos.as_object() else { continue };
            let mut senses: Vec<(u8, String)> = Vec::new();
            for (pos, details) in by_pos {
                let Some(label) = part_of_speech(pos) else { continue };
                if let Some(list) = details.get("form").and_then(Value::as_array) {
                    for form in list.iter().filter_map(Value::as_str) {
                        let form = form.trim().to_lowercase();
                        if form != lemma.to_lowercase() && !form.is_empty() {
                            forms.entry(form).or_insert_with(|| lemma.clone());
                        }
                    }
                }
                if let Some(list) = details.get("sense").and_then(Value::as_array) {
                    for sense in list {
                        let Some(id) = sense.get("synset").and_then(Value::as_str) else { continue };
                        let Some(definition) = definitions.get(id) else { continue };
                        senses.push((rank(pos) as u8, format!("{label} {definition}")));
                    }
                }
            }
            if senses.is_empty() {
                continue;
            }
            // The part of speech with the most senses is the one the word is mostly used as, so a
            // reader asking about "go" is shown the verb before the noun.
            senses.sort_by_key(|(order, _)| *order);
            let mut counts: HashMap<u8, usize> = HashMap::new();
            for (order, _) in &senses {
                *counts.entry(*order).or_default() += 1;
            }
            senses.sort_by(|a, b| counts[&b.0].cmp(&counts[&a.0]).then(a.0.cmp(&b.0)));
            words.push((lemma.clone(), senses));
        }
    }
    words.sort_by(|a, b| a.0.cmp(&b.0));

    let mut lexicon = String::from(
        "#bible-rs-lexicon v1\n#name\tOpen English WordNet (2025 edition)\n#licence\tCC BY 4.0 — \
         derived from Princeton WordNet under the WordNet License; attribute Princeton WordNet and \
         the Open English WordNet team\n",
    );
    for (lemma, senses) in &words {
        let mut lines: Vec<String> = Vec::new();
        for (_, line) in senses {
            if lines.len() == SENSES_PER_WORD {
                break;
            }
            if !lines.iter().any(|kept| kept == line) {
                lines.push(line.clone());
            }
        }
        if lines.is_empty() {
            continue;
        }
        let headword = lemma.replace('\t', " ");
        lexicon.push_str(&format!("wordnet\t{headword}\t{}\n", lines.join("\\n")));
    }

    let mut sorted_forms: Vec<(&String, &String)> = forms.iter().collect();
    sorted_forms.sort();
    let mut form_text = String::from("#bible-rs-forms v1\n");
    for (form, lemma) in sorted_forms {
        form_text.push_str(&format!("{form}\t{lemma}\n"));
    }
    Ok((lexicon, form_text))
}

fn read(archive: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Result<String> {
    let mut f = archive.by_name(name).with_context(|| name.to_string())?;
    let mut text = String::new();
    std::io::Read::read_to_string(&mut f, &mut text).with_context(|| name.to_string())?;
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_definition_is_cut_to_one_short_line() {
        assert_eq!(
            shorten("a ceremonial dinner party for many people"),
            "a ceremonial dinner party for many people"
        );
        let long = "word ".repeat(60);
        let cut = shorten(&long);
        assert!(cut.ends_with('…'), "{cut}");
        assert!(cut.chars().count() <= LONGEST_DEFINITION + 1, "{}", cut.chars().count());
    }

    #[test]
    fn parts_of_speech_are_the_ones_a_dictionary_shows() {
        assert_eq!(part_of_speech("n"), Some("n."));
        assert_eq!(part_of_speech("v"), Some("v."));
        assert_eq!(part_of_speech("a"), Some("adj."));
        assert_eq!(part_of_speech("s"), Some("adj."));
        assert_eq!(part_of_speech("r"), Some("adv."));
        assert_eq!(part_of_speech("x"), None);
    }
}
