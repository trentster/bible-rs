//! Read Aloud: what to say and how a session runs. Audio lives in the frontend.

mod session;
mod stretch;
mod text;
mod voices;

pub use session::{Session, Sleep};
pub use stretch::stretch;
pub use text::{chapter, speakable, words, Utterance, ANNOUNCE_PAUSE_MS, VERSE_PAUSE_MS};
pub use voices::{Accent, Voice, SPEECH_WPM, VOICES};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slowing_keeps_every_sample_and_stretches_the_time() {
        let rate = 24_000;
        let pcm: Vec<f32> = (0..rate as usize).map(|i| (i as f32 * 0.01).sin()).collect();
        let slower = stretch(&pcm, rate, 0.85);
        assert!(slower.len() > pcm.len());
        assert!((slower.len() as f32 / pcm.len() as f32 - 1.0 / 0.85).abs() < 0.05);
        assert!(slower.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn a_tempo_of_one_changes_nothing() {
        let pcm: Vec<f32> = (0..1000).map(|i| i as f32).collect();
        assert_eq!(stretch(&pcm, 24_000, 1.0), pcm);
    }

    #[test]
    fn silence_stays_silent() {
        let pcm = vec![0.0f32; 48_000];
        let out = stretch(&pcm, 24_000, 0.85);
        assert!(out.iter().all(|s| s.abs() < 1e-6), "silence picked up a tone");
    }

    #[test]
    fn the_lord_is_read_as_lord() {
        let said = speakable("The LORD is my shepherd; I shall not want.");
        assert_eq!(said, "The Lord is my shepherd; I shall not want.");
        assert_eq!(speakable("saith the LORD of hosts"), "saith the Lord of hosts");
    }

    #[test]
    fn a_verse_number_that_slips_through_is_not_spoken() {
        assert_eq!(speakable(" 1 For this Melchisedec, king of Salem"), "For this Melchisedec, king of Salem");
    }

    #[test]
    fn dashes_become_a_pause_rather_than_a_word() {
        let said = speakable("the same\u{2014}and not without");
        assert!(!said.contains('\u{2014}'), "em dash survived: {said}");
        assert_eq!(said, "the same, and not without");
    }

    #[test]
    fn digits_are_spoken_as_words() {
        assert_eq!(words(7), "seven");
        assert_eq!(words(23), "twenty-three");
        assert_eq!(words(119), "one hundred nineteen");
        assert_eq!(speakable("the 23rd Psalm"), "the twenty-third Psalm");
        assert_eq!(speakable("the 1st day"), "the first day");
        assert_eq!(speakable("the 12th day"), "the twelfth day");
    }

    #[test]
    fn a_chapter_opens_with_its_announcement() {
        let verses = vec![(1, "For this Melchisedec".to_string()), (2, "To whom also".to_string())];
        let script = chapter("Hebrews", 7, &verses);
        assert_eq!(script[0].text, "Hebrews, chapter seven");
        assert_eq!(script[0].pause_ms, ANNOUNCE_PAUSE_MS);
        assert_eq!(script.len(), 3);
        assert_eq!(script[1].text, "For this Melchisedec");
        assert_eq!(script[1].pause_ms, VERSE_PAUSE_MS);
    }

    #[test]
    fn an_empty_verse_is_skipped_rather_than_spoken() {
        let verses = vec![(1, "   ".to_string()), (2, "To whom also".to_string())];
        let script = chapter("Hebrews", 7, &verses);
        assert_eq!(script.len(), 2);
        assert_eq!(script[1].text, "To whom also");
    }

    #[test]
    fn a_session_reads_the_announcement_then_every_verse() {
        let verses = vec![(1, "For this Melchisedec".to_string()), (2, "To whom also".to_string())];
        let mut session = Session::new("Hebrews", 7, &verses, Sleep::Off);
        assert!(session.next().is_none(), "nothing is said before it plays");
        session.play();
        assert_eq!(session.next().unwrap().text, "Hebrews, chapter seven");
        assert_eq!(session.next().unwrap().verse, Some(1));
        assert_eq!(session.next().unwrap().verse, Some(2));
        assert!(session.next().is_none());
        assert!(session.finished());
    }

    #[test]
    fn a_paused_session_says_nothing_further() {
        let verses = vec![(1, "one".to_string())];
        let mut session = Session::new("Psalms", 23, &verses, Sleep::Off);
        session.play();
        session.next();
        session.pause();
        assert!(session.next().is_none());
        assert_eq!(session.position(), 1, "one of the two utterances was handed out");
        assert!(!session.finished());
    }

    #[test]
    fn a_session_can_start_where_the_reader_left_off() {
        let verses = vec![(1, "one".to_string()), (2, "two".to_string()), (3, "three".to_string())];
        let mut session = Session::new("Psalms", 23, &verses, Sleep::Off);
        session.seek_to_verse(3);
        session.play();
        assert_eq!(session.next().unwrap().verse, Some(3));
    }

    #[test]
    fn stopping_goes_back_to_the_top() {
        let verses = vec![(1, "one".to_string())];
        let mut session = Session::new("Psalms", 23, &verses, Sleep::Minutes(15));
        session.play();
        session.next();
        session.stop();
        assert_eq!(session.position(), 0);
        assert!(!session.playing());
    }

    #[test]
    fn every_voice_lands_on_the_same_pace() {
        for voice in VOICES {
            let landed = voice.native_wpm as f32 * voice.tempo();
            let error = (landed - SPEECH_WPM).abs() / SPEECH_WPM;
            assert!(error < 0.01, "{} lands on {landed:.1} wpm", voice.name);
        }
    }

    #[test]
    fn an_american_device_gets_the_american_man() {
        assert_eq!(Voice::for_locale("en_US").name, "Russell");
        assert_eq!(Voice::for_locale("en-us").name, "Russell");
        assert_eq!(Voice::for_locale("en_GB").name, "Spencer");
        assert_eq!(Voice::for_locale("en_AU").name, "Spencer");
        assert_eq!(Voice::for_locale("de_DE").name, "Spencer");
        assert_eq!(Voice::by_name("harper").map(|v| v.female), Some(true));
        assert_eq!(Voice::by_name("nobody"), None);
    }
}
