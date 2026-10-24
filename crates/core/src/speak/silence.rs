//! Taking back the silence a voice leaves around what it says.
//!
//! A speech model does not stop when the last word stops: measured on the shipped voice, a sentence
//! trails about 0.8 s of quiet and opens with about 0.15 s. Every pause the reader asks for was
//! landing on top of that, so a breath chosen at 380 ms was heard as 1.4 s — too long to sit between
//! sentences, and impossible to tune while the model's own tail was in the way. Trimming first makes
//! the numbers mean what they say.
//!
//! Loudness is judged in 10 ms blocks by level, not sample by sample: speech crosses zero constantly,
//! and a single quiet sample inside a word is not a gap.

/// Mean level below which a 10 ms block counts as silence, in full scale.
const QUIET: f32 = 0.004;
/// The tail kept after the last sound. Long enough that a soft final consonant is not clipped short,
/// short enough not to be heard as a pause in its own right.
pub const KEEP_TAIL_MS: u32 = 60;
/// The block over which loudness is judged.
const BLOCK_MS: u32 = 10;

/// The speech without the silence around it, and with `KEEP_TAIL_MS` of room after the last sound.
pub fn trim_silence(pcm: &[f32], rate: u32) -> Vec<f32> {
    let block = ((rate as usize * BLOCK_MS as usize) / 1000).max(1);
    let blocks = pcm.len().div_ceil(block);
    let loud = |b: usize| {
        let lo = b * block;
        let hi = (lo + block).min(pcm.len());
        if hi <= lo {
            return false;
        }
        let level = (pcm[lo..hi].iter().map(|s| s * s).sum::<f32>() / (hi - lo) as f32).sqrt();
        level >= QUIET
    };
    let Some(first) = (0..blocks).find(|&b| loud(b)) else {
        return Vec::new();
    };
    let last = (first..blocks).rev().find(|&b| loud(b)).unwrap_or(first);
    let tail = (rate as usize * KEEP_TAIL_MS as usize) / 1000;
    let start = first * block;
    let end = ((last + 1) * block + tail).min(pcm.len());
    pcm[start..end].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 24_000;

    fn ms(n: u32) -> usize {
        (RATE as usize * n as usize) / 1000
    }

    fn tone(ms_long: u32) -> Vec<f32> {
        (0..ms(ms_long)).map(|i| (i as f32 * 0.05).sin() * 0.5).collect()
    }

    fn quiet(ms_long: u32) -> Vec<f32> {
        vec![0.0; ms(ms_long)]
    }

    /// Within a block of the mark: the trimmer judges loudness in blocks, not samples.
    fn near(actual: usize, expected: usize) {
        let slack = ms(BLOCK_MS) + ms(BLOCK_MS);
        assert!(
            actual.abs_diff(expected) <= slack,
            "trimmed to {actual} samples, expected about {expected}"
        );
    }

    #[test]
    fn the_silence_at_both_ends_is_taken_back() {
        let mut pcm = quiet(800);
        pcm.extend(tone(500));
        pcm.extend(quiet(800));
        let trimmed = trim_silence(&pcm, RATE);
        near(trimmed.len(), ms(500) + ms(KEEP_TAIL_MS));
    }

    #[test]
    fn a_sentence_that_ends_loudly_keeps_room_rather_than_being_clipped() {
        let sound = tone(300);
        let mut pcm = sound.clone();
        pcm.extend(quiet(400));
        let trimmed = trim_silence(&pcm, RATE);
        near(trimmed.len(), sound.len() + ms(KEEP_TAIL_MS));
        assert!(trimmed.len() > sound.len(), "room was kept after the last sound");
        assert_eq!(&trimmed[..sound.len()], &sound[..], "nothing of the sentence itself was touched");
    }

    #[test]
    fn silence_on_its_own_is_nothing_to_say() {
        assert!(trim_silence(&quiet(500), RATE).is_empty());
        assert!(trim_silence(&[], RATE).is_empty());
    }

    #[test]
    fn a_sound_right_at_the_end_is_not_lost() {
        let mut pcm = quiet(200);
        pcm.extend(tone(200));
        let trimmed = trim_silence(&pcm, RATE);
        // The buffer ends with the sound, so there is no room for a tail: nothing is invented.
        near(trimmed.len(), ms(200));
    }

    #[test]
    fn the_zero_crossings_inside_a_word_are_not_mistaken_for_a_gap() {
        // A low, slow tone crosses zero often — the flaw this test exists for.
        let mut slow: Vec<f32> = (0..ms(600)).map(|i| (i as f32 * 0.004).sin() * 0.3).collect();
        slow.extend(quiet(300));
        let trimmed = trim_silence(&slow, RATE);
        near(trimmed.len(), ms(600) + ms(KEEP_TAIL_MS));
    }
}
