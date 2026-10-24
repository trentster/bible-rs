//! The audio device: 24 kHz mono out to whatever speakers this machine has.
//!
//! Playback has to obey the reader — pause now, stop now — so the device asks shared flags on every
//! callback rather than being handed a buffer and forgotten. The mixing itself is a pure function,
//! so pausing, stopping and finishing are tested without an audio device.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// What the reader can do to a voice while it is speaking.
#[derive(Default)]
pub struct Controls {
    paused: AtomicBool,
    stopped: AtomicBool,
    /// Microseconds of the voice that have actually reached the speakers. It stops while paused and
    /// never rewinds, so anything counting down from it can only go one way.
    heard_micros: AtomicU64,
}

impl Controls {
    /// How much of the voice has been heard, in seconds.
    pub fn heard_seconds(&self) -> f32 {
        self.heard_micros.load(Ordering::Relaxed) as f32 / 1_000_000.0
    }

    pub fn paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }

    pub fn stopped(&self) -> bool {
        self.stopped.load(Ordering::Relaxed)
    }

    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Relaxed);
    }

    /// Clears a stop so the next utterance can be played.
    pub fn resume_after_stop(&self) {
        self.stopped.store(false, Ordering::Relaxed);
    }
}

/// Fills `out` from `samples` starting at `position`, obeying pause and stop.
///
/// Paused: the output is silence and the position does not move, so the words resume where they
/// were. Stopped: silence, and the position jumps to the end so the caller sees it as finished.
pub fn pump(samples: &[f32], out: &mut [f32], position: usize, paused: bool, stopped: bool) -> usize {
    if stopped {
        out.fill(0.0);
        return samples.len();
    }
    if paused {
        out.fill(0.0);
        return position;
    }
    let mut at = position;
    for slot in out.iter_mut() {
        *slot = match samples.get(at) {
            Some(sample) => {
                at += 1;
                *sample
            }
            None => 0.0,
        };
    }
    at
}

/// Plays mono `pcm` at `rate` through the default output device until the audio ends, the reader
/// pauses it for a while, or the reader stops it. Returns how many seconds were actually heard.
pub fn play(pcm: &Arc<Vec<f32>>, rate: u32, controls: &Arc<Controls>) -> Result<f32, String> {
    let host = cpal::default_host();
    let device = host.default_output_device().ok_or("there is no audio output")?;
    let config = device.default_output_config().map_err(|e| e.to_string())?;
    let out_rate = config.sample_rate().0;
    let channels = config.channels() as usize;
    let format = config.sample_format();
    let buffer = Arc::new(resample(pcm, rate, out_rate, channels));
    let position = Arc::new(AtomicUsize::new(0));
    let heard = Arc::new(AtomicUsize::new(0));

    let audio = buffer.clone();
    let at = position.clone();
    let played = heard.clone();
    let obey = controls.clone();
    let on_error = |e| eprintln!("read aloud: audio stream error: {e}");
    let stream = match format {
        cpal::SampleFormat::F32 => device
            .build_output_stream(
                &config.into(),
                move |out: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let before = at.load(Ordering::Relaxed);
                    let after = pump(&audio, out, before, obey.paused(), obey.stopped());
                    if after > before {
                        let frames = (after - before) / channels.max(1);
                        played.fetch_add(frames, Ordering::Relaxed);
                        obey.heard_micros
                            .fetch_add((frames as f64 * 1e6 / out_rate as f64) as u64, Ordering::Relaxed);
                    }
                    if after != before {
                        at.store(after, Ordering::Relaxed);
                    }
                },
                on_error,
                None,
            )
            .map_err(|e| e.to_string())?,
        other => return Err(format!("the audio device wants {other:?}, which is not handled yet")),
    };
    stream.play().map_err(|e| e.to_string())?;
    loop {
        std::thread::sleep(std::time::Duration::from_millis(40));
        if controls.stopped() || position.load(Ordering::Relaxed) >= buffer.len() {
            break;
        }
    }
    if !controls.stopped() {
        // Let the last buffer reach the speakers rather than clipping the final word.
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    drop(stream);
    Ok(heard.load(Ordering::Relaxed) as f32 / out_rate as f32)
}

/// Linear resampling to the device's rate, interleaved across its channels. The audio is speech, so
/// the difference from a windowed filter is inaudible and the code stays readable.
fn resample(pcm: &[f32], from: u32, to: u32, channels: usize) -> Vec<f32> {
    if from == to {
        return pcm.iter().flat_map(|s| std::iter::repeat_n(*s, channels)).collect();
    }
    let ratio = from as f64 / to as f64;
    let frames = (pcm.len() as f64 / ratio) as usize;
    let mut out = Vec::with_capacity(frames * channels);
    for frame in 0..frames {
        let at = frame as f64 * ratio;
        let i = at.floor() as usize;
        let frac = (at - i as f64) as f32;
        let a = pcm.get(i).copied().unwrap_or(0.0);
        let b = pcm.get(i + 1).copied().unwrap_or(a);
        let value = a + (b - a) * frac;
        out.extend(std::iter::repeat_n(value, channels));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::pump;

    #[test]
    fn a_playing_voice_moves_through_the_audio() {
        let samples: Vec<f32> = (0..8).map(|i| i as f32).collect();
        let mut out = vec![0.0; 4];
        let after = pump(&samples, &mut out, 2, false, false);
        assert_eq!(out, vec![2.0, 3.0, 4.0, 5.0]);
        assert_eq!(after, 6);
    }

    #[test]
    fn a_paused_voice_keeps_its_place() {
        let samples: Vec<f32> = (0..8).map(|i| i as f32).collect();
        let mut out = vec![1.0; 4];
        let after = pump(&samples, &mut out, 3, true, false);
        assert_eq!(out, vec![0.0; 4], "a paused voice is silent");
        assert_eq!(after, 3, "and does not move");
    }

    #[test]
    fn a_stopped_voice_looks_finished() {
        let samples: Vec<f32> = (0..8).map(|i| i as f32).collect();
        let mut out = vec![1.0; 4];
        let after = pump(&samples, &mut out, 2, false, true);
        assert_eq!(out, vec![0.0; 4]);
        assert_eq!(after, samples.len());
    }

    #[test]
    fn the_end_of_the_audio_is_silence_not_a_loop() {
        let samples = vec![0.5f32; 2];
        let mut out = vec![1.0; 4];
        let after = pump(&samples, &mut out, 1, false, false);
        assert_eq!(out, vec![0.5, 0.0, 0.0, 0.0]);
        assert_eq!(after, 2);
    }
}
