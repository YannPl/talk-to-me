# 💬 Talk to Me

A macOS menu bar utility for **local speech-to-text**, powered by Whisper and NVIDIA Parakeet. No cloud, no API key — everything runs on your machine.

Built with [Tauri v2](https://v2.tauri.app/) (Rust + vanilla HTML/CSS/JS).

## Features

- **🎙 Dictation anywhere** — Press `Alt+Space` to record, press again to transcribe and inject text into any app
- **🤖 Multiple engines** — Whisper (via whisper.cpp) and NVIDIA Parakeet (via ONNX Runtime), including multilingual TDT models
- **📦 Model management** — Browse, download, and switch models from HuggingFace directly in the app
- **🔒 100% local** — No data leaves your machine, no account required
- **⚡ Apple Silicon optimized** — CoreML/Metal acceleration for fast inference

## Quick start

```bash
# Prerequisites: Rust toolchain, Node.js
cargo tauri dev
```

On first launch, open the settings window to download a model. Recommended starting points:

| Model | Engine | Size | Languages |
|-------|--------|------|-----------|
| Whisper Small | whisper.cpp | ~244 MB | Multilingual |
| Whisper Large v3 Turbo | whisper.cpp | ~1.5 GB | Multilingual |
| Parakeet CTC 0.6B | ONNX | ~700 MB | English |
| Parakeet TDT 0.6B v3 | ONNX | ~2.5 GB | 25 languages (EN, FR, DE, ES…) |

Models are stored in `~/Library/Application Support/TalkToMe/models/`.

## How it works

```
Alt+Space → start recording (mic capture via cpal)
Alt+Space → stop recording
   → resample to 16kHz
   → compute mel spectrogram
   → run inference (Whisper or Parakeet)
   → inject text into active app (CGEvent or clipboard)
```

The overlay window shows recording state and transcription progress.

## Architecture

```
src-tauri/src/          Rust backend
├── engine/             SttEngine trait → whisper_stt.rs, onnx_stt.rs
├── audio/              Mic capture, resampling, mel spectrogram (pure Rust)
├── commands/           Tauri IPC: STT, models, settings
├── hub/                HuggingFace API, downloads, model registry
├── hotkey/             Global shortcut dispatch
└── platform/           OS abstraction (TextInjector, TextSelector traits)

src/                    Vanilla JS frontend
├── index.html          Settings window (model management, preferences)
└── overlay.html        Floating recording/transcription overlay
```

Designed for future TTS support (Phase 6) and cross-platform portability (Windows/Linux via `platform/` trait abstraction).

## Requirements

- **macOS 13+** (Ventura)
- Rust toolchain
- Node.js
- Microphone access permission
- Accessibility permission (for keystroke injection, optional — falls back to clipboard)

## Build

```bash
cargo tauri build       # Production .dmg
```

> ⚠️ Without an Apple Developer certificate, users will need to right-click → Open on first launch.

## License

MIT
