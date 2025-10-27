// Audio mixer for combining system audio and microphone streams
//
// This module provides time-aligned mixing of two audio streams:
// - System audio (applications, browser, etc.)
// - Microphone input (user's voice)
//
// The mixer buffers frames from each stream, aligns them by timestamp,
// and mixes the samples together using simple addition with clipping.

use anyhow::Result;
use std::collections::{HashMap, HashSet, VecDeque};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use super::backend::{AudioFrame, AudioStreamSource};

/// Configuration for audio mixer
#[derive(Debug, Clone)]
pub struct MixerConfig {
    /// Target sample rate for output
    pub sample_rate: u32,
    /// Number of channels in output
    pub channels: u16,
    /// Maximum buffering delay in milliseconds (default: 200ms)
    /// Frames older than this are dropped to prevent unbounded buffering
    pub max_buffer_delay_ms: u64,
    /// Sources to include in the mix (empty = include all sources)
    /// Example: [AudioStreamSource::System, AudioStreamSource::Microphone]
    pub enabled_sources: HashSet<AudioStreamSource>,
}

impl Default for MixerConfig {
    fn default() -> Self {
        // By default, include all sources
        let mut enabled_sources = HashSet::new();
        enabled_sources.insert(AudioStreamSource::System);
        enabled_sources.insert(AudioStreamSource::Microphone);

        Self {
            sample_rate: 16000,
            channels: 1,
            max_buffer_delay_ms: 200,
            enabled_sources,
        }
    }
}

/// Audio mixer that combines multiple audio streams
pub struct AudioMixer {
    config: MixerConfig,
    /// Buffers for each audio source type
    buffers: HashMap<AudioStreamSource, VecDeque<AudioFrame>>,
    current_position_ms: u64,
}

impl AudioMixer {
    pub fn new(config: MixerConfig) -> Self {
        info!(
            "Audio mixer initialized: {}Hz, {} channels, {} enabled sources",
            config.sample_rate,
            config.channels,
            config.enabled_sources.len()
        );

        // Initialize buffers for enabled sources
        let mut buffers = HashMap::new();
        for source in &config.enabled_sources {
            buffers.insert(*source, VecDeque::new());
        }

        Self {
            config,
            buffers,
            current_position_ms: 0,
        }
    }

    /// Mix audio frames from two sources into a single output stream
    ///
    /// Receives frames from both system and microphone, time-aligns them,
    /// and produces mixed output frames
    pub async fn mix(
        &mut self,
        mut audio_rx: mpsc::Receiver<AudioFrame>,
    ) -> Result<Vec<AudioFrame>> {
        let mut mixed_frames = Vec::new();

        info!("Starting audio mixing");

        while let Some(frame) = audio_rx.recv().await {
            // Route frame to appropriate buffer
            self.buffer_frame(frame);

            // Try to mix available frames
            if let Some(mixed) = self.mix_next_chunk()? {
                mixed_frames.push(mixed);
            }
        }

        // Flush remaining buffered frames
        while let Some(mixed) = self.mix_next_chunk()? {
            mixed_frames.push(mixed);
        }

        info!(
            "Audio mixing complete: {} mixed frames produced",
            mixed_frames.len()
        );

        Ok(mixed_frames)
    }

    /// Buffer a frame based on its source type
    fn buffer_frame(&mut self, frame: AudioFrame) {
        // Check if this source is enabled
        if !self.config.enabled_sources.contains(&frame.source) {
            debug!(
                "Skipping frame from disabled source: {:?} at {}ms",
                frame.source, frame.timestamp_ms
            );
            return;
        }

        // Validate frame format
        if frame.sample_rate != self.config.sample_rate {
            warn!(
                "Frame sample rate mismatch: expected {}, got {}. Dropping frame.",
                self.config.sample_rate, frame.sample_rate
            );
            return;
        }

        if frame.channels != self.config.channels {
            warn!(
                "Frame channel count mismatch: expected {}, got {}. Dropping frame.",
                self.config.channels, frame.channels
            );
            return;
        }

        // Add to source-specific buffer
        if let Some(buffer) = self.buffers.get_mut(&frame.source) {
            debug!(
                "Buffered {:?} frame: {}ms ({} samples)",
                frame.source,
                frame.timestamp_ms,
                frame.samples.len()
            );
            buffer.push_back(frame);
        }

        // Clean up old frames to prevent unbounded buffering
        self.cleanup_old_frames();
    }

    /// Remove frames that are too old (beyond max buffer delay)
    fn cleanup_old_frames(&mut self) {
        let cutoff_time = self
            .current_position_ms
            .saturating_sub(self.config.max_buffer_delay_ms);

        // Clean all buffers
        for (source, buffer) in &mut self.buffers {
            while let Some(frame) = buffer.front() {
                if frame.timestamp_ms < cutoff_time {
                    warn!(
                        "Dropping old {:?} frame at {}ms (current position: {}ms)",
                        source, frame.timestamp_ms, self.current_position_ms
                    );
                    buffer.pop_front();
                } else {
                    break;
                }
            }
        }
    }

    /// Try to mix the next chunk of audio from all enabled source buffers
    ///
    /// Returns None if there's no data available in any buffer
    fn mix_next_chunk(&mut self) -> Result<Option<AudioFrame>> {
        // Collect one frame from each source buffer
        let mut frames_to_mix: Vec<AudioFrame> = Vec::new();

        for (_source, buffer) in &mut self.buffers {
            if let Some(frame) = buffer.pop_front() {
                frames_to_mix.push(frame);
            }
        }

        // If no frames available, return None
        if frames_to_mix.is_empty() {
            return Ok(None);
        }

        // If only one frame, return it directly (no mixing needed)
        if frames_to_mix.len() == 1 {
            let frame = frames_to_mix.into_iter().next().unwrap();
            debug!(
                "Single source {:?} at {}ms",
                frame.source, frame.timestamp_ms
            );
            self.current_position_ms = frame.timestamp_ms;
            return Ok(Some(frame));
        }

        // Mix multiple frames together
        let mixed = self.mix_multiple_frames(&frames_to_mix)?;
        self.current_position_ms = mixed.timestamp_ms;
        Ok(Some(mixed))
    }

    /// Mix multiple audio frames together by adding their samples
    ///
    /// Handles timestamp alignment and sample mixing with clipping
    fn mix_multiple_frames(&self, frames: &[AudioFrame]) -> Result<AudioFrame> {
        if frames.is_empty() {
            anyhow::bail!("Cannot mix zero frames");
        }

        // Use the earliest timestamp
        let timestamp_ms = frames
            .iter()
            .map(|f| f.timestamp_ms)
            .min()
            .unwrap_or(0);

        // Determine output length (use the longest frame)
        let max_len = frames.iter().map(|f| f.samples.len()).max().unwrap_or(0);
        let mut mixed_samples = Vec::with_capacity(max_len);

        // Mix samples by adding them together with clipping
        for i in 0..max_len {
            let mut sum: i32 = 0;

            // Add sample from each frame
            for frame in frames {
                let sample = frame.samples.get(i).copied().unwrap_or(0);
                sum += sample as i32;
            }

            // Clip to prevent overflow
            let mixed = sum.clamp(i16::MIN as i32, i16::MAX as i32);
            mixed_samples.push(mixed as i16);
        }

        debug!(
            "Mixed {} frames at {}ms: {} samples total",
            frames.len(),
            timestamp_ms,
            mixed_samples.len()
        );

        Ok(AudioFrame {
            samples: mixed_samples,
            sample_rate: self.config.sample_rate,
            channels: self.config.channels,
            timestamp_ms,
            source: AudioStreamSource::System, // Mixed frames are marked as System
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mixer_creation() {
        let config = MixerConfig::default();
        let mixer = AudioMixer::new(config);

        assert_eq!(mixer.buffers.len(), 2); // System and Microphone by default
        assert_eq!(mixer.current_position_ms, 0);
    }

    #[test]
    fn test_mixer_config_system_only() {
        let mut enabled_sources = HashSet::new();
        enabled_sources.insert(AudioStreamSource::System);

        let config = MixerConfig {
            sample_rate: 16000,
            channels: 1,
            max_buffer_delay_ms: 200,
            enabled_sources,
        };

        assert_eq!(config.enabled_sources.len(), 1);
        assert!(config.enabled_sources.contains(&AudioStreamSource::System));
        assert!(!config.enabled_sources.contains(&AudioStreamSource::Microphone));
    }

    #[test]
    fn test_mixer_config_microphone_only() {
        let mut enabled_sources = HashSet::new();
        enabled_sources.insert(AudioStreamSource::Microphone);

        let config = MixerConfig {
            sample_rate: 16000,
            channels: 1,
            max_buffer_delay_ms: 200,
            enabled_sources,
        };

        assert_eq!(config.enabled_sources.len(), 1);
        assert!(!config.enabled_sources.contains(&AudioStreamSource::System));
        assert!(config.enabled_sources.contains(&AudioStreamSource::Microphone));
    }

    #[test]
    fn test_mixer_config_both() {
        let config = MixerConfig::default();
        assert_eq!(config.enabled_sources.len(), 2);
        assert!(config.enabled_sources.contains(&AudioStreamSource::System));
        assert!(config.enabled_sources.contains(&AudioStreamSource::Microphone));
    }

    #[test]
    fn test_mix_frames_equal_length() {
        let mixer = AudioMixer::new(MixerConfig::default());

        let frame1 = AudioFrame {
            samples: vec![100, 200, 300],
            sample_rate: 16000,
            channels: 1,
            timestamp_ms: 0,
            source: AudioStreamSource::System,
        };

        let frame2 = AudioFrame {
            samples: vec![50, 100, 150],
            sample_rate: 16000,
            channels: 1,
            timestamp_ms: 0,
            source: AudioStreamSource::Microphone,
        };

        let frames = vec![frame1, frame2];
        let mixed = mixer.mix_multiple_frames(&frames).unwrap();

        assert_eq!(mixed.samples.len(), 3);
        assert_eq!(mixed.samples[0], 150); // 100 + 50
        assert_eq!(mixed.samples[1], 300); // 200 + 100
        assert_eq!(mixed.samples[2], 450); // 300 + 150
    }

    #[test]
    fn test_mix_frames_with_clipping() {
        let mixer = AudioMixer::new(MixerConfig::default());

        let frame1 = AudioFrame {
            samples: vec![i16::MAX - 100],
            sample_rate: 16000,
            channels: 1,
            timestamp_ms: 0,
            source: AudioStreamSource::System,
        };

        let frame2 = AudioFrame {
            samples: vec![200],
            sample_rate: 16000,
            channels: 1,
            timestamp_ms: 0,
            source: AudioStreamSource::Microphone,
        };

        let frames = vec![frame1, frame2];
        let mixed = mixer.mix_multiple_frames(&frames).unwrap();

        assert_eq!(mixed.samples[0], i16::MAX); // Clipped to max
    }

    #[test]
    fn test_mix_frames_different_lengths() {
        let mixer = AudioMixer::new(MixerConfig::default());

        let frame1 = AudioFrame {
            samples: vec![100, 200],
            sample_rate: 16000,
            channels: 1,
            timestamp_ms: 0,
            source: AudioStreamSource::System,
        };

        let frame2 = AudioFrame {
            samples: vec![50, 100, 150, 200],
            sample_rate: 16000,
            channels: 1,
            timestamp_ms: 0,
            source: AudioStreamSource::Microphone,
        };

        let frames = vec![frame1, frame2];
        let mixed = mixer.mix_multiple_frames(&frames).unwrap();

        assert_eq!(mixed.samples.len(), 4); // Length of longer frame
        assert_eq!(mixed.samples[0], 150); // 100 + 50
        assert_eq!(mixed.samples[1], 300); // 200 + 100
        assert_eq!(mixed.samples[2], 150); // 0 + 150 (frame1 ended)
        assert_eq!(mixed.samples[3], 200); // 0 + 200 (frame1 ended)
    }
}
