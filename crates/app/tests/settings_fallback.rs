//! A saved reading translation that this build no longer has falls back to the default one, and
//! the reader's study data keeps working. Lamplit used to offer online translations; a settings
//! file can still name one.

use std::sync::Arc;

use bible_core::books::{BookId, VerseRef};
use bible_core::study::{HighlightColor, StudyStore};
use bible_rs::app::{Settings, initialize};

#[test]
fn a_gone_translation_falls_back_and_keeps_the_study_data() {
    let dir = std::env::temp_dir().join(format!("lamplit-fallback-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let john = VerseRef::new(BookId(42), 3, 16);
    {
        let mut study = StudyStore::open(&dir.join("study.sqlite")).unwrap();
        study.set_highlight(john, Some(HighlightColor::Green)).unwrap();
        let mut settings = Settings::default();
        settings.translation = "esv".into();
        settings.parallel = vec!["esv".into()];
        settings.save(&mut study);
    }

    let init = initialize(dir.clone(), Arc::new(|| {})).expect("the app starts");
    assert_eq!(init.settings.translation, bible_core::bundled::DEFAULT_ID);
    assert!(init.library.contains(&init.settings.translation));
    assert_eq!(init.study.highlight(john), Some(HighlightColor::Green));

    let _ = std::fs::remove_dir_all(&dir);
}
