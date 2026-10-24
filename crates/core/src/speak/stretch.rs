//! Speakable text and the reading session: the words, not the sound.
//!
//! Everything here is platform-neutral: pure functions and state machines that a Mac, an iPhone or
//! a browser can call. Making audio is the frontend's job.

use std::f32::consts::PI;

/// Time-stretch PCM without changing pitch, so scripture can be read at a calmer pace than the
/// voice model's own 190 words per minute.
pub fn stretch(pcm: &[f32], rate: u32, tempo: f32) -> Vec<f32> {
    if pcm.is_empty() || (tempo - 1.0).abs() < 1e-3 {
        return pcm.to_vec();
    }
    let tempo = tempo.clamp(0.5, 2.0);
    let frame = ((0.040 * rate as f32) as usize) & !1;
    let hop = frame / 2;
    let seek = (0.010 * rate as f32) as usize;
    let window: Vec<f32> =
        (0..frame).map(|i| 0.5 - 0.5 * ((2.0 * PI * i as f32) / frame as f32).cos()).collect();
    let mut out: Vec<f32> = Vec::with_capacity((pcm.len() as f32 / tempo) as usize + frame);
    let mut in_pos = 0.0f64;
    let mut tail = vec![0.0f32; hop];
    let mut started = false;
    while (in_pos as usize) + frame + seek < pcm.len() {
        let base = in_pos as usize;
        let shift = if started { best_shift(&pcm[base..], &tail, seek) } else { 0 };
        let start = base + shift;
        let frame_samples: Vec<f32> = (0..frame).map(|i| pcm[start + i] * window[i]).collect();
        if started {
            let at = out.len() - hop;
            for i in 0..hop {
                out[at + i] += frame_samples[i];
            }
            out.extend_from_slice(&frame_samples[hop..]);
        } else {
            out.extend_from_slice(&frame_samples);
            started = true;
        }
        tail.copy_from_slice(&frame_samples[hop..]);
        in_pos += hop as f64 * tempo as f64;
    }
    out
}

/// The offset that best continues the previous frame, so overlapping frames stay in phase.
fn best_shift(input: &[f32], tail: &[f32], seek: usize) -> usize {
    let mut best = 0usize;
    let mut best_score = f32::NEG_INFINITY;
    for shift in 0..=seek.min(input.len().saturating_sub(tail.len())) {
        let mut score = 0.0f32;
        for i in 0..tail.len() {
            score += input[shift + i] * tail[i];
        }
        if score > best_score {
            best_score = score;
            best = shift;
        }
    }
    best
}
