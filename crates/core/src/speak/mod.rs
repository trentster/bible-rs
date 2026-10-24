//! Read Aloud: what to say and how a session runs. Audio lives in the frontend.

mod stretch;

pub use stretch::stretch;

#[cfg(test)]
mod tests {
    use super::stretch;

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
}
