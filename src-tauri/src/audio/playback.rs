use anyhow::Result;
use crate::engine::AudioBuffer;

/// Audio playback for TTS output (Phase 6)
pub struct AudioPlayback;

impl AudioPlayback {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    pub fn play(&self, _audio: &AudioBuffer) -> Result<()> {
        todo!("Audio playback - Phase 6: play AudioBuffer via cpal output device")
    }

    pub fn stop(&self) -> Result<()> {
        todo!("Audio playback stop - Phase 6")
    }

    pub fn is_playing(&self) -> bool {
        false
    }
}
