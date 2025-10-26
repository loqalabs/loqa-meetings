use anyhow::{Context, Result};
use hound::WavReader;
use std::path::Path;
use tracing::info;

pub struct AudioFile {
    pub path: String,
    pub duration_seconds: f64,
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<i16>,
}

impl AudioFile {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        info!("Opening audio file: {}", path.display());

        let reader = WavReader::open(path)
            .context("Failed to open WAV file")?;

        let spec = reader.spec();
        let samples: Vec<i16> = reader
            .into_samples::<i16>()
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to read audio samples")?;

        let duration_seconds = samples.len() as f64 /
            (spec.sample_rate as f64 * spec.channels as f64);

        info!(
            "Audio file loaded: {:.1}s, {}Hz, {} channels, {} samples",
            duration_seconds,
            spec.sample_rate,
            spec.channels,
            samples.len()
        );

        Ok(Self {
            path: path.display().to_string(),
            duration_seconds,
            sample_rate: spec.sample_rate,
            channels: spec.channels,
            samples,
        })
    }

    pub fn resample_to_mono_16khz(&self) -> Result<Vec<i16>> {
        // TODO: Implement resampling for Whisper (16kHz mono)
        // For Week 1, just return original samples if already 16kHz mono
        if self.sample_rate == 16000 && self.channels == 1 {
            Ok(self.samples.clone())
        } else {
            anyhow::bail!(
                "Resampling not implemented yet. Expected 16kHz mono, got {}Hz {}ch",
                self.sample_rate,
                self.channels
            )
        }
    }
}
