//! Looking a word up: the dictionaries a reader can define a word from.
//!
//! A lexicon is data, like a translation: one file of entries, embedded in the binary and parsed
//! the first time someone defines a word, so it costs nothing at launch. Entries are looked up by
//! headword, with the ordinary English inflections stripped first (`loved` finds `love`), because
//! the text says "loved" and the dictionary says "love".
//!
//! Every entry names the source it came from, so the popover can say which dictionary answered
//! rather than presenting a definition from nowhere.

use std::collections::HashMap;

use anyhow::{Context, Result};

/// Which dictionary an entry came from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    /// Open English WordNet (2025): plain meanings of ordinary English words.
    Wordnet,
    /// Easton's Bible Dictionary (1897): Bible words, names and places.
    Easton,
    /// Webster's Revised Unabridged (1913): ordinary and archaic English.
    Webster1913,
}

impl Source {
    /// The line the popover shows under an entry.
    pub fn label(self) -> &'static str {
        match self {
            Source::Wordnet => "WordNet",
            Source::Easton => "Easton's Bible Dictionary (1897)",
            Source::Webster1913 => "Webster's Revised Unabridged (1913)",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "wordnet" => Some(Source::Wordnet),
            "easton" => Some(Source::Easton),
            "webster1913" | "webster" => Some(Source::Webster1913),
            _ => None,
        }
    }
}

/// One dictionary entry: a headword and what it says.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Entry {
    pub headword: String,
    pub body: String,
    pub source: Source,
}

/// The entries, by the headword they are filed under.
pub struct Lexicon {
    by_headword: HashMap<String, Vec<Entry>>,
}

/// The first line of a lexicon file, so a wrong or truncated file is refused instead of half read.
const HEADER: &str = "#bible-rs-lexicon v1";

impl Lexicon {
    /// Reads a lexicon file: the header, then one entry a line, as `source<TAB>headword<TAB>body`.
    ///
    /// Blank lines and lines starting with `#` are skipped, so a file can be annotated by hand.
    pub fn parse(text: &str) -> Result<Self> {
        let mut lines = text.lines();
        let header = lines.next().unwrap_or_default().trim_end();
        anyhow::ensure!(header == HEADER, "a lexicon must start with {HEADER:?}, not {header:?}");
        let mut by_headword: HashMap<String, Vec<Entry>> = HashMap::new();
        for (i, line) in lines.enumerate() {
            let line = line.trim_end();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut fields = line.split('\t');
            let (Some(source), Some(headword), Some(body)) = (fields.next(), fields.next(), fields.next())
            else {
                anyhow::bail!("lexicon line {} needs a source, a headword and a body", i + 2);
            };
            let source = Source::parse(source)
                .with_context(|| format!("lexicon line {}: unknown source {source:?}", i + 2))?;
            let headword = headword.trim();
            anyhow::ensure!(!headword.is_empty(), "lexicon line {}: empty headword", i + 2);
            let entry = Entry {
                headword: headword.to_string(),
                body: body.replace("\\n", "\n").trim().to_string(),
                source,
            };
            by_headword.entry(normalize(headword)).or_default().push(entry);
        }
        Ok(Self { by_headword })
    }

    /// An empty dictionary: what a file that will not parse leaves behind, rather than a panic.
    fn none() -> Self {
        Self { by_headword: HashMap::new() }
    }

    /// The entries for a word as it appears in the text, alphabetically by source.
    ///
    /// The word is tried as written, then with ordinary inflections stripped: `loved`, `loving`
    /// and `loves` all reach `love`.
    pub fn lookup(&self, word: &str) -> Vec<Entry> {
        for form in forms(word) {
            if let Some(entries) = self.by_headword.get(&form) {
                let mut entries = entries.clone();
                entries.sort_by_key(|e| e.source as u8);
                return entries;
            }
        }
        Vec::new()
    }

    /// How many headwords the lexicon holds (several senses of one word count once).
    pub fn len(&self) -> usize {
        self.by_headword.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_headword.is_empty()
    }
}

/// A headword or a word from the text, reduced to what a lookup compares: lowercase, no leading or
/// trailing punctuation, and an apostrophe's two spellings made one.
fn normalize(word: &str) -> String {
    word.trim()
        .trim_matches(|c: char| !c.is_alphanumeric() && c != '\'')
        .to_lowercase()
        .replace('\u{2019}', "'")
}

/// The first `count` sentences of a text, so a long encyclopaedia entry becomes a definition.
fn first_sentences(text: &str, count: usize) -> String {
    let mut out = String::new();
    let mut seen = 0;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        out.push(c);
        if c == '.' && chars.peek().map_or(true, |next| next.is_whitespace()) {
            seen += 1;
            if seen == count {
                break;
            }
        }
    }
    out.trim().to_string()
}

/// The keys to try for `word`, most specific first: the word itself, then its likelier base forms.
fn forms(word: &str) -> Vec<String> {
    let word = normalize(word);
    let mut forms = vec![word.clone()];
    let push = |forms: &mut Vec<String>, form: String| {
        if form.len() >= 2 && !forms.contains(&form) {
            forms.push(form);
        }
    };
    // Plurals and third person: loves, churches, cities.
    if let Some(stem) = word.strip_suffix("ies").filter(|s| !s.is_empty()) {
        push(&mut forms, format!("{stem}y"));
    }
    if let Some(stem) = word.strip_suffix("es").filter(|s| !s.is_empty()) {
        push(&mut forms, stem.to_string());
    }
    if let Some(stem) = word.strip_suffix('s').filter(|s| !s.is_empty()) {
        push(&mut forms, stem.to_string());
    }
    // Participles and past tense: loving, loved, beguiled; with the final consonant doubled
    // (begged, running) or an e restored (loved → love).
    for suffix in ["ing", "ed"] {
        if let Some(stem) = word.strip_suffix(suffix).filter(|s| s.len() >= 2) {
            push(&mut forms, stem.to_string());
            push(&mut forms, format!("{stem}e"));
            if let Some(last) = stem.chars().last() {
                if stem.chars().rev().nth(1) == Some(last) {
                    push(&mut forms, stem.chars().take(stem.chars().count() - 1).collect());
                }
            }
        }
    }
    // Comparatives: holy → holier, great → greater.
    for (suffix, end) in [("ier", "y"), ("er", "")] {
        if let Some(stem) = word.strip_suffix(suffix).filter(|s| s.len() >= 2) {
            push(&mut forms, format!("{stem}{end}"));
        }
    }
    forms
}

/// The dictionaries compiled into the binary, read on first use.
///
/// The files are written by `cargo run -p xtask -- build-lexicon`, which converts the modules
/// CrossWire ships into this crate's plain-text format. None of this runs on the startup path: the
/// parse happens the first time a word is looked up, and a file that will not parse leaves the
/// dictionary empty rather than panicking in front of the reader.
#[cfg(feature = "bundled")]
mod bundled {
    use super::{Entry, Lexicon};
    use std::collections::HashMap;
    use std::sync::OnceLock;

    /// The bundled tables, compressed with zstd exactly as the translations are: the C library in a
    /// native build, the pure-Rust decoder in the browser. Unpacking waits for the first lookup, so
    /// 13 MB of dictionary costs nothing on the way to a first painted frame.
    macro_rules! lex {
        ($id:literal) => {
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../data/lexicon/",
                $id,
                ".tsv.zst"
            )) as &[u8]
        };
    }

    static EASTON: &[u8] = lex!("easton");
    static WORDNET: &[u8] = lex!("wordnet");
    static FORMS: &[u8] = lex!("wordnet-forms");
    /// Hand-edited source data, a couple of kilobytes: kept as text so an edit can never leave a
    /// stale compressed copy behind for the app to use.
    static ALIASES: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../data/lexicon/aliases.tsv"
    ));

    /// One compressed table, unpacked to text. A table that will not unpack leaves the dictionary
    /// empty and says so, rather than taking the app down in front of a reader.
    fn unpack(data: &[u8]) -> String {
        use std::io::Read;
        let mut out = Vec::new();
        #[cfg(not(target_arch = "wasm32"))]
        let unpacked = zstd::stream::read::Decoder::with_buffer(data)
            .and_then(|mut decoder| decoder.read_to_end(&mut out));
        #[cfg(target_arch = "wasm32")]
        let unpacked = match ruzstd::decoding::StreamingDecoder::new(data) {
            Ok(mut decoder) => decoder.read_to_end(&mut out),
            Err(e) => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())),
        };
        if let Err(e) = unpacked {
            eprintln!("lexicon: a bundled table would not unpack: {e}");
            return String::new();
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    /// One spelling the text uses, and the headword the dictionary files it under.
    #[derive(Clone, Debug)]
    pub struct Alias {
        pub kjv: String,
        pub headword: String,
        pub verse: String,
    }

    /// What a lookup found: the entries, and the headword they were found under when the spelling
    /// asked about is not itself a headword ("Melchisedec" reaching "Melchizedek").
    #[derive(Clone, Debug)]
    pub struct Definition {
        pub entries: Vec<Entry>,
        pub shown_as: Option<String>,
    }

    impl Definition {
        pub fn is_empty(&self) -> bool {
            self.entries.is_empty()
        }
    }

    /// The curated spelling table, read on first use.
    pub fn aliases() -> &'static HashMap<String, Alias> {
        static ONCE: OnceLock<HashMap<String, Alias>> = OnceLock::new();
        ONCE.get_or_init(|| {
            let mut map = HashMap::new();
            for line in ALIASES.lines() {
                let line = line.trim_end();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let mut fields = line.split('\t');
                let (Some(kjv), Some(headword)) = (fields.next(), fields.next()) else {
                    continue;
                };
                map.insert(
                    super::normalize(kjv),
                    Alias {
                        kjv: kjv.to_string(),
                        headword: headword.to_string(),
                        verse: fields.next().unwrap_or_default().to_string(),
                    },
                );
            }
            map
        })
    }

    fn parsed(once: &'static OnceLock<Lexicon>, data: &[u8]) -> &'static Lexicon {
        once.get_or_init(|| {
            let text = unpack(data);
            match Lexicon::parse(&text) {
                Ok(lexicon) => lexicon,
                Err(e) => {
                    // Loud once, then empty: a broken build-time file must not panic in front of a
                    // reader.
                    eprintln!("lexicon: a bundled dictionary would not parse: {e}");
                    Lexicon::none()
                }
            }
        })
    }

    fn easton() -> &'static Lexicon {
        static ONCE: OnceLock<Lexicon> = OnceLock::new();
        parsed(&ONCE, EASTON)
    }

    fn wordnet() -> &'static Lexicon {
        static ONCE: OnceLock<Lexicon> = OnceLock::new();
        parsed(&ONCE, WORDNET)
    }

    /// WordNet's irregular forms, as `form → lemma` ("went" → "go").
    fn forms() -> &'static HashMap<String, String> {
        static ONCE: OnceLock<HashMap<String, String>> = OnceLock::new();
        ONCE.get_or_init(|| {
            let mut map = HashMap::new();
            for line in unpack(FORMS).lines() {
                let line = line.trim_end();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((form, lemma)) = line.split_once('\t') {
                    map.insert(super::normalize(form), lemma.to_string());
                }
            }
            map
        })
    }

    /// What the bundled dictionaries say about a word or short phrase.
    ///
    /// WordNet answers ordinary words with plain meanings. Easton's answers what WordNet has never
    /// heard of — Bible names and places — and only its opening sentences are shown, because a
    /// reader asked what a word means, not for an essay. Irregular forms come from WordNet's own
    /// table ("went" → "go"), and the curated spelling table covers the rest ("spake" → "speak").
    pub fn define(word: &str) -> Definition {
        let alias = aliases().get(&super::normalize(word)).cloned();

        // WordNet first: ordinary words get a plain meaning.
        if let Some(entries) = wordnet_entries(word) {
            return Definition { entries, shown_as: None };
        }
        // A spelling the table points elsewhere may still be an ordinary word ("spake" → "speak").
        if let Some(target) = &alias {
            if let Some(entries) = wordnet_entries(&target.headword) {
                return Definition { entries, shown_as: Some(target.headword.clone()) };
            }
        }
        // Then Easton's, for Bible names and places WordNet has never heard of.
        let entries = easton().lookup(word);
        if !entries.is_empty() {
            return Definition { entries: opening(entries), shown_as: None };
        }
        if let Some(target) = &alias {
            let entries = easton().lookup(&target.headword);
            if !entries.is_empty() {
                return Definition { entries: opening(entries), shown_as: Some(target.headword.clone()) };
            }
        }
        Definition { entries: Vec::new(), shown_as: None }
    }

    /// WordNet's answer for a word, following its irregular forms first.
    fn wordnet_entries(word: &str) -> Option<Vec<Entry>> {
        let direct = wordnet().lookup(word);
        if !direct.is_empty() {
            return Some(direct);
        }
        let lemma = forms().get(&super::normalize(word))?;
        let found = wordnet().lookup(lemma);
        if found.is_empty() {
            None
        } else {
            Some(found)
        }
    }

    /// Each entry cut to its first two sentences.
    fn opening(entries: Vec<Entry>) -> Vec<Entry> {
        entries
            .into_iter()
            .map(|mut entry| {
                entry.body = super::first_sentences(&entry.body, 2);
                entry
            })
            .collect()
    }

    /// How many headwords the bundled dictionaries hold together; for tests and diagnostics.
    pub fn headwords() -> usize {
        wordnet().len() + easton().len()
    }
}

#[cfg(feature = "bundled")]
pub use bundled::{aliases, define, headwords, Alias, Definition};

#[cfg(feature = "bundled")]
#[test]
fn the_bundled_dictionary_answers_the_words_a_reader_will_ask_about() {
    assert!(headwords() > 3_000, "the bundled dictionary parsed only {} headwords", headwords());
    let melchizedek = define("Melchizedek");
    assert_eq!(melchizedek.entries.len(), 1, "{melchizedek:?}");
    assert!(melchizedek.entries[0].body.contains("king of Salem"), "{}", melchizedek.entries[0].body);
    assert_eq!(melchizedek.entries[0].source, Source::Easton);
    assert_eq!(melchizedek.entries[0].source.label(), "Easton's Bible Dictionary (1897)");
    // A headword is answered as itself, with no remapping to report.
    assert!(melchizedek.shown_as.is_none());
    // An inflection of a headword reaches the headword itself.
    assert!(!define("Aarons").is_empty());
    // A phrase with its own entry is found as a phrase.
    assert!(!define("burnt offering").is_empty());
    // A word with no entry says nothing rather than guessing.
    assert!(define("zorblat").is_empty());
    assert!(define("zorblat").shown_as.is_none());
    // An ordinary word comes from WordNet, with its part of speech.
    let feast = define("feast");
    assert_eq!(feast.entries[0].source, Source::Wordnet);
    assert!(feast.entries[0].body.starts_with("n. "), "{}", feast.entries[0].body);
    // An irregular form reaches its lemma through WordNet's own table.
    assert!(!define("went").is_empty());
    assert!(!define("feet").is_empty());
    // The KJV's archaic past tense is not in WordNet, so the curated table carries it.
    assert!(!define("spake").is_empty());
    // A Bible name WordNet has never heard of comes from Easton's, cut to its opening sentences.
    let melchisedec = define("Melchisedec");
    assert_eq!(melchisedec.entries[0].source, Source::Easton);
    let body = &melchisedec.entries[0].body;
    assert!(body.contains("king of Salem"), "{body}");
    assert!(!body.contains("typical significance"), "the essay was not trimmed: {body}");
}

#[cfg(all(test, feature = "bundled"))]
mod spelling_tests {
    use super::*;
    use crate::bundled;

    #[test]
    fn a_kjv_spelling_reaches_the_headword_the_dictionary_files_it_under() {
        let melchisedec = define("Melchisedec");
        assert_eq!(melchisedec.shown_as.as_deref(), Some("Melchizedek"));
        assert!(melchisedec.entries[0].body.contains("king of Salem"), "{:?}", melchisedec);
    }

    /// One assertion per line of the curated table: the headword is real, and the KJV prints that
    /// spelling in the verse given. A wrong verse reference fails here rather than in front of a
    /// reader.
    #[test]
    fn every_alias_names_a_real_headword_and_a_verse_that_uses_that_spelling() {
        let kjv = bundled::load("kjv").expect("the bundled KJV");
        assert!(!aliases().is_empty(), "the alias table is empty");
        for alias in aliases().values() {
            let found = define(&alias.kjv);
            assert_eq!(
                found.shown_as.as_deref(),
                Some(alias.headword.as_str()),
                "{} did not reach {}",
                alias.kjv,
                alias.headword
            );
            assert!(!found.entries.is_empty(), "{} has no entry", alias.headword);
            let reference = crate::reference::parse(&alias.verse)
                .unwrap_or_else(|| panic!("{}: verse {:?} does not parse", alias.kjv, alias.verse));
            let verse = kjv
                .find(reference.verse_ref())
                .unwrap_or_else(|| panic!("{}: the KJV has no {}", alias.kjv, alias.verse));
            assert!(
                verse.text.to_lowercase().contains(&alias.kjv.to_lowercase()),
                "the KJV does not print {:?} in {}: {:?}",
                alias.kjv,
                alias.verse,
                verse.text
            );
        }
    }

    /// An alias must never shadow a word that is a headword in its own right, nor map to itself.
    #[test]
    fn no_alias_points_at_a_word_the_dictionary_already_has_under_that_spelling() {
        for alias in aliases().values() {
            assert_ne!(
                normalize_for_test(&alias.kjv),
                normalize_for_test(&alias.headword),
                "{} maps to itself",
                alias.kjv
            );
            assert!(
                define(&alias.headword).shown_as.is_none(),
                "{} is itself an alias, so chains would form",
                alias.headword
            );
        }
    }

    fn normalize_for_test(word: &str) -> String {
        word.trim().to_lowercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A handful of real headwords, in the file format, so the engine is tested before the whole
    /// dictionary is poured in.
    fn fixture() -> Lexicon {
        Lexicon::parse(
            "#bible-rs-lexicon v1\n\
             # a comment line, skipped\n\
             easton\tPropitiation\tThat by which God is rendered propitious.\n\
             webster1913\tLove\tA feeling of strong attachment.\\nSecond sense.\n\
             easton\tLove\tIn a biblical sense, the love of God.\n\
             easton\tMelchizedek\tKing of Salem and priest of the most high God.\n",
        )
        .unwrap()
    }

    #[test]
    fn finds_a_word_and_every_source_that_defines_it() {
        let lex = fixture();
        let entries = lex.lookup("love");
        assert_eq!(entries.len(), 2);
        // Easton first, then Webster's: the same order the popover shows.
        assert_eq!(entries[0].source, Source::Easton);
        assert_eq!(entries[1].source, Source::Webster1913);
        assert_eq!(entries[0].headword, "Love");
        // A literal \n in the file becomes a paragraph in the body.
        assert!(entries[1].body.contains("\nSecond sense."));
    }

    #[test]
    fn ignores_case_and_punctuation_around_a_word() {
        let lex = fixture();
        assert_eq!(lex.lookup("MELCHIZEDEK").len(), 1);
        assert_eq!(lex.lookup("  propitiation,").len(), 1);
        assert_eq!(lex.lookup("Propitiation.").len(), 1);
    }

    #[test]
    fn an_inflected_word_reaches_its_headword() {
        let lex = fixture();
        for word in ["loves", "loved", "loving"] {
            assert_eq!(lex.lookup(word).len(), 2, "{word} should reach love");
        }
    }

    #[test]
    fn a_word_with_no_entry_is_empty_rather_than_a_guess() {
        let lex = fixture();
        assert!(lex.lookup("zzzz").is_empty());
        assert!(lex.lookup("").is_empty());
        // A word that only looks like an inflection of something absent stays absent.
        assert!(lex.lookup("gibberishing").is_empty());
    }

    #[test]
    fn a_broken_file_is_refused() {
        assert!(Lexicon::parse("easton\tLove\tNo header\n").is_err());
        assert!(Lexicon::parse("#bible-rs-lexicon v1\nnonesuch\tLove\tx\n").is_err());
        assert!(Lexicon::parse("#bible-rs-lexicon v1\nonly-two\tfields\n").is_err());
    }

    #[test]
    fn counts_headwords_not_senses() {
        let lex = fixture();
        assert_eq!(lex.len(), 3, "love, propitiation and Melchizedek");
        assert!(!lex.is_empty());
    }
}
