use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::fs::{self, File};
use std::io::BufWriter;
use tokio::sync::mpsc;
use tracing::{info, warn};

use super::backend::AudioFrame;

/// Chunk configuration
#[derive(Debug, Clone)]
pub struct ChunkConfig {
    /// Duration of each chunk in seconds (default: 300 = 5 minutes)
    pub chunk_duration_secs: u64,
    /// Output directory for chunks
    pub output_dir: PathBuf,
    /// Meeting ID (used for chunk filenames)
    pub meeting_id: String,
}

impl ChunkConfig {
    pub fn new(meeting_id: String, output_dir: PathBuf) -> Self {
        Self {
            chunk_duration_secs: 300,  // 5 minutes default
            output_dir,
            meeting_id,
        }
    }
}

/// Metadata for a single chunk
#[derive(Debug, Clone)]
pub struct ChunkMetadata {
    /// Chunk number (0-indexed)
    pub chunk_index: usize,
    /// File path to the chunk
    pub file_path: PathBuf,
    /// Start time in milliseconds since meeting started
    pub start_ms: u64,
    /// End time in milliseconds since meeting started
    pub end_ms: u64,
    /// Sample rate
    pub sample_rate: u32,
    /// Number of channels
    pub channels: u16,
    /// Number of samples in this chunk
    pub sample_count: usize,
}

/// Chunked audio recorder
///
/// Receives audio frames from a backend and saves them to disk in fixed-duration chunks
pub struct ChunkedRecorder {
    config: ChunkConfig,
    current_chunk: Option<ChunkWriter>,
    chunk_index: usize,
    meeting_start_ms: u64,
}

impl ChunkedRecorder {
    pub fn new(config: ChunkConfig) -> Result<Self> {
        // Create output directory if it doesn't exist
        fs::create_dir_all(&config.output_dir)
            .context("Failed to create output directory")?;

        info!(
            "Chunked recorder initialized: {} (chunks: {}s each)",
            config.meeting_id, config.chunk_duration_secs
        );

        Ok(Self {
            config,
            current_chunk: None,
            chunk_index: 0,
            meeting_start_ms: 0,
        })
    }

    /// Process incoming audio frames and save to chunks
    pub async fn record(
        &mut self,
        mut audio_rx: mpsc::Receiver<AudioFrame>,
    ) -> Result<Vec<ChunkMetadata>> {
        let mut metadata = Vec::new();

        info!("Starting chunked recording");

        while let Some(frame) = audio_rx.recv().await {
            // Initialize meeting start time from first frame
            if self.meeting_start_ms == 0 {
                self.meeting_start_ms = frame.timestamp_ms;
            }

            // Check if we need to start a new chunk
            if self.should_start_new_chunk(&frame) {
                // Finish current chunk
                if let Some(chunk) = self.current_chunk.take() {
                    let chunk_meta = chunk.finish()?;
                    info!(
                        "Chunk {} complete: {:.1}s - {:.1}s ({} samples)",
                        chunk_meta.chunk_index,
                        chunk_meta.start_ms as f64 / 1000.0,
                        chunk_meta.end_ms as f64 / 1000.0,
                        chunk_meta.sample_count
                    );
                    metadata.push(chunk_meta);
                }

                // Start new chunk
                self.current_chunk = Some(self.start_new_chunk(&frame)?);
            }

            // Write frame to current chunk
            if let Some(chunk) = &mut self.current_chunk {
                chunk.write_frame(&frame)?;
            }
        }

        // Finish final chunk
        if let Some(chunk) = self.current_chunk.take() {
            let chunk_meta = chunk.finish()?;
            info!(
                "Final chunk {} complete: {:.1}s - {:.1}s ({} samples)",
                chunk_meta.chunk_index,
                chunk_meta.start_ms as f64 / 1000.0,
                chunk_meta.end_ms as f64 / 1000.0,
                chunk_meta.sample_count
            );
            metadata.push(chunk_meta);
        }

        info!(
            "Chunked recording complete: {} chunks saved",
            metadata.len()
        );

        Ok(metadata)
    }

    fn should_start_new_chunk(&self, frame: &AudioFrame) -> bool {
        match &self.current_chunk {
            None => true, // No current chunk, start one
            Some(chunk) => {
                // Check if chunk duration exceeded
                let chunk_duration_ms = self.config.chunk_duration_secs * 1000;
                let elapsed_ms = frame.timestamp_ms - chunk.metadata.start_ms;
                elapsed_ms >= chunk_duration_ms
            }
        }
    }

    fn start_new_chunk(&mut self, frame: &AudioFrame) -> Result<ChunkWriter> {
        let chunk_path = self.config.output_dir.join(format!(
            "{}-chunk-{:03}.wav",
            self.config.meeting_id, self.chunk_index
        ));

        let chunk = ChunkWriter::new(
            chunk_path,
            self.chunk_index,
            frame.timestamp_ms,
            frame.sample_rate,
            frame.channels,
        )?;

        self.chunk_index += 1;

        Ok(chunk)
    }
}

/// Writes a single chunk to disk as WAV file
struct ChunkWriter {
    writer: Option<hound::WavWriter<BufWriter<File>>>,
    metadata: ChunkMetadata,
}

impl ChunkWriter {
    fn new(
        file_path: PathBuf,
        chunk_index: usize,
        start_ms: u64,
        sample_rate: u32,
        channels: u16,
    ) -> Result<Self> {
        let spec = hound::WavSpec {
            channels,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let writer = hound::WavWriter::create(&file_path, spec)
            .with_context(|| format!("Failed to create WAV file: {:?}", file_path))?;

        Ok(Self {
            writer: Some(writer),
            metadata: ChunkMetadata {
                chunk_index,
                file_path,
                start_ms,
                end_ms: start_ms,
                sample_rate,
                channels,
                sample_count: 0,
            },
        })
    }

    fn write_frame(&mut self, frame: &AudioFrame) -> Result<()> {
        if let Some(writer) = &mut self.writer {
            for &sample in &frame.samples {
                writer.write_sample(sample)
                    .context("Failed to write sample to WAV")?;
            }

            self.metadata.end_ms = frame.timestamp_ms;
            self.metadata.sample_count += frame.samples.len();
        }

        Ok(())
    }

    fn finish(mut self) -> Result<ChunkMetadata> {
        if let Some(writer) = self.writer.take() {
            writer.finalize()
                .context("Failed to finalize WAV file")?;
        }

        Ok(self.metadata.clone())
    }
}

impl Drop for ChunkWriter {
    fn drop(&mut self) {
        if let Some(writer) = self.writer.take() {
            if let Err(e) = writer.finalize() {
                warn!("Failed to finalize WAV writer on drop: {}", e);
            }
        }
    }
}
