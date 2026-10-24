//! The audio device: 24 kHz mono out to whatever speakers this machine has.

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// Plays mono `pcm` at `rate` through the default output device, returning when it has finished.
/// Called off the UI thread; a sample is a second or two long.
pub fn play(pcm: &[f32], rate: u32) -> Result<(), String> {
    let host = cpal::default_host();
    let device = host.default_output_device().ok_or("there is no audio output")?;
    let config = device.default_output_config().map_err(|e| e.to_string())?;
    let out_rate = config.sample_rate().0;
    let channels = config.channels() as usize;
    let format = config.sample_format();
    let buffer = resample(pcm, rate, out_rate, channels);
    let state = Arc::new(Mutex::new((buffer, 0usize)));
    let shared = state.clone();
    let on_error = |e| eprintln!("read aloud: audio stream error: {e}");
    let stream = match format {
        cpal::SampleFormat::F32 => device
            .build_output_stream(
                &config.into(),
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let mut guard = match shared.lock() {
                        Ok(guard) => guard,
                        Err(_) => return,
                    };
                    let (buffer, position) = &mut *guard;
                    for slot in data.iter_mut() {
                        *slot = match buffer.get(*position) {
                            Some(sample) => {
                                *position += 1;
                                *sample
                            }
                            None => 0.0,
                        };
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
        let finished = match state.lock() {
            Ok(guard) => guard.1 >= guard.0.len(),
            Err(_) => true,
        };
        if finished {
            return Ok(());
        }
    }
}

/// Linear resampling to the device's rate, interleaved across its channels. The sample is speech,
/// so the difference from a windowed filter is inaudible and the code stays readable.
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
