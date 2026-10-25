//! Sound output: the machine's samples go to the default output device through cpal,
//! resampled from `p63_apu::SAMPLE_RATE` to whatever the device runs at.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

type Queue = Arc<Mutex<VecDeque<(i16, i16)>>>;

/// About one frame: playback waits for this much before starting or after running dry.
const PREFILL: usize = p63_apu::SAMPLE_RATE as usize / 60;
/// About two frames: the frame loop waits while more than this is queued.
pub const TARGET_QUEUED: usize = PREFILL * 2;
/// About eight frames; older samples are dropped past this (only if the loop stalls).
const MAX_QUEUED: usize = PREFILL * 8;

pub struct Speaker {
    queue: Queue,
    _stream: cpal::Stream,
}

impl Speaker {
    pub fn open() -> Result<Self, String> {
        let device = cpal::default_host().default_output_device().ok_or("no audio output device")?;
        let config = device.default_output_config().map_err(|e| e.to_string())?;
        let queue: Queue = Arc::default();
        let format = config.sample_format();
        let config = config.into();
        let stream = match format {
            cpal::SampleFormat::F32 => build::<f32>(&device, &config, queue.clone()),
            cpal::SampleFormat::I16 => build::<i16>(&device, &config, queue.clone()),
            cpal::SampleFormat::U16 => build::<u16>(&device, &config, queue.clone()),
            other => return Err(format!("unsupported sample format {other:?}")),
        }?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Self { queue, _stream: stream })
    }

    /// Samples waiting to be played.
    pub fn queued(&self) -> usize {
        self.queue.lock().map_or(0, |q| q.len())
    }

    pub fn push(&self, samples: &[(i16, i16)]) {
        if let Ok(mut q) = self.queue.lock() {
            q.extend(samples);
            let excess = q.len().saturating_sub(MAX_QUEUED);
            q.drain(..excess);
        }
    }
}

fn build<T>(device: &cpal::Device, config: &cpal::StreamConfig, queue: Queue) -> Result<cpal::Stream, String>
where
    T: cpal::SizedSample + cpal::FromSample<i16>,
{
    let channels = usize::from(config.channels);
    let step = f64::from(p63_apu::SAMPLE_RATE) / f64::from(config.sample_rate.0);
    let (mut phase, mut prev, mut next, mut primed) = (0.0f64, (0i16, 0i16), (0i16, 0i16), false);
    let fill = move |out: &mut [T], _: &cpal::OutputCallbackInfo| {
        let Ok(mut q) = queue.lock() else { return };
        for frame in out.chunks_mut(channels) {
            primed = primed || q.len() >= PREFILL;
            if primed {
                phase += step;
                while phase >= 1.0 {
                    phase -= 1.0;
                    prev = next;
                    match q.pop_front() {
                        Some(s) => next = s,
                        None => primed = false, // ran dry: hold the last sample and refill
                    }
                }
            }
            let lerp = |a: i16, b: i16| (f64::from(a) + (f64::from(b) - f64::from(a)) * phase) as i16;
            let (l, r) = (lerp(prev.0, next.0), lerp(prev.1, next.1));
            for (i, s) in frame.iter_mut().enumerate() {
                let v = match (channels, i) {
                    (1, _) => ((i32::from(l) + i32::from(r)) / 2) as i16,
                    (_, 0) => l,
                    (_, 1) => r,
                    _ => 0,
                };
                *s = T::from_sample(v);
            }
        }
    };
    device.build_output_stream(config, fill, |e| eprintln!("audio: {e}"), None).map_err(|e| e.to_string())
}

/// Writes 16-bit stereo PCM as a .wav file.
pub fn save_wav(path: &Path, samples: &[(i16, i16)]) -> Result<(), String> {
    let data_len = samples.len() as u32 * 4;
    let rate = p63_apu::SAMPLE_RATE;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend(b"RIFF");
    out.extend((36 + data_len).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes()); // PCM
    out.extend(2u16.to_le_bytes()); // stereo
    out.extend(rate.to_le_bytes());
    out.extend((rate * 4).to_le_bytes());
    out.extend(4u16.to_le_bytes());
    out.extend(16u16.to_le_bytes());
    out.extend(b"data");
    out.extend(data_len.to_le_bytes());
    for &(l, r) in samples {
        out.extend(l.to_le_bytes());
        out.extend(r.to_le_bytes());
    }
    std::fs::write(path, out).map_err(|e| format!("{}: {e}", path.display()))
}
