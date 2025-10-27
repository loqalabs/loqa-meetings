// Week 2 Final Test: Real audio capture with chunking and mixing
//
// This example tests the complete Week 2 implementation:
// - ScreenCaptureKit system audio capture
// - Microphone capture (macOS 15.0+)
// - Chunked recording (5-minute chunks)
// - Audio mixing
//
// IMPORTANT: Requires macOS permissions:
// 1. System Settings → Privacy & Security → Screen Recording → Add Terminal/IDE
// 2. System Settings → Privacy & Security → Microphone → Add Terminal/IDE

use anyhow::Result;
use loqa_meetings::{ChunkConfig, ChunkedRecorder};
use std::path::PathBuf;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("=== Week 2 Final Test: Audio Capture & Mixing ===\n");

    // Check if ScreenCaptureKit is available
    #[cfg(target_os = "macos")]
    {
        if !loqa_meetings::screencapture::is_available() {
            eprintln!("❌ ScreenCaptureKit not available (requires macOS 13.0+)");
            return Ok(());
        }
        println!("✅ ScreenCaptureKit available");
    }

    #[cfg(not(target_os = "macos"))]
    {
        eprintln!("❌ This test requires macOS");
        return Ok(());
    }

    // Set up output directory
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let output_dir = PathBuf::from(format!("{}/.loqa/recordings", home));

    println!("📁 Output directory: {}", output_dir.display());
    std::fs::create_dir_all(&output_dir)?;

    // Configure chunked recording (10 seconds for testing, normally 5 minutes)
    let config = ChunkConfig {
        chunk_duration_secs: 10, // 10 second chunks for quick testing
        output_dir: output_dir.clone(),
        meeting_id: "week2-test".to_string(),
    };

    println!("\n🎙️  Starting audio capture...");
    println!("   - Recording for 15 seconds");
    println!("   - Chunk size: {} seconds", config.chunk_duration_secs);
    println!("   - Sources: System audio + Microphone (macOS 15.0+)");
    println!("\n💡 TIP: Play some audio or speak to test the capture!\n");

    #[cfg(target_os = "macos")]
    {
        use loqa_meetings::screencapture::ScreenCaptureSession;
        use loqa_meetings::{AudioMixer, MixerConfig};

        // Start capture session (48kHz native - no resampling for clean audio)
        let mut session = ScreenCaptureSession::new(48000, 1);
        let audio_rx = session.start()?;

        // Create mixer to combine system + microphone streams at 48kHz
        let mixer_config = MixerConfig {
            sample_rate: 48000,  // Native sample rate for clean audio
            channels: 1,
            max_buffer_delay_ms: 200,
            enabled_sources: {
                let mut sources = std::collections::HashSet::new();
                sources.insert(loqa_meetings::AudioStreamSource::System);
                sources.insert(loqa_meetings::AudioStreamSource::Microphone);
                sources
            },
        };
        let mut mixer = AudioMixer::new(mixer_config);

        // Start chunked recorder
        let mut recorder = ChunkedRecorder::new(config.clone())?;

        // Spawn mixing and recording task
        let record_handle = tokio::spawn(async move {
            // Mix the audio streams first
            let mixed_frames = mixer.mix(audio_rx).await?;

            // Convert Vec<AudioFrame> to channel - need to spawn sender to avoid deadlock
            let (tx, rx) = tokio::sync::mpsc::channel(1000);

            // Spawn task to send frames
            tokio::spawn(async move {
                for frame in mixed_frames {
                    if tx.send(frame).await.is_err() {
                        break;
                    }
                }
                // Drop tx to signal end of stream
            });

            // Record the mixed audio
            recorder.record(rx).await
        });

        // Record for 15 seconds
        sleep(Duration::from_secs(15)).await;

        // Stop capture
        println!("\n⏹️  Stopping capture...");
        session.stop()?;

        // Wait for recorder to finish
        let chunks = record_handle.await??;

        // Display results
        println!("\n✅ Recording complete!");
        println!("\n📊 Results:");
        println!("   - Chunks created: {}", chunks.len());

        for (i, chunk) in chunks.iter().enumerate() {
            let duration_secs = (chunk.end_ms - chunk.start_ms) as f64 / 1000.0;
            let file_size = std::fs::metadata(&chunk.file_path)?.len();

            println!("\n   Chunk {}:", i + 1);
            println!("     - File: {}", chunk.file_path.display());
            println!("     - Duration: {:.1}s", duration_secs);
            println!("     - Samples: {}", chunk.sample_count);
            println!("     - Size: {:.1} KB", file_size as f64 / 1024.0);
            println!("     - Format: {}Hz, {} channel(s)", chunk.sample_rate, chunk.channels);
        }

        println!("\n🎉 Week 2 testing complete!");
        println!("\n📁 Audio files saved to: {}", output_dir.display());
    }

    Ok(())
}
