//! Read Aloud: what to say and how a session runs. Audio lives in the frontend.

mod session;
mod stretch;
mod text;
mod voices;

pub use session::{Session, Sleep};
pub use silence::{KEEP_TAIL_MS, LONGEST_GAP_MS, cap_gaps, trim_silence};
pub mod silence;
pub use stretch::stretch;
pub use text::{
    ANNOUNCE_PAUSE_MS, CLAUSE_PAUSE_MS, SENTENCE_PAUSE_MS, Utterance, chapter, narration, speakable, words,
};
pub use voices::{Accent, SPEECH_WPM, VOICES, Voice};

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
        assert_eq!(
            speakable(" 1 For this Melchisedec, king of Salem"),
            "For this Melchisedec, king of Salem"
        );
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
        assert_eq!(script[1].pause_ms, SENTENCE_PAUSE_MS);
    }

    #[test]
    fn an_empty_verse_is_skipped_rather_than_spoken() {
        let verses = vec![(1, "   ".to_string()), (2, "To whom also".to_string())];
        let script = chapter("Hebrews", 7, &verses);
        assert_eq!(script.len(), 2);
        assert_eq!(script[1].text, "To whom also");
    }

    #[test]
    #[ignore]
    fn measure_run_on_verses() {
        use crate::books::{BookId, ChapterIdx};
        for id in ["kjv", "web", "asv"] {
            let t = crate::bundled::load(id).unwrap();
            let (mut run_on, mut total) = (0u32, 0u32);
            for book in BookId::all() {
                for ch in 1..=book.chapters() {
                    let Some(c) = ChapterIdx::new(book, ch) else { continue };
                    for verse in t.verses_in(c) {
                        total += 1;
                        if super::text::runs_on(&verse.text) {
                            run_on += 1;
                        }
                    }
                }
            }
            println!("{id}: {run_on}/{total} verses run on = {:.1}%", 100.0 * run_on as f32 / total as f32);
            if id == "kjv" {
                let mut chains: Vec<(usize, usize)> = Vec::new(); // (verses, words)
                for book in BookId::all() {
                    for ch in 1..=book.chapters() {
                        let Some(c) = ChapterIdx::new(book, ch) else { continue };
                        let (mut nv, mut nw) = (0usize, 0usize);
                        for verse in t.verses_in(c) {
                            nv += 1;
                            nw += verse.text.split_whitespace().count();
                            if !super::text::runs_on(&verse.text) {
                                chains.push((nv, nw));
                                nv = 0;
                                nw = 0;
                            }
                        }
                        if nv > 0 {
                            chains.push((nv, nw));
                        }
                    }
                }
                let over3 = chains.iter().filter(|c| c.0 > 3).count();
                let over5 = chains.iter().filter(|c| c.0 > 5).count();
                let longest = chains.iter().max().unwrap();
                let words = |limit: usize| chains.iter().filter(|c| c.1 > limit).count();
                println!(
                    "sentences {} | over 3 verses {} ({:.1}%) | over 5 verses {} ({:.1}%) | longest {} verses, {} words | over 60 words {} | over 100 words {}",
                    chains.len(),
                    over3,
                    100.0 * over3 as f32 / chains.len() as f32,
                    over5,
                    100.0 * over5 as f32 / chains.len() as f32,
                    longest.0,
                    longest.1,
                    words(60),
                    words(100)
                );
            }
        }
    }

    #[test]
    fn a_sentence_that_crosses_a_verse_number_is_read_straight_through() {
        let verses = vec![
            (
                12,
                "Now when Jesus had heard that John was cast into prison, he departed into Galilee;"
                    .to_string(),
            ),
            (13, "And leaving Nazareth, he came and dwelt in Capernaum.".to_string()),
        ];
        let script = chapter("Matthew", 4, &verses);
        assert_eq!(script.len(), 2, "the announcement, then one sentence");
        assert_eq!(
            script[1].text,
            "Now when Jesus had heard that John was cast into prison, he departed into Galilee; And leaving Nazareth, he came and dwelt in Capernaum."
        );
        assert_eq!(script[1].verse, Some(12), "the verse it starts in says where the narrator is");
    }

    #[test]
    fn a_verse_that_ends_its_sentence_still_gets_its_breath() {
        let verses = vec![
            (1, "The first thing.".to_string()),
            (2, "A clause that runs on;".to_string()),
            (3, "and the sentence closes.".to_string()),
        ];
        let script = chapter("Psalms", 23, &verses);
        assert_eq!(script.len(), 3);
        assert_eq!(script[1].text, "The first thing.");
        assert_eq!(script[1].pause_ms, SENTENCE_PAUSE_MS);
        assert_eq!(script[2].text, "A clause that runs on; and the sentence closes.");
        assert_eq!(script[2].verse, Some(2));
    }

    #[test]
    fn a_sentence_too_long_to_hold_is_broken_with_a_clause_breath() {
        let verse = |n: u8| (n, format!("clause number {n} carries on;"));
        let verses: Vec<(u32, String)> = (1..=5u8)
            .map(|n| {
                let (n, t) = verse(n);
                (n as u32, t)
            })
            .collect();
        let script = chapter("Hebrews", 7, &verses);
        assert_eq!(script.len(), 3, "the announcement, three verses, then the last two");
        assert_eq!(script[1].verse, Some(1));
        assert_eq!(script[1].pause_ms, CLAUSE_PAUSE_MS, "cut short, so a clause breath");
        assert!(script[1].text.ends_with(';'));
        assert_eq!(script[2].verse, Some(4), "the narrator's position follows the break");
    }

    #[test]
    fn a_closing_quote_does_not_hide_the_colon() {
        let verses = vec![(1, "\u{201c}He said unto them:".to_string()), (2, "Follow me.".to_string())];
        let script = chapter("Matthew", 4, &verses);
        assert_eq!(script.len(), 2);
        assert_eq!(script[1].verse, Some(1));
    }

    #[test]
    fn a_session_reads_the_announcement_then_every_verse() {
        let verses = vec![(1, "For this Melchisedec".to_string()), (2, "To whom also".to_string())];
        let mut session = Session::new("Hebrews", 7, &verses, Sleep::Off);
        assert!(session.next_utterance().is_none(), "nothing is said before it plays");
        session.play();
        assert_eq!(session.next_utterance().unwrap().text, "Hebrews, chapter seven");
        assert_eq!(session.next_utterance().unwrap().verse, Some(1));
        assert_eq!(session.next_utterance().unwrap().verse, Some(2));
        assert!(session.next_utterance().is_none());
        assert!(session.finished());
    }

    #[test]
    fn a_paused_session_says_nothing_further() {
        let verses = vec![(1, "one".to_string())];
        let mut session = Session::new("Psalms", 23, &verses, Sleep::Off);
        session.play();
        session.next_utterance();
        session.pause();
        assert!(session.next_utterance().is_none());
        assert_eq!(session.position(), 1, "one of the two utterances was handed out");
        assert!(!session.finished());
    }

    #[test]
    fn a_session_can_start_where_the_reader_left_off() {
        let verses = vec![(1, "one".to_string()), (2, "two".to_string()), (3, "three".to_string())];
        let mut session = Session::new("Psalms", 23, &verses, Sleep::Off);
        session.seek_to_verse(3);
        session.play();
        assert_eq!(session.next_utterance().unwrap().verse, Some(3));
    }

    #[test]
    fn stopping_goes_back_to_the_top() {
        let verses = vec![(1, "one".to_string())];
        let mut session = Session::new("Psalms", 23, &verses, Sleep::Minutes(15));
        session.play();
        session.next_utterance();
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

    #[test]
    fn ye_is_said_yee_not_yay() {
        assert_eq!(narration("ye shall abide"), "yee shall abide");
        assert_eq!(narration("Ye are the salt"), "Yee are the salt");
    }

    #[test]
    fn words_that_merely_contain_ye_are_left_alone() {
        for word in ["yes", "eyes", "dye", "yearn", "obeyed"] {
            assert_eq!(narration(word), word, "{word} must survive untouched");
        }
    }

    #[test]
    fn the_old_words_are_spelt_as_they_are_said() {
        assert_eq!(narration("thus saith the Lord"), "thus seth the Lord");
        assert_eq!(narration("he doth and thou dost"), "he duth and thou dust");
        assert_eq!(narration("the shewbread"), "the showbread");
        assert_eq!(narration("she shewed them"), "she showed them");
    }

    #[test]
    fn yea_still_rhymes_with_yay() {
        assert_eq!(narration("yea, hath he said"), "yea, hath he said");
    }

    #[test]
    fn the_punctuation_and_the_apostrophes_are_kept() {
        assert_eq!(narration("ye, and ye."), "yee, and yee.");
        assert_eq!(narration("O ye of little faith"), "O yee of little faith");
    }

    #[test]
    fn bezaleel_is_said_bez_a_lail() {
        // The spelling the reader chose by ear: the narrator read "Bezaleel" its own way, and no
        // rule derives the right one, so it is written into the table.
        assert_eq!(narration("Bezaleel"), "Bezalayl");
        assert_eq!(narration("and Bezaleel the son of Uri"), "and Bezalayl the son of Uri");
    }
}
