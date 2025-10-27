// Integration tests for chunked audio recording
//
// These tests verify that audio frames are correctly split into
// time-based chunks and saved to disk as WAV files.

use anyhow::Result;
use loqa_meetings::audio::{AudioFrame, ChunkConfig, ChunkedRecorder};
use std::path::PathBuf;
use std::fs;
use tempfile::TempDir;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_chunked_recording_creates_single_chunk() -> Result<()> {
    // Setup: Create temporary directory for test outputs
    let temp_dir = TempDir::new()?;
    let output_dir = temp_dir.path().to_path_buf();

    let config = ChunkConfig {
        chunk_duration_secs: 10, // 10 second chunks
        output_dir: output_dir.clone(),
        meeting_id: "test-meeting".to_string(),
    };

    let mut recorder = ChunkedRecorder::new(config)?;

    // Create channel for audio frames
    let (tx, rx) = mpsc::channel(100);

    // Spawn recording task
    let recording_handle = tokio::spawn(async move {
        recorder.record(rx).await
    });

    // Send 5 seconds worth of audio frames (16kHz mono)
    // Each frame = 100ms = 1600 samples
    let samples_per_frame = 1600;
    let num_frames = 50; // 50 frames * 100ms = 5 seconds

    for i in 0..num_frames {
        let frame = AudioFrame {
            samples: vec![0i16; samples_per_frame],
            sample_rate: 16000,
            channels: 1,
            timestamp_ms: i * 100, // 100ms intervals
        };
        tx.send(frame).await?;
    }

    // Close the channel to signal end of recording
    drop(tx);

    // Wait for recording to complete
    let metadata = recording_handle.await??;

    // Verify: Should have created exactly 1 chunk (5s < 10s chunk duration)
    assert_eq!(metadata.len(), 1, "Should create exactly 1 chunk");

    // Verify chunk metadata
    let chunk = &metadata[0];
    assert_eq!(chunk.chunk_index, 0);
    assert_eq!(chunk.sample_rate, 16000);
    assert_eq!(chunk.channels, 1);
    assert_eq!(chunk.start_ms, 0);
    assert_eq!(chunk.end_ms, 4900); // Last frame timestamp
    assert_eq!(chunk.sample_count, samples_per_frame * num_frames as usize);

    // Verify file exists
    assert!(chunk.file_path.exists(), "Chunk file should exist");
    assert!(chunk.file_path.to_string_lossy().contains("test-meeting-chunk-000.wav"));

    // Verify file size is reasonable (not empty)
    let file_size = fs::metadata(&chunk.file_path)?.len();
    assert!(file_size > 0, "Chunk file should not be empty");

    Ok(())
}

#[tokio::test]
async fn test_chunked_recording_splits_into_multiple_chunks() -> Result<()> {
    // Setup: Create temporary directory for test outputs
    let temp_dir = TempDir::new()?;
    let output_dir = temp_dir.path().to_path_buf();

    let config = ChunkConfig {
        chunk_duration_secs: 2, // 2 second chunks
        output_dir: output_dir.clone(),
        meeting_id: "multi-chunk-test".to_string(),
    };

    let mut recorder = ChunkedRecorder::new(config)?;

    // Create channel for audio frames
    let (tx, rx) = mpsc::channel(100);

    // Spawn recording task
    let recording_handle = tokio::spawn(async move {
        recorder.record(rx).await
    });

    // Send 5 seconds worth of audio frames
    // This should create 3 chunks: [0-2s], [2-4s], [4-5s]
    let samples_per_frame = 1600; // 100ms at 16kHz
    let num_frames = 50; // 50 frames * 100ms = 5 seconds

    for i in 0..num_frames {
        let frame = AudioFrame {
            samples: vec![(i % 100) as i16; samples_per_frame],
            sample_rate: 16000,
            channels: 1,
            timestamp_ms: i * 100,
        };
        tx.send(frame).await?;
    }

    // Close the channel
    drop(tx);

    // Wait for recording to complete
    let metadata = recording_handle.await??;

    // Verify: Should have created 3 chunks
    assert_eq!(metadata.len(), 3, "Should create 3 chunks for 5s recording with 2s chunks");

    // Verify chunk 0 (0-2s)
    assert_eq!(metadata[0].chunk_index, 0);
    assert_eq!(metadata[0].start_ms, 0);
    assert!(metadata[0].end_ms >= 1900 && metadata[0].end_ms < 2100,
            "Chunk 0 should end around 2s, got {}ms", metadata[0].end_ms);

    // Verify chunk 1 (2-4s)
    assert_eq!(metadata[1].chunk_index, 1);
    assert!(metadata[1].start_ms >= 1900 && metadata[1].start_ms < 2100,
            "Chunk 1 should start around 2s, got {}ms", metadata[1].start_ms);
    assert!(metadata[1].end_ms >= 3900 && metadata[1].end_ms < 4100,
            "Chunk 1 should end around 4s, got {}ms", metadata[1].end_ms);

    // Verify chunk 2 (4-5s)
    assert_eq!(metadata[2].chunk_index, 2);
    assert!(metadata[2].start_ms >= 3900 && metadata[2].start_ms < 4100,
            "Chunk 2 should start around 4s, got {}ms", metadata[2].start_ms);
    assert_eq!(metadata[2].end_ms, 4900); // Last frame timestamp

    // Verify all files exist
    for chunk in &metadata {
        assert!(chunk.file_path.exists(), "Chunk {} file should exist", chunk.chunk_index);
    }

    Ok(())
}

#[tokio::test]
async fn test_chunked_recording_handles_empty_input() -> Result<()> {
    // Setup
    let temp_dir = TempDir::new()?;
    let output_dir = temp_dir.path().to_path_buf();

    let config = ChunkConfig {
        chunk_duration_secs: 5,
        output_dir: output_dir.clone(),
        meeting_id: "empty-test".to_string(),
    };

    let mut recorder = ChunkedRecorder::new(config)?;

    // Create channel but don't send any frames
    let (tx, rx) = mpsc::channel(100);

    // Drop sender immediately to close the channel
    drop(tx);

    // Recording should complete immediately with no chunks
    let metadata = recorder.record(rx).await?;

    // Verify: No chunks created
    assert_eq!(metadata.len(), 0, "Should create 0 chunks for empty input");

    Ok(())
}

#[tokio::test]
async fn test_chunked_recording_preserves_audio_format() -> Result<()> {
    // Setup
    let temp_dir = TempDir::new()?;
    let output_dir = temp_dir.path().to_path_buf();

    let config = ChunkConfig {
        chunk_duration_secs: 10,
        output_dir: output_dir.clone(),
        meeting_id: "format-test".to_string(),
    };

    let mut recorder = ChunkedRecorder::new(config)?;

    let (tx, rx) = mpsc::channel(100);

    let recording_handle = tokio::spawn(async move {
        recorder.record(rx).await
    });

    // Send frames with known sample rate and channels
    let sample_rate = 16000;
    let channels = 1;

    for i in 0..10 {
        let frame = AudioFrame {
            samples: vec![100i16; 1600],
            sample_rate,
            channels,
            timestamp_ms: i * 100,
        };
        tx.send(frame).await?;
    }

    drop(tx);
    let metadata = recording_handle.await??;

    // Verify audio format is preserved
    assert_eq!(metadata[0].sample_rate, sample_rate, "Sample rate should be preserved");
    assert_eq!(metadata[0].channels, channels, "Channel count should be preserved");

    Ok(())
}

#[test]
fn test_chunk_config_creation() {
    let config = ChunkConfig::new(
        "test-meeting".to_string(),
        PathBuf::from("/tmp/test"),
    );

    assert_eq!(config.meeting_id, "test-meeting");
    assert_eq!(config.output_dir, PathBuf::from("/tmp/test"));
    assert_eq!(config.chunk_duration_secs, 300, "Default chunk duration should be 5 minutes");
}

#[test]
fn test_chunk_config_custom_duration() {
    let config = ChunkConfig {
        chunk_duration_secs: 60, // 1 minute
        output_dir: PathBuf::from("/tmp/test"),
        meeting_id: "test".to_string(),
    };

    assert_eq!(config.chunk_duration_secs, 60);
}
