use anyhow::Result;
use rubato::{SincFixedIn, SincInterpolationType, SincInterpolationParameters, WindowFunction, Resampler};

#[derive(Debug, Clone)]
pub struct ChunkBoundary {
    pub start_sample: usize,
    pub end_sample: usize,
}

/// Splits audio into chunks at silence points for chunked transcription.
///
/// - `target_duration_s`: ideal chunk length (e.g. 28s)
/// - `search_window_s`: how far around each target cut to search for silence (e.g. 3s)
/// - `rms_window_ms`: RMS analysis window size in milliseconds (e.g. 100)
pub fn split_at_silence(
    samples: &[f32],
    sample_rate: u32,
    target_duration_s: f32,
    search_window_s: f32,
    rms_window_ms: f32,
) -> Vec<ChunkBoundary> {
    let total_samples = samples.len();
    let max_chunk_samples = ((target_duration_s + search_window_s) * sample_rate as f32) as usize;

    // If audio fits in a single chunk (with margin), return it whole
    if total_samples <= max_chunk_samples {
        return vec![ChunkBoundary { start_sample: 0, end_sample: total_samples }];
    }

    // Compute RMS per window across entire audio
    let rms_win_samples = ((rms_window_ms / 1000.0) * sample_rate as f32) as usize;
    let rms_win_samples = rms_win_samples.max(1);
    let num_windows = total_samples / rms_win_samples;
    let rms_values: Vec<f32> = (0..num_windows)
        .map(|i| {
            let start = i * rms_win_samples;
            let end = (start + rms_win_samples).min(total_samples);
            let sum_sq: f32 = samples[start..end].iter().map(|s| s * s).sum();
            (sum_sq / (end - start) as f32).sqrt()
        })
        .collect();

    let target_samples = (target_duration_s * sample_rate as f32) as usize;
    let search_samples = (search_window_s * sample_rate as f32) as usize;

    let mut chunks = Vec::new();
    let mut chunk_start: usize = 0;

    while chunk_start < total_samples {
        let remaining = total_samples - chunk_start;
        if remaining <= max_chunk_samples {
            chunks.push(ChunkBoundary { start_sample: chunk_start, end_sample: total_samples });
            break;
        }

        let ideal_cut = chunk_start + target_samples;
        let search_lo = ideal_cut.saturating_sub(search_samples);
        let search_hi = (ideal_cut + search_samples).min(total_samples);

        // Find the RMS window index range for our search region
        let win_lo = search_lo / rms_win_samples;
        let win_hi = (search_hi / rms_win_samples).min(num_windows);

        let mut best_win = win_lo;
        let mut best_rms = f32::MAX;
        for w in win_lo..win_hi {
            if rms_values[w] < best_rms {
                best_rms = rms_values[w];
                best_win = w;
            }
        }

        // Cut at center of the quietest window
        let cut_sample = (best_win * rms_win_samples + rms_win_samples / 2).min(total_samples);

        chunks.push(ChunkBoundary { start_sample: chunk_start, end_sample: cut_sample });
        chunk_start = cut_sample;
    }

    chunks
}

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

pub fn normalize(samples: &mut [f32]) {
    let max_val = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    if max_val > 0.0 && max_val != 1.0 {
        for sample in samples.iter_mut() {
            *sample /= max_val;
        }
    }
}

use std::f32::consts::PI;

// NeMo-compatible mel spectrogram configuration
#[derive(Debug, Clone)]
pub struct MelConfig {
    pub sample_rate: u32,
    pub n_fft: usize,
    pub hop_length: usize,
    pub win_length: usize,
    pub n_mels: usize,
    pub fmin: f32,
    pub fmax: f32,
    pub log_scale: bool,
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

// Returns [n_mels, n_frames] in row-major order.
pub fn mel_spectrogram(samples: &[f32], config: &MelConfig) -> Vec<f32> {
    let fmax = if config.fmax <= 0.0 {
        config.sample_rate as f32 / 2.0
    } else {
        config.fmax
    };

    let n_freq_bins = config.n_fft / 2 + 1;
    let mel_bank = build_mel_filterbank(config.n_mels, n_freq_bins, config.sample_rate as f32, config.fmin, fmax);

    let window = hann_window(config.win_length);

    let pad_len = config.n_fft / 2;
    let padded = reflect_pad(samples, pad_len);

    let n_frames = if padded.len() >= config.n_fft {
        (padded.len() - config.n_fft) / config.hop_length + 1
    } else {
        0
    };

    let mut mel_spec = vec![0.0f32; config.n_mels * n_frames];
    let mut fft_buf = vec![0.0f32; config.n_fft * 2]; // interleaved [re, im]

    for frame_idx in 0..n_frames {
        let start = frame_idx * config.hop_length;

        for i in 0..config.n_fft * 2 {
            fft_buf[i] = 0.0;
        }
        for i in 0..config.win_length.min(config.n_fft) {
            let sample = if start + i < padded.len() { padded[start + i] } else { 0.0 };
            fft_buf[i * 2] = sample * window[i];
        }

        fft_in_place(&mut fft_buf, config.n_fft);

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

    if config.log_scale {
        let floor = 1e-10f32;
        for v in mel_spec.iter_mut() {
            *v = (*v + floor).ln();
        }
    }

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

pub fn mel_num_frames(num_samples: usize, config: &MelConfig) -> usize {
    let pad_len = config.n_fft / 2;
    let padded_len = num_samples + 2 * pad_len;
    if padded_len >= config.n_fft {
        (padded_len - config.n_fft) / config.hop_length + 1
    } else {
        0
    }
}

fn build_mel_filterbank(n_mels: usize, n_freq_bins: usize, sample_rate: f32, fmin: f32, fmax: f32) -> Vec<f32> {
    let mut bank = vec![0.0f32; n_mels * n_freq_bins];

    // Convert Hz to mel scale (HTK formula)
    let hz_to_mel = |hz: f32| -> f32 { 2595.0 * (1.0 + hz / 700.0).log10() };
    let mel_to_hz = |mel: f32| -> f32 { 700.0 * (10.0f32.powf(mel / 2595.0) - 1.0) };

    let mel_min = hz_to_mel(fmin);
    let mel_max = hz_to_mel(fmax);

    let mel_points: Vec<f32> = (0..=(n_mels + 1))
        .map(|i| mel_min + (mel_max - mel_min) * i as f32 / (n_mels + 1) as f32)
        .collect();

    let hz_points: Vec<f32> = mel_points.iter().map(|&m| mel_to_hz(m)).collect();

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

fn hann_window(length: usize) -> Vec<f32> {
    (0..length)
        .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / length as f32).cos()))
        .collect()
}

fn reflect_pad(samples: &[f32], pad_len: usize) -> Vec<f32> {
    let n = samples.len();
    if n == 0 {
        return vec![0.0; pad_len * 2];
    }

    let mut padded = Vec::with_capacity(n + 2 * pad_len);

    for i in (1..=pad_len).rev() {
        let idx = if i < n { i } else { n - 1 };
        padded.push(samples[idx]);
    }

    padded.extend_from_slice(samples);

    for i in 1..=pad_len {
        let idx = if n > i + 1 { n - 1 - i } else { 0 };
        padded.push(samples[idx]);
    }

    padded
}

// In-place radix-2 Cooley-Tukey FFT. buf is interleaved [re, im, ...] with length 2*n.
fn fft_in_place(buf: &mut [f32], n: usize) {
    assert!(n.is_power_of_two(), "FFT size must be power of 2");

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
