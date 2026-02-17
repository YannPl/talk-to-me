use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use anyhow::{Result, Context};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::engine::AudioBuffer;

const TARGET_SAMPLE_RATE: u32 = 16000;

pub struct AudioCapture {
    samples: Arc<Mutex<Vec<f32>>>,
    is_recording: Arc<AtomicBool>,
    stream: Option<cpal::Stream>,
    device_sample_rate: u32,
}

// Safety: cpal::Stream on macOS wraps a CoreAudio AudioUnit which is thread-safe.
// AudioCapture is always accessed behind a Mutex in AppState, so concurrent access
// to the stream is impossible.
unsafe impl Send for AudioCapture {}
unsafe impl Sync for AudioCapture {}

impl AudioCapture {
    pub fn new() -> Result<Self> {
        Ok(Self {
            samples: Arc::new(Mutex::new(Vec::new())),
            is_recording: Arc::new(AtomicBool::new(false)),
            stream: None,
            device_sample_rate: TARGET_SAMPLE_RATE,
        })
    }

    pub fn start(&mut self) -> Result<()> {
        let host = cpal::default_host();
        let device = host.default_input_device()
            .context("No input device available")?;

        let config = device.default_input_config()
            .context("Failed to get default input config")?;

        self.device_sample_rate = config.sample_rate().0;

        let samples = Arc::clone(&self.samples);
        let is_recording = Arc::clone(&self.is_recording);

        samples.lock().unwrap().clear();
        is_recording.store(true, Ordering::SeqCst);

        let stream_config: cpal::StreamConfig = config.into();
        let channels = stream_config.channels as usize;

        let stream = device.build_input_stream(
            &stream_config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if is_recording.load(Ordering::SeqCst) {
                    let mono: Vec<f32> = data.iter().step_by(channels).copied().collect();
                    samples.lock().unwrap().extend_from_slice(&mono);
                }
            },
            |err| {
                tracing::error!("Audio capture error: {}", err);
            },
            None,
        ).context("Failed to build input stream")?;

        stream.play().context("Failed to start audio stream")?;
        self.stream = Some(stream);

        tracing::info!("Audio capture started (device sample rate: {}Hz)", self.device_sample_rate);
        Ok(())
    }

    pub fn stop(&mut self) -> Result<AudioBuffer> {
        self.is_recording.store(false, Ordering::SeqCst);

        self.stream = None;

        let raw_samples = {
            let mut guard = self.samples.lock().unwrap();
            std::mem::take(&mut *guard)
        };

        tracing::info!("Audio capture stopped: {} samples at {}Hz", raw_samples.len(), self.device_sample_rate);

        let (samples, sample_rate) = if self.device_sample_rate != TARGET_SAMPLE_RATE {
            let resampled = super::processing::resample(
                &raw_samples,
                self.device_sample_rate,
                TARGET_SAMPLE_RATE,
            )?;
            (resampled, TARGET_SAMPLE_RATE)
        } else {
            (raw_samples, self.device_sample_rate)
        };

        Ok(AudioBuffer {
            samples,
            sample_rate,
            channels: 1,
        })
    }

    pub fn current_level(&self) -> f32 {
        let guard = self.samples.lock().unwrap();
        if guard.is_empty() {
            return 0.0;
        }
        // RMS of last 1600 samples (~100ms at 16kHz)
        let window_size = 1600.min(guard.len());
        let start = guard.len() - window_size;
        let rms: f32 = guard[start..].iter().map(|s| s * s).sum::<f32>() / window_size as f32;
        rms.sqrt().min(1.0)
    }

    pub fn is_recording(&self) -> bool {
        self.is_recording.load(Ordering::SeqCst)
    }
}
