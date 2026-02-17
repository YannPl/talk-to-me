use anyhow::Result;
use rubato::{SincFixedIn, SincInterpolationType, SincInterpolationParameters, WindowFunction, Resampler};

/// Resample audio from one sample rate to another
pub fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Result<Vec<f32>> {
    if from_rate == to_rate {
        return Ok(samples.to_vec());
    }

    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };

    let ratio = to_rate as f64 / from_rate as f64;
    let mut resampler = SincFixedIn::<f32>::new(
        ratio,
        2.0,
        params,
        samples.len(),
        1, // mono
    )?;

    let input = vec![samples.to_vec()];
    let output = resampler.process(&input, None)?;

    Ok(output.into_iter().next().unwrap_or_default())
}

/// Normalize audio samples to [-1.0, 1.0] range
pub fn normalize(samples: &mut [f32]) {
    let max_val = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    if max_val > 0.0 && max_val != 1.0 {
        for sample in samples.iter_mut() {
            *sample /= max_val;
        }
    }
}

// ─── Mel Spectrogram for NeMo / Parakeet models ───

use std::f32::consts::PI;

/// Configuration for mel spectrogram extraction (matching NeMo defaults)
#[derive(Debug, Clone)]
pub struct MelConfig {
    pub sample_rate: u32,
    pub n_fft: usize,
    pub hop_length: usize,
    pub win_length: usize,
    pub n_mels: usize,
    pub fmin: f32,
    pub fmax: f32,
    /// Whether to apply log() to mel energies
    pub log_scale: bool,
    /// Whether to normalize per-feature (zero mean, unit variance)
    pub normalize_per_feature: bool,
}

impl Default for MelConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16000,
            n_fft: 512,
            hop_length: 160,
            win_length: 400,  // 0.025s * 16000 (NeMo default)
            n_mels: 80,
            fmin: 0.0,
            fmax: 0.0, // 0.0 = sample_rate / 2
            log_scale: true,
            normalize_per_feature: true,
        }
    }
}

/// Compute mel spectrogram from audio samples.
/// Returns a 2D array of shape [n_mels, n_frames] in row-major order.
pub fn mel_spectrogram(samples: &[f32], config: &MelConfig) -> Vec<f32> {
    let fmax = if config.fmax <= 0.0 {
        config.sample_rate as f32 / 2.0
    } else {
        config.fmax
    };

    // Build mel filterbank
    let n_freq_bins = config.n_fft / 2 + 1;
    let mel_bank = build_mel_filterbank(config.n_mels, n_freq_bins, config.sample_rate as f32, config.fmin, fmax);

    // Build Hann window
    let window = hann_window(config.win_length);

    // Pad signal with reflection at the edges
    let pad_len = config.n_fft / 2;
    let padded = reflect_pad(samples, pad_len);

    // Compute STFT frames
    let n_frames = if padded.len() >= config.n_fft {
        (padded.len() - config.n_fft) / config.hop_length + 1
    } else {
        0
    };

    // Pre-allocate output: [n_mels, n_frames]
    let mut mel_spec = vec![0.0f32; config.n_mels * n_frames];

    // Scratch buffer for FFT
    let mut fft_buf = vec![0.0f32; config.n_fft * 2]; // interleaved [re, im]

    for frame_idx in 0..n_frames {
        let start = frame_idx * config.hop_length;

        // Apply window and zero-pad
        for i in 0..config.n_fft * 2 {
            fft_buf[i] = 0.0;
        }
        for i in 0..config.win_length.min(config.n_fft) {
            let sample = if start + i < padded.len() { padded[start + i] } else { 0.0 };
            fft_buf[i * 2] = sample * window[i]; // real part
            // imag part stays 0
        }

        // In-place FFT (radix-2 Cooley-Tukey)
        fft_in_place(&mut fft_buf, config.n_fft);

        // Compute power spectrum and apply mel filterbank
        for mel_idx in 0..config.n_mels {
            let mut energy = 0.0f32;
            for k in 0..n_freq_bins {
                let weight = mel_bank[mel_idx * n_freq_bins + k];
                if weight > 0.0 {
                    let re = fft_buf[k * 2];
                    let im = fft_buf[k * 2 + 1];
                    let power = re * re + im * im;
                    energy += weight * power;
                }
            }
            mel_spec[mel_idx * n_frames + frame_idx] = energy;
        }
    }

    // Apply log scale
    if config.log_scale {
        let floor = 1e-10f32;
        for v in mel_spec.iter_mut() {
            *v = (*v + floor).ln();
        }
    }

    // Per-feature normalization (zero mean, unit variance across time)
    if config.normalize_per_feature && n_frames > 1 {
        for mel_idx in 0..config.n_mels {
            let row_start = mel_idx * n_frames;
            let row = &mut mel_spec[row_start..row_start + n_frames];

            let mean: f32 = row.iter().sum::<f32>() / n_frames as f32;
            let variance: f32 = row.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / n_frames as f32;
            let std_dev = variance.sqrt().max(1e-5);

            for v in row.iter_mut() {
                *v = (*v - mean) / std_dev;
            }
        }
    }

    mel_spec
}

/// Return the number of frames for given sample count and config
pub fn mel_num_frames(num_samples: usize, config: &MelConfig) -> usize {
    let pad_len = config.n_fft / 2;
    let padded_len = num_samples + 2 * pad_len;
    if padded_len >= config.n_fft {
        (padded_len - config.n_fft) / config.hop_length + 1
    } else {
        0
    }
}

// ─── Internal helpers ───

/// Build a mel filterbank matrix [n_mels, n_freq_bins]
fn build_mel_filterbank(n_mels: usize, n_freq_bins: usize, sample_rate: f32, fmin: f32, fmax: f32) -> Vec<f32> {
    let mut bank = vec![0.0f32; n_mels * n_freq_bins];

    // Convert Hz to mel scale (HTK formula)
    let hz_to_mel = |hz: f32| -> f32 { 2595.0 * (1.0 + hz / 700.0).log10() };
    let mel_to_hz = |mel: f32| -> f32 { 700.0 * (10.0f32.powf(mel / 2595.0) - 1.0) };

    let mel_min = hz_to_mel(fmin);
    let mel_max = hz_to_mel(fmax);

    // n_mels + 2 equally spaced points in mel scale
    let mel_points: Vec<f32> = (0..=(n_mels + 1))
        .map(|i| mel_min + (mel_max - mel_min) * i as f32 / (n_mels + 1) as f32)
        .collect();

    let hz_points: Vec<f32> = mel_points.iter().map(|&m| mel_to_hz(m)).collect();

    // Convert to FFT bin indices
    let bin_points: Vec<f32> = hz_points
        .iter()
        .map(|&hz| hz * (n_freq_bins as f32 - 1.0) * 2.0 / sample_rate)
        .collect();

    for m in 0..n_mels {
        let f_left = bin_points[m];
        let f_center = bin_points[m + 1];
        let f_right = bin_points[m + 2];

        for k in 0..n_freq_bins {
            let kf = k as f32;
            let weight = if kf >= f_left && kf <= f_center {
                if (f_center - f_left).abs() < 1e-6 { 0.0 } else { (kf - f_left) / (f_center - f_left) }
            } else if kf > f_center && kf <= f_right {
                if (f_right - f_center).abs() < 1e-6 { 0.0 } else { (f_right - kf) / (f_right - f_center) }
            } else {
                0.0
            };
            bank[m * n_freq_bins + k] = weight;
        }
    }

    bank
}

/// Generate a Hann window
fn hann_window(length: usize) -> Vec<f32> {
    (0..length)
        .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / length as f32).cos()))
        .collect()
}

/// Reflect-pad a signal (numpy-style "reflect" mode)
fn reflect_pad(samples: &[f32], pad_len: usize) -> Vec<f32> {
    let n = samples.len();
    if n == 0 {
        return vec![0.0; pad_len * 2];
    }

    let mut padded = Vec::with_capacity(n + 2 * pad_len);

    // Left reflection
    for i in (1..=pad_len).rev() {
        let idx = if i < n { i } else { n - 1 };
        padded.push(samples[idx]);
    }

    // Original signal
    padded.extend_from_slice(samples);

    // Right reflection
    for i in 1..=pad_len {
        let idx = if n > i + 1 { n - 1 - i } else { 0 };
        padded.push(samples[idx]);
    }

    padded
}

/// In-place radix-2 Cooley-Tukey FFT
/// `buf` is interleaved [re0, im0, re1, im1, ...] with length 2*n
fn fft_in_place(buf: &mut [f32], n: usize) {
    assert!(n.is_power_of_two(), "FFT size must be power of 2");

    // Bit-reversal permutation
    let mut j = 0usize;
    for i in 0..n {
        if i < j {
            buf.swap(2 * i, 2 * j);
            buf.swap(2 * i + 1, 2 * j + 1);
        }
        let mut m = n >> 1;
        while m >= 1 && j >= m {
            j -= m;
            m >>= 1;
        }
        j += m;
    }

    // Butterfly stages
    let mut step = 1;
    while step < n {
        let half_step = step;
        step <<= 1;
        let angle = -PI / half_step as f32;
        let w_re = angle.cos();
        let w_im = angle.sin();

        let mut k = 0;
        while k < n {
            let mut tw_re = 1.0f32;
            let mut tw_im = 0.0f32;

            for m in 0..half_step {
                let i = k + m;
                let j = i + half_step;

                let tr = tw_re * buf[2 * j] - tw_im * buf[2 * j + 1];
                let ti = tw_re * buf[2 * j + 1] + tw_im * buf[2 * j];

                buf[2 * j] = buf[2 * i] - tr;
                buf[2 * j + 1] = buf[2 * i + 1] - ti;
                buf[2 * i] += tr;
                buf[2 * i + 1] += ti;

                let new_tw_re = tw_re * w_re - tw_im * w_im;
                let new_tw_im = tw_re * w_im + tw_im * w_re;
                tw_re = new_tw_re;
                tw_im = new_tw_im;
            }

            k += step;
        }
    }
}
