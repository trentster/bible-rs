//! Study data outlives the translation it was made in.
//!
//! Lamplit used to offer online translations (ESV, NLT, NET, API.Bible). They are gone, so a study
//! file can name a translation this build no longer has. Everything the reader made — highlights,
//! notes, reading history — is keyed by canonical verse reference, not by translation, so it must
//! still be there, and the reading history must keep saying which translation was read.

use bible_core::books::{BookId, VerseRef};
use bible_core::study::{HighlightColor, ReadingSession, StudyStore};

fn john_3_16() -> VerseRef {
    VerseRef::new(BookId(42), 3, 16)
}

#[test]
fn highlights_and_notes_survive_a_translation_that_is_gone() {
    let dir = std::env::temp_dir().join(format!("lamplit-gone-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("study.sqlite");

    {
        let mut study = StudyStore::open(&path).unwrap();
        study.set_highlight(john_3_16(), Some(HighlightColor::Yellow)).unwrap();
        study.set_note(john_3_16(), "Made while reading the ESV.").unwrap();
        study
            .record_reading(&ReadingSession {
                started: 1_700_000_000,
                book: BookId(42),
                chapter: 3,
                translation: "esv".into(),
                seconds: 90,
                furthest: 17,
                verses: 36,
            })
            .unwrap();
    }

    // A later launch: the file is reopened by a build with no online translations at all.
    let study = StudyStore::open(&path).unwrap();
    assert_eq!(study.highlight(john_3_16()), Some(HighlightColor::Yellow));
    assert_eq!(study.note(john_3_16()).map(|n| n.body.as_str()), Some("Made while reading the ESV."));
    let history = study.reading_since(0);
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].translation, "esv");

    let _ = std::fs::remove_dir_all(&dir);
}
