// Multichannel Audio Capture Test
//
// Tests the multichannel approach where:
// - System audio → Left channel
// - Microphone → Right channel
//
// This approach ensures that if only ONE source has audio,
// that source will still be captured (fixing the silent audio bug).
//
// IMPORTANT: Requires macOS permissions:
// 1. System Settings → Privacy & Security → Screen Recording → Add Terminal/IDE
// 2. System Settings → Privacy & Security → Microphone → Add Terminal/IDE

use anyhow::Result;
use std::path::PathBuf;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    println!("=== Multichannel Audio Capture Test ===\n");

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
    let output_file = output_dir.join("multichannel-test.wav");

    println!("📁 Output file: {}", output_file.display());
    std::fs::create_dir_all(&output_dir)?;

    println!("\n🎙️  Starting multichannel audio capture...");
    println!("   - Recording for 15 seconds");
    println!("   - Format: 48kHz stereo");
    println!("   - System audio → LEFT channel");
    println!("   - Microphone → RIGHT channel");
    println!("\n💡 TIP: Play some audio AND speak to test both channels!\n");
    println!("   Or try just ONE source to verify it still captures.\n");

    #[cfg(target_os = "macos")]
    {
        use loqa_meetings::screencapture::ScreenCaptureSession;
        use loqa_meetings::AudioFrame;

        // Start capture session (48kHz, stereo output now)
        let mut session = ScreenCaptureSession::new(48000, 2);
        let mut audio_rx = session.start()?;

        // Collect frames for 15 seconds
        let mut all_frames: Vec<AudioFrame> = Vec::new();

        let collect_handle = tokio::spawn(async move {
            while let Some(frame) = audio_rx.recv().await {
                all_frames.push(frame);
            }
            all_frames
        });

        // Record for 15 seconds
        sleep(Duration::from_secs(15)).await;

        // Stop capture
        println!("\n⏹️  Stopping capture...");
        session.stop()?;

        // Wait for frames to be collected
        let frames = collect_handle.await?;

        println!("   - Collected {} frames", frames.len());

        // Write to WAV file
        println!("\n💾 Writing WAV file...");

        if !frames.is_empty() {
            let first_frame = &frames[0];
            let sample_rate = first_frame.sample_rate;
            let channels = first_frame.channels;

            // Flatten all samples
            let mut all_samples: Vec<i16> = Vec::new();
            for frame in &frames {
                all_samples.extend_from_slice(&frame.samples);
            }

            // Write WAV file
            let spec = hound::WavSpec {
                channels,
                sample_rate,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };

            let mut writer = hound::WavWriter::create(&output_file, spec)?;
            for sample in all_samples {
                writer.write_sample(sample)?;
            }
            writer.finalize()?;

            let file_size = std::fs::metadata(&output_file)?.len();
            let duration_secs = frames.len() as f64 * 0.02; // 20ms per frame

            println!("✅ WAV file written!");
            println!("\n📊 Results:");
            println!("   - File: {}", output_file.display());
            println!("   - Duration: {:.1}s", duration_secs);
            println!(
                "   - Format: {}Hz, {} channels (stereo)",
                sample_rate, channels
            );
            println!("   - Size: {:.1} KB", file_size as f64 / 1024.0);
            println!("\n🎧 Test the file:");
            println!("   - System audio should play from LEFT speaker");
            println!("   - Microphone should play from RIGHT speaker");
            println!("   - If only one source had audio, that channel should work");
        } else {
            println!("⚠️  No frames captured");
        }

        println!("\n🎉 Multichannel test complete!");
    }

    Ok(())
}
