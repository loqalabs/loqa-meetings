// Example: Record system audio in 5-minute chunks
//
// This example demonstrates the complete audio capture pipeline:
// 1. Create macOS ScreenCaptureKit backend
// 2. Start capturing system audio
// 3. Feed audio frames to ChunkedRecorder
// 4. Save chunks to disk as WAV files
//
// Requirements: macOS 13.0+ (Ventura)
//
// Usage: cargo run --example record_chunks -- --duration 30
//
// This will record for 30 seconds and save chunks to ~/.loqa/recordings/test-meeting/

use anyhow::Result;
use clap::Parser;
use loqa_meetings::audio::{
    AudioBackendConfig, AudioBackendFactory, AudioSource,
    ChunkConfig, ChunkedRecorder,
};
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, Level};

#[derive(Parser)]
#[command(name = "record_chunks")]
#[command(about = "Record system audio in chunks")]
struct Args {
    /// Duration to record in seconds
    #[arg(short, long, default_value = "30")]
    duration: u64,

    /// Meeting ID (used for chunk filenames)
    #[arg(short, long, default_value = "test-meeting")]
    meeting_id: String,

    /// Output directory
    #[arg(short, long, default_value = "~/.loqa/recordings")]
    output_dir: String,

    /// Chunk duration in seconds
    #[arg(short, long, default_value = "300")]
    chunk_duration: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    let args = Args::parse();

    info!("Loqa Meetings - Chunked Recording Example");
    info!("Recording for {} seconds", args.duration);
    info!("Meeting ID: {}", args.meeting_id);
    info!("Chunk duration: {} seconds", args.chunk_duration);

    // Expand home directory
    let output_dir = shellexpand::tilde(&args.output_dir);
    let output_dir = PathBuf::from(output_dir.as_ref()).join(&args.meeting_id);

    info!("Output directory: {}", output_dir.display());

    // Create audio backend configuration
    let backend_config = AudioBackendConfig {
        target_sample_rate: 16000,  // 16kHz for Whisper
        target_channels: 1,          // Mono
        buffer_duration_ms: 100,     // 100ms buffers
    };

    // Create backend (macOS ScreenCaptureKit for system audio)
    info!("Creating macOS ScreenCaptureKit backend...");
    let mut backend = AudioBackendFactory::create(
        AudioSource::System,
        backend_config,
    )?;

    info!("Backend created: {}", backend.name());

    // Create chunked recorder
    let chunk_config = ChunkConfig {
        chunk_duration_secs: args.chunk_duration,
        output_dir: output_dir.clone(),
        meeting_id: args.meeting_id.clone(),
    };

    let mut recorder = ChunkedRecorder::new(chunk_config)?;

    // Start capturing
    info!("Starting audio capture...");
    let audio_rx = backend.start().await?;

    info!("Recording started! Press Ctrl+C to stop early, or wait {} seconds", args.duration);

    // Spawn recording task
    let recording_handle = tokio::spawn(async move {
        recorder.record(audio_rx).await
    });

    // Wait for duration
    sleep(Duration::from_secs(args.duration)).await;

    // Stop capture
    info!("Stopping audio capture...");
    backend.stop().await?;

    // Wait for recording to finish
    info!("Finalizing chunks...");
    let metadata = recording_handle.await??;

    // Print summary
    info!("Recording complete!");
    info!("Saved {} chunks:", metadata.len());
    for chunk in &metadata {
        info!(
            "  - Chunk {}: {} ({:.1}s - {:.1}s, {} samples)",
            chunk.chunk_index,
            chunk.file_path.display(),
            chunk.start_ms as f64 / 1000.0,
            chunk.end_ms as f64 / 1000.0,
            chunk.sample_count
        );
    }

    info!("Total recording duration: {:.1}s",
        metadata.last().map(|c| c.end_ms as f64 / 1000.0).unwrap_or(0.0));

    Ok(())
}
