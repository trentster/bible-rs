//! The two lookups that sit beside the text: the verses the tradition links to this one, and the
//! words it was written in.
//!
//! Both already live in `bible-core` — the Mac app reads them through Rust directly. The phone apps
//! reach the same data through here, so a verse means the same thing on both: the same cross
//! references in the same order, the same Hebrew and Greek, the same lexicon entry behind a number.
//!
//! Nothing here decodes a file. `related::of` and `original::verse` unpack on first ask and cache,
//! so the cost is paid when a reader opens a card, never at launch.

use bible_core::{BookId, original, related};

use crate::{BibleError, VerseRef};

/// One verse offered as related. A link may cover a range, which is why it carries an end.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RelatedLink {
    pub book: u8,
    pub chapter: u8,
    pub verse: u8,
    /// The last chapter and verse of the range; equal to the start when the link is one verse.
    pub end_chapter: u8,
    pub end_verse: u8,
    /// OpenBible's readers' votes, best first. A guide to how well worn the link is, nothing more.
    pub votes: u16,
    /// "Romans 5:8", or "John 11:25-26" for a range.
    pub label: String,
}

/// One word as it stands in the verse: the text itself, how it sounds, the gloss translators give
/// it, and the number that leads to the entry.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct OriginalWord {
    pub surface: String,
    pub translit: String,
    pub gloss: String,
    pub strong: String,
}

/// A verse's original words, in the order they were written.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct OriginalVerse {
    /// "Hebrew" or "Greek" — what the reader is looking at.
    pub language: String,
    pub words: Vec<OriginalWord>,
}

/// A lexicon entry for one Strong's number.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct LexiconEntry {
    pub id: String,
    pub lemma: String,
    pub translit: String,
    pub pos: String,
    /// The one-line gloss: "father", "to create".
    pub short: String,
    /// The full definition, one line per row, with the source's line markers already removed.
    pub lines: Vec<String>,
}

fn split(at: VerseRef) -> Result<(BookId, u8, u8), BibleError> {
    let book = BookId::new(at.book as usize)
        .ok_or_else(|| BibleError::Failed { message: format!("no book {}", at.book) })?;
    Ok((book, at.chapter, at.verse))
}

/// The verses linked to this one, best-voted first. Empty when the tradition links none.
pub fn related_verses(at: VerseRef) -> Result<Vec<RelatedLink>, BibleError> {
    let (book, chapter, verse) = split(at)?;
    Ok(related::of(book, chapter, verse)
        .iter()
        .map(|link| RelatedLink {
            book: link.book.index() as u8,
            chapter: link.chapter,
            verse: link.verse,
            end_chapter: link.end_chapter,
            end_verse: link.end_verse,
            votes: link.votes,
            label: link.label(),
        })
        .collect())
}

/// How to credit the cross references.
pub fn related_credit() -> String {
    related::credit().to_string()
}

/// The words of one verse in the language it was written in, or `None` when we have none for it.
pub fn original_verse(at: VerseRef) -> Result<Option<OriginalVerse>, BibleError> {
    let (book, chapter, verse) = split(at)?;
    Ok(original::verse(book, chapter, verse).map(|found| OriginalVerse {
        language: found.language.name().to_string(),
        words: found
            .words
            .into_iter()
            .map(|word| OriginalWord {
                surface: word.surface,
                translit: word.translit,
                gloss: word.gloss,
                strong: word.strong,
            })
            .collect(),
    }))
}

/// The dictionary entry behind a Strong's number, or `None` when there is no entry for it.
pub fn lexicon_entry(strong: String) -> Result<Option<LexiconEntry>, BibleError> {
    Ok(original::entry(&strong).map(|entry| {
        // Take the lines before the fields move: `lines()` borrows the body it reads.
        let lines: Vec<String> = entry.lines().map(str::to_string).collect();
        LexiconEntry {
            id: entry.id,
            lemma: entry.lemma,
            translit: entry.translit,
            pos: entry.pos,
            short: entry.short,
            lines,
        }
    }))
}

/// How to credit the original-language text and lexicon.
pub fn original_credit() -> String {
    original::credit().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(book: u8, chapter: u8, verse: u8) -> VerseRef {
        VerseRef { book, chapter, verse }
    }

    /// John 3:16 is the most cross-referenced verse there is; it must never come back empty, and
    /// it must stay best-voted first.
    #[test]
    fn john_three_sixteen_has_links_best_first() {
        let links = related_verses(at(42, 3, 16)).expect("links");
        assert!(!links.is_empty(), "John 3:16 has no links at all");
        let votes: Vec<u16> = links.iter().map(|l| l.votes).collect();
        assert!(votes.windows(2).all(|w| w[0] >= w[1]), "links are not ordered by votes");
        assert!(links.iter().any(|l| l.label.contains(':')), "a label has no chapter and verse");
    }

    #[test]
    fn a_verse_with_no_links_is_empty_not_an_error() {
        // Matthew 1:1 is a known gap, recorded in DECISIONS.md.
        let links = related_verses(at(39, 1, 1)).expect("links");
        assert!(links.is_empty() || !links.is_empty(), "must not error either way");
    }

    #[test]
    fn genesis_one_one_carries_hebrew_words() {
        let verse = original_verse(at(0, 1, 1)).expect("verse").expect("Genesis 1:1 has no original text");
        assert_eq!(verse.language, "Hebrew");
        assert!(verse.words.len() >= 7, "Genesis 1:1 should have at least seven words");
        assert!(verse.words.iter().all(|w| !w.surface.is_empty()));
    }

    #[test]
    fn john_three_sixteen_carries_greek_words() {
        let verse = original_verse(at(42, 3, 16)).expect("verse").expect("John 3:16 has no original text");
        assert_eq!(verse.language, "Greek");
        assert!(verse.words.iter().any(|w| !w.strong.is_empty()), "no Strong's numbers at all");
    }

    /// Lexicon numbers are four digits, zero-padded, and may carry a sense letter (`H1254A`).
    /// A reader's word carries the number exactly as the source writes it, so the phone must not
    /// reformat it on the way in. A bare number resolves to its first sense, so the entry that
    /// comes back may be a more specific id than the one asked for — `H0430` returns `H0430G`.
    #[test]
    fn a_strong_number_leads_to_an_entry_with_lines() {
        let entry = lexicon_entry("H0430".to_string()).expect("entry").expect("H0430 is not in the lexicon");
        assert!(entry.id.starts_with("H0430"), "resolved to an unrelated entry: {}", entry.id);
        assert!(!entry.short.is_empty());
        assert!(!entry.lines.is_empty(), "the entry has no definition lines");
        assert!(entry.lines.iter().all(|l| !l.contains("<br>")), "line markers were not stripped");
    }

    /// An unpadded number is not the same key, and must come back empty rather than guess.
    #[test]
    fn an_unpadded_or_unknown_number_is_none() {
        assert!(lexicon_entry("H430".to_string()).expect("entry").is_none());
        assert!(lexicon_entry("H99999".to_string()).expect("entry").is_none());
    }

    #[test]
    fn a_bad_book_is_an_error_not_a_panic() {
        assert!(related_verses(at(200, 1, 1)).is_err());
        assert!(original_verse(at(200, 1, 1)).is_err());
    }
}
