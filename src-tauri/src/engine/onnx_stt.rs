use std::path::Path;
use std::sync::Mutex;
use anyhow::{Result, Context};
use ort::session::Session;
use ort::value::Tensor;

use super::{Engine, SttEngine, ModelCapability, ModelInfo, AudioBuffer, TranscriptionResult};
use crate::audio::processing::{MelConfig, mel_spectrogram, mel_num_frames};

struct Vocabulary {
    tokens: Vec<String>,
    blank_id: usize,
    vocab_size: usize,
}

#[derive(Debug, Clone, PartialEq)]
enum ParakeetVariant {
    Ctc,
    Tdt,
}

pub struct OnnxSttEngine {
    encoder_session: Mutex<Option<Session>>,
    decoder_session: Mutex<Option<Session>>,
    vocabulary: Mutex<Option<Vocabulary>>,
    variant: Mutex<ParakeetVariant>,
    mel_config: Mutex<MelConfig>,
}

impl OnnxSttEngine {
    pub fn new() -> Self {
        Self {
            encoder_session: Mutex::new(None),
            decoder_session: Mutex::new(None),
            vocabulary: Mutex::new(None),
            variant: Mutex::new(ParakeetVariant::Ctc),
            mel_config: Mutex::new(MelConfig::default()),
        }
    }

    fn load_vocabulary_from_dir(model_dir: &Path) -> Result<Vocabulary> {
        let vocab_txt = model_dir.join("vocab.txt");
        let tokenizer_json = model_dir.join("tokenizer.json");

        if vocab_txt.exists() {
            Self::load_vocab_txt(&vocab_txt)
        } else if tokenizer_json.exists() {
            Self::load_tokenizer_json(&tokenizer_json)
        } else {
            anyhow::bail!("No vocab.txt or tokenizer.json found in {}", model_dir.display());
        }
    }

    fn load_vocab_txt(path: &Path) -> Result<Vocabulary> {
        let data = std::fs::read_to_string(path)
            .context("Failed to read vocab.txt")?;

        let mut pairs: Vec<(String, usize)> = Vec::new();

        for line in data.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(last_space) = line.rfind(' ') {
                let token = &line[..last_space];
                if let Ok(id) = line[last_space + 1..].parse::<usize>() {
                    pairs.push((token.to_string(), id));
                }
            }
        }

        if pairs.is_empty() {
            anyhow::bail!("vocab.txt is empty or has invalid format");
        }

        pairs.sort_by_key(|(_, id)| *id);

        let max_id = pairs.last().map(|(_, id)| *id).unwrap_or(0);
        let mut tokens = vec![String::new(); max_id + 1];
        for (token, id) in &pairs {
            if *id < tokens.len() {
                tokens[*id] = token.clone();
            }
        }

        // Blank token is the last in NeMo vocab
        let blank_id = tokens.len() - 1;

        // vocab_size = total number of token classes (including blank)
        // For TDT, the decoder outputs [vocab_size + num_durations] logits
        // where vocab_size includes the blank token
        let vocab_size = tokens.len();

        tracing::info!("Loaded vocab.txt: {} tokens, blank_id={}, vocab_size={}", tokens.len(), blank_id, vocab_size);
        Ok(Vocabulary { tokens, blank_id, vocab_size })
    }

    fn load_tokenizer_json(tokenizer_path: &Path) -> Result<Vocabulary> {
        let data = std::fs::read_to_string(tokenizer_path)
            .context("Failed to read tokenizer.json")?;
        let json: serde_json::Value = serde_json::from_str(&data)
            .context("Failed to parse tokenizer.json")?;

        let mut tokens: Vec<String> = Vec::new();

        if let Some(model) = json.get("model") {
            if let Some(vocab) = model.get("vocab") {
                if let Some(vocab_arr) = vocab.as_array() {
                    for entry in vocab_arr {
                        if let Some(s) = entry.as_str() {
                            tokens.push(s.to_string());
                        } else if let Some(pair) = entry.as_array() {
                            if let Some(s) = pair.first().and_then(|v| v.as_str()) {
                                tokens.push(s.to_string());
                            }
                        }
                    }
                } else if let Some(vocab_map) = vocab.as_object() {
                    let mut pairs: Vec<(String, usize)> = vocab_map.iter()
                        .filter_map(|(k, v)| v.as_u64().map(|id| (k.clone(), id as usize)))
                        .collect();
                    pairs.sort_by_key(|(_, id)| *id);
                    tokens = pairs.into_iter().map(|(k, _)| k).collect();
                }
            }
        }

        if tokens.is_empty() {
            if let Some(vocab) = json.get("vocab") {
                if let Some(vocab_map) = vocab.as_object() {
                    let mut pairs: Vec<(String, usize)> = vocab_map.iter()
                        .filter_map(|(k, v)| v.as_u64().map(|id| (k.clone(), id as usize)))
                        .collect();
                    pairs.sort_by_key(|(_, id)| *id);
                    tokens = pairs.into_iter().map(|(k, _)| k).collect();
                }
            }
        }

        if tokens.is_empty() {
            anyhow::bail!("Could not find vocabulary in tokenizer.json");
        }

        let blank_id = tokens.len() - 1;
        let vocab_size = tokens.len();

        tracing::info!("Loaded tokenizer.json: {} tokens, blank_id={}, vocab_size={}", tokens.len(), blank_id, vocab_size);
        Ok(Vocabulary { tokens, blank_id, vocab_size })
    }

    fn detect_variant(model_id: &str) -> ParakeetVariant {
        if model_id.contains("tdt") {
            ParakeetVariant::Tdt
        } else {
            ParakeetVariant::Ctc
        }
    }

    fn ctc_decode(logits: &[f32], time_steps: usize, vocab_size: usize, vocab: &Vocabulary) -> String {
        let mut prev_token: Option<usize> = None;
        let mut result_tokens: Vec<&str> = Vec::new();

        for t in 0..time_steps {
            let frame_start = t * vocab_size;
            let frame_end = frame_start + vocab_size;
            if frame_end > logits.len() { break; }
            let frame = &logits[frame_start..frame_end];

            let token_id = frame.iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(i, _)| i)
                .unwrap_or(vocab.blank_id);

            if token_id == vocab.blank_id {
                prev_token = None;
                continue;
            }

            if Some(token_id) == prev_token {
                continue;
            }

            prev_token = Some(token_id);

            if token_id < vocab.tokens.len() {
                result_tokens.push(&vocab.tokens[token_id]);
            }
        }

        let raw = result_tokens.join("");
        raw.replace('\u{2581}', " ").trim().to_string()
    }

    /// NeMo TDT transducer decoding: for each encoder time step, the decoder_joint
    /// predicts a token + duration (0-4 frames to skip). Non-blank tokens are emitted,
    /// LSTM state is only updated on non-blank emissions.
    fn tdt_decode(
        decoder_session: &mut Session,
        encoder_out: &[f32],      // flat [T', D] row-major (after transpose)
        encoded_length: usize,    // T' (number of encoder time steps)
        encoder_dim: usize,       // D (encoder output dimension)
        vocab: &Vocabulary,
    ) -> Result<String> {
        let max_tokens_per_step = 10;
        let num_tdt_durations = 5;

        // LSTM state dimensions — initial guess [2, 1, 640] based on Parakeet TDT 0.6B v3,
        // auto-detected from first decoder output
        let mut s1_dim0: usize = 2;
        let mut s1_dim2: usize = 640;
        let mut s2_dim0: usize = 2;
        let mut s2_dim2: usize = 640;
        let mut state_dims_detected = false;

        let mut state1 = vec![0.0f32; s1_dim0 * s1_dim2];
        let mut state2 = vec![0.0f32; s2_dim0 * s2_dim2];

        let mut result_tokens: Vec<String> = Vec::new();
        // NOTE: targets input expects int32, not int64
        let mut prev_token_id: i32 = vocab.blank_id as i32;
        let mut t: usize = 0;
        let mut emitted_this_step: usize = 0;

        while t < encoded_length {
            let frame_start = t * encoder_dim;
            let frame_end = frame_start + encoder_dim;
            if frame_end > encoder_out.len() { break; }
            let encoder_frame: Vec<f32> = encoder_out[frame_start..frame_end].to_vec();

            let enc_tensor = Tensor::from_array((
                vec![1i64, encoder_dim as i64, 1i64],
                encoder_frame,
            )).context("Failed to create encoder frame tensor")?;

            // targets and target_length must be int32 (per ONNX model spec)
            let targets_tensor = Tensor::from_array((
                vec![1i64, 1i64],
                vec![prev_token_id],  // i32
            )).context("Failed to create targets tensor")?;

            let target_length_tensor = Tensor::from_array((
                vec![1i64],
                vec![1i32],  // int32
            )).context("Failed to create target length tensor")?;

            let state1_tensor = Tensor::from_array((
                vec![s1_dim0 as i64, 1i64, s1_dim2 as i64],
                state1.clone(),
            )).context("Failed to create state1 tensor")?;

            let state2_tensor = Tensor::from_array((
                vec![s2_dim0 as i64, 1i64, s2_dim2 as i64],
                state2.clone(),
            )).context("Failed to create state2 tensor")?;

            if t == 0 {
                tracing::info!("TDT decoder first call: encoder_frame=[1, {}, 1], targets=[[{}]], state1=[{}, 1, {}], state2=[{}, 1, {}]",
                    encoder_dim, prev_token_id, s1_dim0, s1_dim2, s2_dim0, s2_dim2);

                let dec_input_names: Vec<String> = decoder_session.inputs().iter()
                    .map(|i| i.name().to_string()).collect();
                let dec_output_names: Vec<String> = decoder_session.outputs().iter()
                    .map(|o| o.name().to_string()).collect();
                tracing::info!("TDT decoder inputs: {:?}", dec_input_names);
                tracing::info!("TDT decoder outputs: {:?}", dec_output_names);
            }

            let outputs = match decoder_session.run(ort::inputs![
                "encoder_outputs" => enc_tensor,
                "targets" => targets_tensor,
                "target_length" => target_length_tensor,
                "input_states_1" => state1_tensor,
                "input_states_2" => state2_tensor,
            ]) {
                Ok(o) => o,
                Err(e) => {
                    tracing::error!("TDT decoder_joint inference failed at t={}: {}", t, e);
                    anyhow::bail!("TDT decoder_joint inference failed at t={}: {}", t, e);
                }
            };

            let logits_value = outputs.get("outputs")
                .context("No 'outputs' tensor from decoder_joint")?;
            let new_state1_value = outputs.get("output_states_1")
                .context("No 'output_states_1' tensor")?;
            let new_state2_value = outputs.get("output_states_2")
                .context("No 'output_states_2' tensor")?;

            let (_logits_shape, logits_data) = logits_value.try_extract_tensor::<f32>()
                .context("Failed to extract decoder logits")?;
            let (s1_shape, s1_data) = new_state1_value.try_extract_tensor::<f32>()
                .context("Failed to extract state1")?;
            let (s2_shape, s2_data) = new_state2_value.try_extract_tensor::<f32>()
                .context("Failed to extract state2")?;

            if !state_dims_detected {
                let s1_dims: Vec<usize> = s1_shape.iter().map(|&d| d as usize).collect();
                let s2_dims: Vec<usize> = s2_shape.iter().map(|&d| d as usize).collect();
                tracing::info!("TDT decoder output state1 shape: {:?}, state2 shape: {:?}", s1_dims, s2_dims);
                tracing::info!("TDT decoder logits size: {}, vocab_size: {}", logits_data.len(), vocab.vocab_size);

                if s1_dims.len() == 3 {
                    s1_dim0 = s1_dims[0];
                    s1_dim2 = s1_dims[2];
                }
                if s2_dims.len() == 3 {
                    s2_dim0 = s2_dims[0];
                    s2_dim2 = s2_dims[2];
                }
                state_dims_detected = true;
            }

            // Output: [vocab_size + num_tdt_durations] logits
            let token_logits = if logits_data.len() >= vocab.vocab_size {
                &logits_data[..vocab.vocab_size]
            } else {
                tracing::warn!("Decoder output too small: {} < vocab_size {}", logits_data.len(), vocab.vocab_size);
                break;
            };

            let duration_logits = if logits_data.len() >= vocab.vocab_size + num_tdt_durations {
                &logits_data[vocab.vocab_size..vocab.vocab_size + num_tdt_durations]
            } else {
                &logits_data[0..0]
            };

            let token_id = token_logits.iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(i, _)| i)
                .unwrap_or(vocab.blank_id);

            let step: usize = if !duration_logits.is_empty() {
                duration_logits.iter()
                    .enumerate()
                    .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            } else {
                0
            };

            if token_id != vocab.blank_id && token_id < vocab.tokens.len() {
                state1 = s1_data.to_vec();
                state2 = s2_data.to_vec();
                prev_token_id = token_id as i32;
                result_tokens.push(vocab.tokens[token_id].clone());
                emitted_this_step += 1;
            }

            if step > 0 {
                t += step;
                emitted_this_step = 0;
            } else if token_id == vocab.blank_id || emitted_this_step >= max_tokens_per_step {
                t += 1;
                emitted_this_step = 0;
            }
            // Otherwise (non-blank token with step=0), stay on same frame
        }

        let raw = result_tokens.join("");
        Ok(raw.replace('\u{2581}', " ").trim().to_string())
    }
}

impl Engine for OnnxSttEngine {
    fn load_model(&mut self, model_path: &Path, info: &ModelInfo) -> Result<()> {
        let model_dir = if model_path.is_dir() {
            model_path.to_path_buf()
        } else {
            model_path.parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| model_path.to_path_buf())
        };

        let variant = Self::detect_variant(&info.id);
        tracing::info!("Parakeet variant: {:?}", variant);

        match variant {
            ParakeetVariant::Ctc => {
                let onnx_path = model_dir.join("model.onnx");
                if !onnx_path.exists() {
                    anyhow::bail!("model.onnx not found in {}", model_dir.display());
                }

                tracing::info!("Loading CTC ONNX model from {}", onnx_path.display());
                let session = Session::builder()?
                    .with_intra_threads(4)?
                    .commit_from_file(&onnx_path)
                    .context("Failed to load CTC ONNX model")?;

                for input in session.inputs() {
                    tracing::info!("CTC input: {}", input.name());
                }
                for output in session.outputs() {
                    tracing::info!("CTC output: {}", output.name());
                }

                *self.encoder_session.lock().unwrap() = Some(session);
                *self.decoder_session.lock().unwrap() = None;
            }
            ParakeetVariant::Tdt => {
                let encoder_path = model_dir.join("encoder-model.onnx");
                let decoder_path = model_dir.join("decoder_joint-model.onnx");

                if !encoder_path.exists() {
                    anyhow::bail!("encoder-model.onnx not found in {}", model_dir.display());
                }
                if !decoder_path.exists() {
                    anyhow::bail!("decoder_joint-model.onnx not found in {}", model_dir.display());
                }

                tracing::info!("Loading TDT encoder from {}", encoder_path.display());
                let encoder = Session::builder()?
                    .with_intra_threads(4)?
                    .commit_from_file(&encoder_path)
                    .context("Failed to load TDT encoder")?;

                for input in encoder.inputs() {
                    tracing::info!("TDT encoder input: {}", input.name());
                }
                for output in encoder.outputs() {
                    tracing::info!("TDT encoder output: {}", output.name());
                }

                tracing::info!("Loading TDT decoder_joint from {}", decoder_path.display());
                let decoder = Session::builder()?
                    .with_intra_threads(4)?
                    .commit_from_file(&decoder_path)
                    .context("Failed to load TDT decoder_joint")?;

                for input in decoder.inputs() {
                    tracing::info!("TDT decoder input: {}", input.name());
                }
                for output in decoder.outputs() {
                    tracing::info!("TDT decoder output: {}", output.name());
                }

                *self.encoder_session.lock().unwrap() = Some(encoder);
                *self.decoder_session.lock().unwrap() = Some(decoder);
            }
        }

        let vocabulary = Self::load_vocabulary_from_dir(&model_dir)?;

        *self.vocabulary.lock().unwrap() = Some(vocabulary);
        *self.variant.lock().unwrap() = variant.clone();

        let mut mel_cfg = self.mel_config.lock().unwrap();
        mel_cfg.sample_rate = 16000;
        mel_cfg.n_fft = 512;
        mel_cfg.hop_length = 160;  // 0.01s * 16000
        mel_cfg.fmin = 0.0;
        mel_cfg.fmax = 0.0;
        mel_cfg.log_scale = true;
        mel_cfg.normalize_per_feature = true;

        match variant {
            ParakeetVariant::Ctc => {
                mel_cfg.n_mels = 80;
                mel_cfg.win_length = 400;  // 0.025s * 16000
            }
            ParakeetVariant::Tdt => {
                mel_cfg.n_mels = 128;
                mel_cfg.win_length = 400;  // 0.025s * 16000
            }
        }

        tracing::info!("ONNX STT engine loaded successfully (n_mels={}, win_length={})", mel_cfg.n_mels, mel_cfg.win_length);
        Ok(())
    }

    fn unload_model(&mut self) -> Result<()> {
        *self.encoder_session.lock().unwrap() = None;
        *self.decoder_session.lock().unwrap() = None;
        *self.vocabulary.lock().unwrap() = None;
        Ok(())
    }

    fn is_loaded(&self) -> bool {
        self.encoder_session.lock().unwrap().is_some()
    }

    fn capability(&self) -> ModelCapability {
        ModelCapability::SpeechToText
    }
}

impl SttEngine for OnnxSttEngine {
    fn transcribe(&self, audio: &AudioBuffer, language: Option<&str>) -> Result<TranscriptionResult> {
        let variant = self.variant.lock().unwrap().clone();
        let mel_cfg = self.mel_config.lock().unwrap().clone();

        let vocab_guard = self.vocabulary.lock().unwrap();
        let vocab = vocab_guard.as_ref().context("Vocabulary not loaded")?;

        let start = std::time::Instant::now();

        let n_frames = mel_num_frames(audio.samples.len(), &mel_cfg);
        let mel_flat = mel_spectrogram(&audio.samples, &mel_cfg);

        tracing::info!("Mel spectrogram: {} mels x {} frames ({} values)", mel_cfg.n_mels, n_frames, mel_flat.len());

        let text = match variant {
            ParakeetVariant::Ctc => {
                let mut session_guard = self.encoder_session.lock().unwrap();
                let session = session_guard.as_mut().context("CTC model not loaded")?;

                let mel_tensor = Tensor::from_array((
                    vec![1i64, mel_cfg.n_mels as i64, n_frames as i64],
                    mel_flat,
                )).context("Failed to create mel tensor")?;

                let length_tensor = Tensor::from_array((
                    vec![1i64],
                    vec![n_frames as i64],
                )).context("Failed to create length tensor")?;

                let input_names: Vec<String> = session.inputs().iter().map(|i| i.name().to_string()).collect();
                let output_names: Vec<String> = session.outputs().iter().map(|o| o.name().to_string()).collect();

                let outputs = if input_names.len() > 1 {
                    session.run(ort::inputs![
                        input_names[0].as_str() => mel_tensor,
                        input_names[1].as_str() => length_tensor,
                    ]).context("CTC inference failed")?
                } else {
                    session.run(ort::inputs![
                        input_names[0].as_str() => mel_tensor,
                    ]).context("CTC inference failed (single input)")?
                };

                let first_output_name = &output_names[0];
                let logits_value = outputs.get(first_output_name.as_str())
                    .context("No CTC output tensor found")?;

                let (shape, logits_data) = logits_value.try_extract_tensor::<f32>()
                    .context("Failed to extract CTC logits")?;

                let dims: Vec<usize> = shape.iter().map(|&d| d as usize).collect();
                tracing::info!("CTC output shape: {:?}", dims);

                if dims.len() == 3 {
                    let time_steps = dims[1];
                    let vsize = dims[2];
                    Self::ctc_decode(&logits_data[..time_steps * vsize], time_steps, vsize, vocab)
                } else if dims.len() == 2 {
                    Self::ctc_decode(logits_data, dims[0], dims[1], vocab)
                } else {
                    anyhow::bail!("Unexpected CTC output shape: {:?}", dims);
                }
            }

            ParakeetVariant::Tdt => {
                let encoder_out;
                let encoded_length: usize;
                let encoder_dim: usize;

                {
                    let mut enc_guard = self.encoder_session.lock().unwrap();
                    let encoder = enc_guard.as_mut().context("TDT encoder not loaded")?;

                    let mel_tensor = Tensor::from_array((
                        vec![1i64, mel_cfg.n_mels as i64, n_frames as i64],
                        mel_flat,
                    )).context("Failed to create mel tensor")?;

                    let length_tensor = Tensor::from_array((
                        vec![1i64],
                        vec![n_frames as i64],
                    )).context("Failed to create length tensor")?;

                    let input_names: Vec<String> = encoder.inputs().iter().map(|i| i.name().to_string()).collect();

                    let enc_outputs = if input_names.len() > 1 {
                        encoder.run(ort::inputs![
                            input_names[0].as_str() => mel_tensor,
                            input_names[1].as_str() => length_tensor,
                        ]).context("TDT encoder inference failed")?
                    } else {
                        encoder.run(ort::inputs![
                            input_names[0].as_str() => mel_tensor,
                        ]).context("TDT encoder inference failed (single input)")?
                    };

                    let enc_value = enc_outputs.get("outputs")
                        .context("No 'outputs' tensor from encoder")?;
                    let enc_len_value = enc_outputs.get("encoded_lengths")
                        .context("No 'encoded_lengths' tensor from encoder")?;

                    let (enc_shape, enc_data) = enc_value.try_extract_tensor::<f32>()
                        .context("Failed to extract encoder outputs")?;
                    let (_len_shape, len_data) = enc_len_value.try_extract_tensor::<i64>()
                        .context("Failed to extract encoded lengths")?;

                    let enc_dims: Vec<usize> = enc_shape.iter().map(|&d| d as usize).collect();
                    tracing::info!("TDT encoder output shape: {:?}", enc_dims);

                    // Transpose [1, D, T'] → [T', D] for frame-by-frame decoder access
                    if enc_dims.len() == 3 {
                        let _batch = enc_dims[0];
                        let d = enc_dims[1];
                        let t_enc = enc_dims[2];

                        let mut transposed = vec![0.0f32; t_enc * d];
                        for i in 0..d {
                            for j in 0..t_enc {
                                transposed[j * d + i] = enc_data[i * t_enc + j];
                            }
                        }

                        encoder_dim = d;
                        encoded_length = len_data[0] as usize;
                        encoder_out = transposed;
                    } else {
                        anyhow::bail!("Unexpected encoder output shape: {:?}", enc_dims);
                    }

                    tracing::info!("Encoder: {} frames x {} dim, encoded_length={}", encoder_out.len() / encoder_dim, encoder_dim, encoded_length);
                }

                let mut dec_guard = self.decoder_session.lock().unwrap();
                let decoder = dec_guard.as_mut().context("TDT decoder_joint not loaded")?;

                Self::tdt_decode(decoder, &encoder_out, encoded_length, encoder_dim, vocab)?
            }
        };

        let duration_ms = start.elapsed().as_millis() as u64;
        tracing::info!("Transcription ({}ms): \"{}\"", duration_ms, text);

        Ok(TranscriptionResult {
            text,
            language: language.map(|l| l.to_string()),
            duration_ms,
            segments: None,
        })
    }
}
