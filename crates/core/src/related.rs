//! Cross references: the verses the church's reading tradition links to this one.
//!
//! The data is OpenBible.info's cross-reference set, which draws mostly on the public-domain Treasury
//! of Scripture Knowledge and ranks each link by the votes of the site's own readers. It is CC BY
//! 4.0 and credited in the About panel and at the foot of the card that shows it.
//!
//! Nothing here is unpacked until a reader asks for a verse's related verses: the table is 0.75 MB
//! compressed and about 2 MB parsed, and startup pays none of it.

use crate::books::BookId;

/// One verse offered as related. A link may cover a range, which is why it carries an end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Link {
    pub book: BookId,
    pub chapter: u8,
    pub verse: u8,
    /// The last chapter and verse of the range; equal to the start when the link is one verse.
    pub end_chapter: u8,
    pub end_verse: u8,
    /// OpenBible's readers' votes, best first. A guide to how well worn the link is, nothing more.
    pub votes: u16,
}

impl Link {
    /// "Romans 5:8", or "John 11:25–26" for a range — and "Genesis 1:1–2:3" when it crosses chapters.
    pub fn label(&self) -> String {
        let book = self.book.ref_name();
        if self.end_chapter == self.chapter && self.end_verse == self.verse {
            format!("{book} {}:{}", self.chapter, self.verse)
        } else if self.end_chapter == self.chapter {
            format!("{book} {}:{}-{}", self.chapter, self.verse, self.end_verse)
        } else {
            format!("{book} {}:{}-{}:{}", self.chapter, self.verse, self.end_chapter, self.end_verse)
        }
    }
}

/// How to credit the data, for the foot of the card and the About panel.
pub fn credit() -> &'static str {
    "OpenBible.info cross references, CC BY 4.0"
}

/// The verses linked to this one, best-voted first. Empty when the tradition links none.
pub fn of(book: BookId, chapter: u8, verse: u8) -> &'static [Link] {
    bundled::of(book, chapter, verse)
}

/// Is anything linked to this verse at all? The menu asks before offering the card.
pub fn has(book: BookId, chapter: u8, verse: u8) -> bool {
    !of(book, chapter, verse).is_empty()
}

#[cfg(feature = "bundled")]
mod bundled {
    use super::Link;
    use crate::books::BookId;
    use std::collections::HashMap;
    use std::sync::OnceLock;

    /// The table, compressed with zstd exactly as the translations and dictionaries are: the C
    /// library in a native build, the pure-Rust decoder in the browser.
    static XREFS: &[u8] =
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/related/xrefs.tsv.zst"));

    /// The compressed table, unpacked to text. A table that will not unpack yields `None` rather
    /// than taking the app down in front of a reader.
    fn unpack() -> Option<String> {
        use std::io::Read;
        let mut out = Vec::new();
        #[cfg(not(target_arch = "wasm32"))]
        let unpacked = zstd::stream::read::Decoder::with_buffer(XREFS)
            .and_then(|mut decoder| decoder.read_to_end(&mut out));
        #[cfg(target_arch = "wasm32")]
        let unpacked = match ruzstd::decoding::StreamingDecoder::new(XREFS) {
            Ok(mut decoder) => decoder
                .read_to_end(&mut out)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())),
            Err(e) => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())),
        };
        match unpacked {
            Ok(_) => String::from_utf8(out).ok(),
            Err(e) => {
                #[cfg(not(target_arch = "wasm32"))]
                eprintln!("bible: cross references would not unpack: {e}");
                let _ = e;
                None
            }
        }
    }

    /// Every link in one flat list — the verses' links are contiguous — plus where each verse's run
    /// starts and ends. The text is dropped once parsed: the numbers are all the card needs, and
    /// keeping 3.4 MB of text alive to read eight references would be waste.
    struct Table {
        links: Vec<Link>,
        runs: HashMap<(u8, u8, u8), (u32, u32)>,
    }

    impl Table {
        fn get() -> Option<&'static Table> {
            static ONCE: OnceLock<Option<Table>> = OnceLock::new();
            ONCE.get_or_init(|| Table::parse(&unpack()?)).as_ref()
        }

        /// One pass: `@CODE chapter verse` opens a run, then six-field rows fill it.
        fn parse(text: &str) -> Option<Table> {
            let mut links: Vec<Link> = Vec::with_capacity(200_000);
            let mut runs: HashMap<(u8, u8, u8), (u32, u32)> = HashMap::with_capacity(30_000);
            let mut open: Option<((u8, u8, u8), u32)> = None;
            for line in text.lines() {
                if line.starts_with('#') || line.is_empty() {
                    continue;
                }
                if let Some(header) = line.strip_prefix('@') {
                    if let Some((key, start)) = open.take() {
                        runs.insert(key, (start, links.len() as u32));
                    }
                    let mut parts = header.split('\t');
                    let code = parts.next()?;
                    let chapter: u8 = parts.next()?.trim().parse().ok()?;
                    let verse: u8 = parts.next()?.trim().parse().ok()?;
                    let book = BookId::from_code(code)?;
                    open = Some(((book.index() as u8, chapter, verse), links.len() as u32));
                    continue;
                }
                let mut parts = line.split('\t');
                let votes: i32 = parts.next()?.trim().parse().ok()?;
                let code = parts.next()?;
                let chapter: u8 = parts.next()?.trim().parse().ok()?;
                let verse: u8 = parts.next()?.trim().parse().ok()?;
                let end_chapter: u8 = parts.next()?.trim().parse().ok()?;
                let end_verse: u8 = parts.next()?.trim().parse().ok()?;
                let book = BookId::from_code(code)?;
                // The build drops links the source's readers voted down; one that slipped through is
                // not offered either.
                let Ok(votes) = u16::try_from(votes) else { continue };
                if votes == 0 {
                    continue;
                }
                let (end_chapter, end_verse) = if end_chapter == 0 || end_verse == 0 {
                    (chapter, verse)
                } else {
                    (end_chapter, end_verse)
                };
                links.push(Link { book, chapter, verse, end_chapter, end_verse, votes });
            }
            if let Some((key, start)) = open.take() {
                runs.insert(key, (start, links.len() as u32));
            }
            (!runs.is_empty()).then(|| Table { links, runs })
        }

        fn of(&self, book: BookId, chapter: u8, verse: u8) -> &[Link] {
            match self.runs.get(&(book.index() as u8, chapter, verse)) {
                Some((start, end)) => &self.links[*start as usize..*end as usize],
                None => &[],
            }
        }
    }

    pub(super) fn of(book: BookId, chapter: u8, verse: u8) -> &'static [Link] {
        Table::get().map_or(&[], |table| table.of(book, chapter, verse))
    }
}

#[cfg(not(feature = "bundled"))]
mod bundled {
    use super::Link;
    use crate::books::BookId;

    pub(super) fn of(_book: BookId, _chapter: u8, _verse: u8) -> &'static [Link] {
        &[]
    }
}

#[cfg(all(test, feature = "bundled"))]
mod tests {
    use super::*;

    fn book(code: &str) -> BookId {
        BookId::from_code(code).expect("a book code")
    }

    #[test]
    fn john_three_sixteen_links_to_the_love_of_romans_five() {
        let links = of(book("JHN"), 3, 16);
        assert!(!links.is_empty(), "John 3:16 has related verses");
        // The card shows a handful, never the whole list: the source has up to ninety-seven.
        assert!(links.len() <= 8, "{} links is more than the card shows", links.len());
        assert_eq!(links[0].label(), "Romans 5:8", "the best-voted link comes first");
        assert_eq!(links[0].votes, 992);
        assert!(links.windows(2).all(|w| w[0].votes >= w[1].votes), "best first, all the way down");
    }

    #[test]
    fn a_range_keeps_its_end_and_reads_as_a_range() {
        let links = of(book("JHN"), 3, 16);
        let range = links.iter().find(|l| l.book == book("1JN")).expect("1 John is linked");
        assert_eq!((range.chapter, range.verse, range.end_chapter, range.end_verse), (4, 9, 4, 10));
        assert_eq!(range.label(), "1 John 4:9-10");
    }

    #[test]
    fn a_cross_chapter_range_names_both_chapters() {
        // 1 Chronicles 1:1 is linked to Genesis 4:25–5:32.
        let links = of(book("1CH"), 1, 1);
        let link = links.iter().find(|l| l.book == BookId::GENESIS).expect("Genesis is linked");
        assert_eq!((link.chapter, link.verse, link.end_chapter, link.end_verse), (4, 25, 5, 32));
        assert_eq!(link.label(), "Genesis 4:25-5:32");
    }

    #[test]
    fn a_verse_with_no_links_has_none() {
        // Genesis 1:8 and 6:1 are among the verses whose every link the site's readers voted down,
        // so the card has nothing to offer for them.
        for (code, chapter, verse) in [("GEN", 1, 8), ("GEN", 6, 1)] {
            assert!(!has(book(code), chapter, verse), "{code} {chapter}:{verse} has nothing to offer");
            assert!(of(book(code), chapter, verse).is_empty());
        }
    }

    #[test]
    fn no_link_carries_a_vote_the_readers_took_back() {
        // A link voted down is not offered at all, which the build already enforces.
        for (code, chapter, verse) in [("JHN", 3, 16), ("GEN", 1, 1), ("PSA", 23, 1)] {
            for link in of(book(code), chapter, verse) {
                assert!(link.votes > 0, "{} has {} votes", link.label(), link.votes);
            }
        }
    }

    #[test]
    fn the_psalms_are_named_in_the_singular() {
        let links = of(BookId::PSALMS, 23, 1);
        assert!(!links.is_empty(), "Psalm 23:1 has related verses");
        assert!(
            links.iter().all(|l| !l.label().starts_with("Psalms")),
            "a label says Psalm 23:2, not Psalms 23:2"
        );
    }

    #[test]
    fn the_credit_names_the_source_and_its_licence() {
        assert!(credit().contains("OpenBible.info"));
        assert!(credit().contains("CC BY 4.0"));
    }
}
