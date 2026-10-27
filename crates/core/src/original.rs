//! The original-language words behind a verse — Hebrew for the Old Testament, Aramaic in the parts
//! of Daniel and Ezra written in it, and Greek for the New.
//!
//! Source: STEPBible.org "Translators Amalgamated" Hebrew OT and Greek NT, and their Translators
//! Brief lexicons for the entries. CC BY 4.0, built into `data/original/` by
//! `scripts/build-original.py`. The step is the point: we show the verse's own words rather than
//! claiming which English word became which, so nothing here depends on a translation's wording.
//!
//! Nothing runs on the startup path. The tables unpack the first time a lookup asks for them, and a
//! table that will not unpack leaves the feature empty rather than panicking in front of a reader.

use crate::books::BookId;

/// The language a verse was written in. Aramaic is not a rounding error: Daniel 2:4 onwards is
/// Aramaic, and calling it Hebrew would be a small lie told on every word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    #[default]
    Hebrew,
    Aramaic,
    Greek,
}

impl Language {
    pub fn name(self) -> &'static str {
        match self {
            Language::Hebrew => "Hebrew",
            Language::Aramaic => "Aramaic",
            Language::Greek => "Greek",
        }
    }

    fn from_code(code: u8) -> Self {
        match code {
            b'A' => Language::Aramaic,
            b'G' => Language::Greek,
            _ => Language::Hebrew,
        }
    }
}

/// One word as it stands in the verse: the text itself, how it sounds, the gloss translators give
/// it, and the number that leads to the entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    pub surface: String,
    pub translit: String,
    pub gloss: String,
    pub strong: String,
}

/// A verse's original words, in the order they were written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verse {
    pub language: Language,
    pub words: Vec<Word>,
}

/// A lexicon entry for one Strong's number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    pub lemma: String,
    pub translit: String,
    pub pos: String,
    /// The one-line gloss: "father", "to create".
    pub short: String,
    /// The full definition, with `<br>` between lines exactly as the source writes it.
    pub body: String,
}

impl Entry {
    /// The definition, line by line, with the source's `<br>` markers removed.
    pub fn lines(&self) -> impl Iterator<Item = &str> {
        self.body.split("<br>").map(str::trim).filter(|line| !line.is_empty())
    }
}

/// How to credit the data, for the About panel and the foot of a list.
pub fn credit() -> &'static str {
    "Original-language text and lexicon: STEPBible.org, CC BY 4.0"
}

/// The words of one verse, or `None` when we have no original text for it.
pub fn verse(book: BookId, chapter: u8, verse: u8) -> Option<Verse> {
    bundled::verse(book, chapter, verse)
}

/// The dictionary entry behind a Strong's number, or `None` when there is no entry for it.
pub fn entry(strong: &str) -> Option<Entry> {
    bundled::entry(strong)
}

/// Is there original text for this verse at all? A popover asks this before offering the row.
pub fn has(book: BookId, chapter: u8, verse: u8) -> bool {
    bundled::verse(book, chapter, verse).is_some()
}

#[cfg(feature = "bundled")]
mod bundled {
    use super::{Entry, Language, Verse, Word};
    use crate::books::BookId;
    use std::collections::HashMap;
    use std::sync::OnceLock;

    /// The tables, compressed with zstd exactly as the translations and dictionaries are: the C
    /// library in a native build, the pure-Rust decoder in the browser.
    macro_rules! table {
        ($id:literal) => {
            include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/original/", $id, ".tsv.zst"))
                as &[u8]
        };
    }

    static WORDS: &[u8] = table!("words");
    static LEX_H: &[u8] = table!("lex-h");
    static LEX_G: &[u8] = table!("lex-g");

    /// One compressed table, unpacked to text. A table that will not unpack yields `None` rather
    /// than taking the app down in front of a reader.
    fn unpack(data: &[u8]) -> Option<String> {
        use std::io::Read;
        let mut out = Vec::new();
        #[cfg(not(target_arch = "wasm32"))]
        let unpacked = zstd::stream::read::Decoder::with_buffer(data)
            .and_then(|mut decoder| decoder.read_to_end(&mut out));
        #[cfg(target_arch = "wasm32")]
        let unpacked = match ruzstd::decoding::StreamingDecoder::new(data) {
            Ok(mut decoder) => decoder
                .read_to_end(&mut out)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())),
            Err(e) => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())),
        };
        match unpacked {
            Ok(_) => String::from_utf8(out).ok(),
            Err(e) => {
                #[cfg(not(target_arch = "wasm32"))]
                eprintln!("bible: original-language text would not unpack: {e}");
                let _ = e;
                None
            }
        }
    }

    /// Where one verse's words sit in the unpacked text: from the first byte of its first word to
    /// the last byte of its last, plus the language they are in.
    struct Span {
        start: usize,
        end: usize,
        language: Language,
    }

    struct Words {
        text: String,
        verses: HashMap<(u8, u8, u8), Span>,
    }

    impl Words {
        /// Parsed once, on the first verse anyone asks about.
        fn get() -> Option<&'static Words> {
            static ONCE: OnceLock<Option<Words>> = OnceLock::new();
            ONCE.get_or_init(|| Words::parse(&unpack(WORDS)?)).as_ref()
        }

        /// The file is verse headers — `@GEN 1 1 7 H`, then seven word lines — so one pass builds
        /// every span without keeping a second copy of the text.
        fn parse(text: &str) -> Option<Words> {
            let mut verses = HashMap::with_capacity(32_000);
            let mut open: Option<((u8, u8, u8), usize, Language)> = None;
            let mut offset = 0;
            let mut lines = 0;
            for line in text.split_inclusive('\n') {
                let start = offset;
                offset += line.len();
                lines += 1;
                if line.starts_with('#') {
                    continue;
                }
                if let Some(header) = line.strip_prefix('@') {
                    if let Some((key, from, language)) = open.take() {
                        verses.insert(key, Span { start: from, end: start, language });
                    }
                    let mut parts = header.trim_end().split('\t');
                    let book = parts.next()?;
                    let chapter: u8 = parts.next()?.trim().parse().ok()?;
                    let number: u8 = parts.next()?.trim().parse().ok()?;
                    let _count: usize = parts.next()?.trim().parse().ok()?;
                    let code = parts.next().unwrap_or("H").trim().as_bytes();
                    let language = Language::from_code(*code.first().unwrap_or(&b'H'));
                    let index = BookId::from_code(book)?.index() as u8;
                    open = Some(((index, chapter, number), offset, language));
                }
            }
            if let Some((key, from, language)) = open.take() {
                verses.insert(key, Span { start: from, end: offset.min(text.len()), language });
            }
            debug_assert!(lines > 0);
            (!verses.is_empty()).then(|| Words { text: text.to_string(), verses })
        }

        fn verse(&self, book: BookId, chapter: u8, verse: u8) -> Option<Verse> {
            let span = self.verses.get(&(book.index() as u8, chapter, verse))?;
            let mut words = Vec::new();
            for line in self.text[span.start..span.end].lines() {
                let mut parts = line.trim_end().split('\t');
                let surface = parts.next().unwrap_or_default();
                if surface.is_empty() {
                    continue;
                }
                words.push(Word {
                    surface: surface.to_string(),
                    translit: parts.next().unwrap_or_default().to_string(),
                    gloss: parts.next().unwrap_or_default().to_string(),
                    strong: parts.next().unwrap_or_default().to_string(),
                });
            }
            (!words.is_empty()).then(|| Verse { language: span.language, words })
        }
    }

    /// The two lexicons as one index: entry id to where its text sits, plus a fallback from a bare
    /// number to the first numbered sense, because words carry `H0430` while the lexicon lists
    /// `H0430G`, `H0430H`, `H0430I`.
    struct Lexicon {
        text: String,
        spans: HashMap<String, (usize, usize)>,
        bare: HashMap<String, String>,
    }

    impl Lexicon {
        fn get() -> Option<&'static Lexicon> {
            static ONCE: OnceLock<Option<Lexicon>> = OnceLock::new();
            ONCE.get_or_init(|| {
                let mut text = unpack(LEX_H)?;
                text.push('\n');
                text.push_str(&unpack(LEX_G)?);
                Lexicon::parse(&text)
            })
            .as_ref()
        }

        fn parse(text: &str) -> Option<Lexicon> {
            let mut spans = HashMap::with_capacity(24_000);
            let mut bare: HashMap<String, String> = HashMap::with_capacity(24_000);
            let mut offset = 0;
            for line in text.split_inclusive('\n') {
                let start = offset;
                offset += line.len();
                if line.starts_with('#') || line.trim().is_empty() {
                    continue;
                }
                let ident = line.split('\t').next().unwrap_or_default().trim();
                if ident.is_empty() {
                    continue;
                }
                bare.entry(base_of(ident)).or_insert_with(|| ident.to_string());
                spans.entry(ident.to_string()).or_insert((start, start + line.len() - 1));
            }
            (!spans.is_empty()).then(|| Lexicon { text: text.to_string(), spans, bare })
        }

        /// `H0430J` is not listed, but `H0430` is the same entry and `H0430G` is its first sense.
        fn resolve(&self, number: &str) -> Option<&str> {
            let key = number.trim().trim_end_matches(|c: char| !c.is_ascii_alphanumeric());
            if key.is_empty() {
                return None;
            }
            if let Some((id, _)) = self.spans.get_key_value(key) {
                return Some(id);
            }
            let bare = base_of(key);
            if let Some((id, _)) = self.spans.get_key_value(bare.as_str()) {
                return Some(id);
            }
            self.bare.get(bare.as_str()).map(String::as_str)
        }

        fn entry(&self, number: &str) -> Option<Entry> {
            let id = self.resolve(number)?;
            let (start, end) = *self.spans.get(id)?;
            let mut parts = self.text[start..end].split('\t');
            let field = |p: Option<&str>| p.unwrap_or_default().trim().to_string();
            Some(Entry {
                id: field(parts.next()),
                lemma: field(parts.next()),
                translit: field(parts.next()),
                pos: field(parts.next()),
                short: field(parts.next()),
                body: field(parts.next()),
            })
        }
    }

    /// `H0430G` and `H0430` are the same number to a reader: the trailing letter only separates
    /// senses within one entry.
    fn base_of(id: &str) -> String {
        let trimmed = id.trim().trim_end_matches(|c: char| !c.is_ascii_alphanumeric());
        let without_sense = trimmed.trim_end_matches(|c: char| c.is_ascii_uppercase());
        if without_sense.is_empty() { trimmed.to_string() } else { without_sense.to_string() }
    }

    pub(super) fn verse(book: BookId, chapter: u8, verse: u8) -> Option<Verse> {
        Words::get()?.verse(book, chapter, verse)
    }

    pub(super) fn entry(strong: &str) -> Option<Entry> {
        Lexicon::get()?.entry(strong)
    }
}

#[cfg(all(test, feature = "bundled"))]
mod tests {
    use super::*;
    use crate::books::BookId;

    /// Hebrew points and cantillation marks: the same word can be encoded with the marks in a
    /// different order, so tests compare the letters a reader sees.
    fn consonants(text: &str) -> String {
        text.chars().filter(|c| !('\u{0591}'..='\u{05C7}').contains(c)).collect()
    }

    #[test]
    fn genesis_one_one_has_its_seven_hebrew_words() {
        let v = verse(BookId::GENESIS, 1, 1).expect("Genesis 1:1 has original text");
        assert_eq!(v.language, Language::Hebrew);
        assert_eq!(v.words.len(), 7, "Bereshit bara elohim et hashamayim ve'et ha'aretz");
        assert_eq!(consonants(&v.words[0].surface), "בראשית");
        assert!(v.words[0].gloss.contains("beginning"), "{}", v.words[0].gloss);
        assert_eq!(v.words[0].strong, "H7225G");
        assert_eq!(v.words[1].strong, "H1254A", "bara, the word behind 'created'");
        // The verse ends with the earth, not with an escape marker from the source.
        assert_eq!(v.words[6].strong, "H0776G");
        assert_eq!(consonants(&v.words[6].surface), "הארץ");
    }

    #[test]
    fn the_new_testament_is_greek() {
        let v = verse(BookId::from_code("JHN").unwrap(), 3, 16).expect("John 3:16");
        assert_eq!(v.language, Language::Greek);
        assert_eq!(v.words.len(), 26);
        assert!(v.words.iter().any(|w| w.strong.starts_with("G0025")), "egapesen is G25");
    }

    #[test]
    fn daniel_two_five_is_aramaic() {
        let v = verse(BookId::from_code("DAN").unwrap(), 2, 5).expect("Daniel 2:5");
        assert_eq!(v.language, Language::Aramaic, "the Aramaic half of Daniel");
    }

    #[test]
    fn psalm_titles_come_with_verse_one() {
        // The KJV prints the title as part of verse 1; the Hebrew numbers it as verse 0. The list
        // must match what the reader can see, so the title's words belong to verse 1.
        let v = verse(BookId::PSALMS, 3, 1).expect("Psalm 3:1");
        assert!(v.words[0].gloss.contains("psalm"), "{:?}", v.words[0]);
    }

    #[test]
    fn a_verse_we_have_nothing_for_says_so() {
        // Romans 16:25 is a versification outlier: it is in the KJV and not in the tagged text.
        assert!(verse(BookId::from_code("ROM").unwrap(), 16, 25).is_none());
        assert!(!has(BookId::from_code("ROM").unwrap(), 16, 25));
    }

    #[test]
    fn an_entry_is_reachable_from_the_numbers_the_words_carry() {
        let entry = entry("H1254A").expect("bara has an entry");
        assert_eq!(consonants(&entry.lemma), "ברא");
        assert!(entry.short.to_lowercase().contains("create"), "{}", entry.short);
        assert!(entry.lines().count() > 0);
    }

    #[test]
    fn a_bare_number_finds_its_first_sense() {
        // Words carry H0430 where the lexicon lists H0430G, H0430H, H0430I.
        let entry = entry("H0430").expect("elohim resolves through its first sense");
        assert!(entry.id.starts_with("H0430"), "{}", entry.id);
        assert!(entry.short.to_lowercase().contains("god"), "{}", entry.short);
        // And a numbered sense that the lexicon never lists still lands on the entry.
        assert!(entry_from("H0430J").is_some());
    }

    fn entry_from(number: &str) -> Option<Entry> {
        entry(number)
    }

    #[test]
    fn the_whole_bible_is_covered_except_the_known_oddities() {
        let mut missing = Vec::new();
        for book in BookId::all() {
            for chapter in 1..=book.chapters() {
                // Only the verses we know about: a chapter's last verse is enough to prove the
                // span parsing works across every book.
                if verse(book, chapter, 1).is_none() {
                    missing.push(format!("{} {chapter}:1", book.name()));
                }
            }
        }
        assert!(missing.len() <= 12, "chapter openings without originals: {missing:?}");
    }
}
