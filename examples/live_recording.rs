use anyhow::Result;
use futures::stream::StreamExt;
use loqa_meetings::{AudioBackendConfig, AudioBackendFactory, AudioSource, NatsClient, TranscriptMessage};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{sleep, timeout};
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("🎙️  Starting live recording test with system audio");

    // 1. Connect to NATS
    let meeting_id = format!("live-test-{}", chrono::Utc::now().timestamp());
    let nats = NatsClient::connect("nats://localhost:4222", meeting_id.clone()).await?;
    info!("✅ Connected to NATS");

    // 2. Subscribe to transcripts
    let mut subscriber = nats.subscribe_transcripts().await?;
    info!("✅ Subscribed to transcripts");

    // 3. Create macOS audio backend (system audio)
    let backend_config = AudioBackendConfig::default(); // 16kHz mono
    let mut backend = AudioBackendFactory::create(AudioSource::System, backend_config)?;
    info!("✅ Created audio backend: macOS ScreenCaptureKit");

    // 4. Spawn transcript listener task
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_clone = stop_flag.clone();

    let transcript_handle = tokio::spawn(async move {
        info!("📝 Listening for transcripts...");
        let mut transcript_count = 0;

        loop {
            if stop_flag_clone.load(Ordering::Relaxed) {
                info!("🛑 Stop signal received in transcript listener");
                break;
            }

            match timeout(Duration::from_millis(500), subscriber.next()).await {
                Ok(Some(msg)) => {
                    if let Ok(transcript) = serde_json::from_slice::<TranscriptMessage>(&msg.payload) {
                        transcript_count += 1;
                        let conf_str = transcript
                            .confidence
                            .map(|c| format!("{:.2}%", c * 100.0))
                            .unwrap_or_else(|| "N/A".to_string());

                        let status = if transcript.partial { "PARTIAL" } else { "FINAL" };

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
                    // Timeout - continue checking stop flag
                    continue;
                }
            }
        }

        info!("✅ Received {} transcripts total", transcript_count);
    });

    // 5. Start capturing audio
    info!("🎤 Starting audio capture for 15 seconds...");
    info!("💬 Please speak into your microphone or play some audio!");
    info!("");

    let mut audio_rx = backend.start().await?;
    let mut chunk_index = 0;
    let start_time = tokio::time::Instant::now();
    let recording_duration = Duration::from_secs(15);

    // 6. Publish audio frames to NATS
    while let Some(frame) = audio_rx.recv().await {
        // Convert samples to bytes
        let pcm_bytes: Vec<u8> = frame.samples.iter().flat_map(|&s| s.to_le_bytes()).collect();

        // Check if we've exceeded recording duration
        let elapsed = start_time.elapsed();
        let is_final = elapsed >= recording_duration;

        nats.publish_audio_frame(
            &pcm_bytes,
            frame.sample_rate,
            frame.channels,
            chunk_index,
            is_final,
        )
        .await?;

        if chunk_index % 10 == 0 {
            info!("📤 Published frame {} ({:.1}s elapsed)", chunk_index, elapsed.as_secs_f32());
        }

        chunk_index += 1;

        if is_final {
            info!("⏰ Recording duration reached - sending final frame");
            break;
        }
    }

    // 7. Stop backend
    backend.stop().await?;
    info!("⏹️  Audio capture stopped");

    // 8. Wait for final transcripts
    info!("⏳ Waiting for final transcripts (5s)...");
    sleep(Duration::from_secs(5)).await;

    // Signal transcript listener to stop
    stop_flag.store(true, Ordering::Relaxed);

    // Wait for transcript listener to finish
    match timeout(Duration::from_secs(2), transcript_handle).await {
        Ok(Ok(_)) => info!("✅ Transcript listener completed"),
        Ok(Err(e)) => info!("❌ Transcript listener error: {}", e),
        Err(_) => info!("⏱️  Transcript listener timeout"),
    }

    info!("🏁 Live recording test complete!");

    Ok(())
}
