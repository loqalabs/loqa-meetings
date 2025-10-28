// Live Recording Example: Real-time transcription with NATS
//
// This example demonstrates the complete Week 3 pipeline:
// 1. ScreenCaptureKit captures system audio + microphone (Swift-based mixing)
// 2. Audio is downsampled from 48kHz stereo to 16kHz mono for Whisper
// 3. Frames are published to NATS
// 4. loqa-core (Whisper) transcribes and publishes results back
// 5. We listen and display transcripts in real-time
//
// IMPORTANT: Requires macOS permissions:
// - System Settings → Privacy & Security → Screen Recording → Add Terminal/IDE
// - System Settings → Privacy & Security → Microphone → Add Terminal/IDE
//
// Prerequisites:
// - NATS server running: docker run -p 4222:4222 nats
// - loqa-core STT service running: cd loqa-core && cargo run
//
// Usage: cargo run --example live_transcription

use anyhow::Result;
use futures::stream::StreamExt;
use hound::{WavSpec, WavWriter};
use loqa_meetings::{
    AudioBackendConfig, AudioBackendFactory, AudioFrame, AudioSource, NatsClient, TranscriptMessage,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{sleep, timeout};
use tracing::info;

/// Simple downsampling by decimation (takes every Nth sample)
/// Converts 48kHz stereo to 16kHz stereo
fn downsample_frame(frame: AudioFrame, target_rate: u32) -> AudioFrame {
    if frame.sample_rate == target_rate {
        return frame; // Already at target rate
    }

    let ratio = frame.sample_rate / target_rate;
    if ratio <= 1 {
        return frame; // Can't upsample, return as-is
    }

    // Decimate: take every Nth sample
    let downsampled: Vec<i16> = frame
        .samples
        .iter()
        .step_by(ratio as usize)
        .copied()
        .collect();

    AudioFrame {
        samples: downsampled,
        sample_rate: target_rate,
        channels: frame.channels,
        timestamp_ms: frame.timestamp_ms,
        source: frame.source,
    }
}

/// Convert stereo to mono by summing left and right channels
/// Input samples are interleaved: [L, R, L, R, ...]
/// Output is mono: [M, M, M, ...]
/// Note: Does NOT average (no division) to preserve volume when one channel is silent
fn stereo_to_mono(frame: AudioFrame) -> AudioFrame {
    if frame.channels == 1 {
        return frame; // Already mono
    }

    if frame.channels != 2 {
        // Only support stereo -> mono conversion
        return frame;
    }

    let mut mono_samples = Vec::with_capacity(frame.samples.len() / 2);

    // Process pairs of samples (left, right)
    // Sum without dividing to preserve volume when one channel is silent
    for chunk in frame.samples.chunks_exact(2) {
        let left = chunk[0] as i32;
        let right = chunk[1] as i32;
        let sum = left + right;
        // Clamp to prevent overflow
        let mono = sum.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        mono_samples.push(mono);
    }

    AudioFrame {
        samples: mono_samples,
        sample_rate: frame.sample_rate,
        channels: 1,
        timestamp_ms: frame.timestamp_ms,
        source: frame.source,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("🎙️  Starting live recording with real-time transcription");

    // 1. Connect to NATS
    let meeting_id = format!("live-test-{}", chrono::Utc::now().timestamp());
    let nats = NatsClient::connect("nats://localhost:4222", meeting_id.clone()).await?;
    info!("✅ Connected to NATS (meeting: {})", meeting_id);

    // 2. Subscribe to transcripts
    let mut subscriber = nats.subscribe_transcripts().await?;
    info!("✅ Subscribed to transcripts");

    // 3. Create macOS audio backend
    // ScreenCaptureKit captures at 48kHz stereo (System→Left, Mic→Right)
    // Swift handles the mixing with zero-fill for silent sources
    let backend_config = AudioBackendConfig {
        target_sample_rate: 48000, // Native macOS rate (will downsample to 16kHz)
        target_channels: 2,        // Stereo (System→L, Mic→R)
        buffer_duration_ms: 100,
    };
    let mut backend = AudioBackendFactory::create(AudioSource::System, backend_config)?;
    info!("✅ Audio backend ready: ScreenCaptureKit (48kHz stereo → 16kHz mono)");
    info!("   System audio → LEFT channel");
    info!("   Microphone → RIGHT channel");

    // 4. Spawn transcript listener task
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_clone = stop_flag.clone();

    let transcript_handle = tokio::spawn(async move {
        info!("📝 Listening for transcripts...");
        let mut transcript_count = 0;

        loop {
            match timeout(Duration::from_millis(500), subscriber.next()).await {
                Ok(Some(msg)) => {
                    if let Ok(transcript) =
                        serde_json::from_slice::<TranscriptMessage>(&msg.payload)
                    {
                        transcript_count += 1;
                        let conf_str = transcript
                            .confidence
                            .map(|c| format!("{:.2}%", c * 100.0))
                            .unwrap_or_else(|| "N/A".to_string());

                        let status = if transcript.partial {
                            "PARTIAL"
                        } else {
                            "FINAL  "
                        };

                        info!(
                            "📝 [{}] #{}: \"{}\" (confidence: {})",
                            status, transcript_count, transcript.text, conf_str
                        );
                    }
                }
                Ok(None) => {
                    info!("⏹️  Transcript stream closed");
                    break;
                }
                Err(_) => {
                    // Timeout - check if we should stop
                    if stop_flag_clone.load(Ordering::Relaxed) {
                        info!("🛑 Stop signal received in transcript listener");
                        break;
                    }
                }
            }
        }

        info!("✅ Received {} transcripts total", transcript_count);
    });

    // 5. Start capturing audio
    info!("");
    info!("🎤 Starting audio capture for 15 seconds...");
    info!("💬 Speak into your microphone AND/OR play system audio!");
    info!("");

    let audio_rx = backend.start().await?;
    let start_time = tokio::time::Instant::now();
    let recording_duration = Duration::from_secs(15);

    // 6. Process frames: downsample and publish to NATS
    let mut chunk_index = 0;
    let mut last_pcm_bytes: Vec<u8> = Vec::new();
    let mut last_sample_rate = 16000;
    let mut last_channels = 2;

    // Collect all processed samples for debugging
    let mut all_processed_samples: Vec<i16> = Vec::new();

    tokio::pin!(audio_rx);
    'outer: loop {
        // Check if we've exceeded recording duration
        if start_time.elapsed() >= recording_duration {
            info!("⏰ Recording duration reached");
            break 'outer;
        }

        // Try to receive a frame with timeout
        match tokio::time::timeout(Duration::from_millis(100), audio_rx.recv()).await {
            Ok(Some(frame)) => {
                // Downsample from 48kHz stereo to 16kHz stereo
                let downsampled = downsample_frame(frame, 16000);

                // Convert from stereo to mono (Whisper expects mono)
                let mono = stereo_to_mono(downsampled);

                // Collect samples for debugging WAV file
                all_processed_samples.extend_from_slice(&mono.samples);

                // Convert samples to bytes
                let pcm_bytes: Vec<u8> =
                    mono.samples.iter().flat_map(|&s| s.to_le_bytes()).collect();

                // Store for potential final frame
                last_pcm_bytes = pcm_bytes.clone();
                last_sample_rate = mono.sample_rate;
                last_channels = mono.channels;

                // Publish to NATS for transcription
                nats.publish_audio_frame(
                    &pcm_bytes,
                    mono.sample_rate,
                    mono.channels,
                    chunk_index,
                    false, // Not final yet
                )
                .await?;

                if chunk_index % 10 == 0 {
                    info!(
                        "📤 Published frame {} ({:.1}s elapsed)",
                        chunk_index,
                        start_time.elapsed().as_secs_f32()
                    );
                }

                chunk_index += 1;
            }
            Ok(None) => {
                // Channel closed - audio capture stopped
                break 'outer;
            }
            Err(_) => {
                // Timeout - continue waiting for frames
            }
        }
    }

    // 7. Send final frame marker to trigger transcription
    if !last_pcm_bytes.is_empty() {
        info!("📤 Sending final frame marker");
        nats.publish_audio_frame(
            &last_pcm_bytes,
            last_sample_rate,
            last_channels,
            chunk_index,
            true, // This is the final frame
        )
        .await?;
    }

    // 8. Stop backend
    backend.stop().await?;
    info!("⏹️  Audio capture stopped");

    // 9. Wait for final transcripts
    info!("⏳ Waiting for final transcripts (10s)...");
    sleep(Duration::from_secs(10)).await;

    // Signal transcript listener to stop
    stop_flag.store(true, Ordering::Relaxed);

    // Wait for transcript listener to finish
    match timeout(Duration::from_secs(3), transcript_handle).await {
        Ok(Ok(_)) => info!("✅ Transcript listener completed"),
        Ok(Err(e)) => info!("❌ Transcript listener error: {}", e),
        Err(_) => info!("⏱️  Transcript listener timeout"),
    }

    info!("");
    info!("🏁 Live recording test complete!");
    info!("📊 Total frames published: {}", chunk_index);

    // Save processed audio to WAV file for debugging
    if !all_processed_samples.is_empty() {
        let output_path = shellexpand::tilde("~/.loqa/recordings/processed-16khz-mono.wav");
        std::fs::create_dir_all(std::path::Path::new(output_path.as_ref()).parent().unwrap())?;

        let spec = WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut writer = WavWriter::create(output_path.as_ref(), spec)?;
        for sample in all_processed_samples {
            writer.write_sample(sample)?;
        }
        writer.finalize()?;

        info!("💾 Saved processed audio to: {}", output_path);
    }

    Ok(())
}
