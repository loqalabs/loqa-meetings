use anyhow::Result;
use futures::stream::StreamExt;
use loqa_meetings::{AudioFile, NatsClient, TranscriptMessage};
use std::time::Duration;
use tokio::time::sleep;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("🧪 Testing NATS integration");

    // 1. Connect to NATS
    let nats = NatsClient::connect("nats://localhost:4222", "test-meeting".to_string()).await?;
    info!("✅ Connected to NATS");

    // 2. Subscribe to transcripts
    let mut subscriber = nats.subscribe_transcripts().await?;
    info!("✅ Subscribed to transcripts");

    // 3. Load test audio file
    let audio = AudioFile::open("tests/fixtures/sample-meeting.wav")?;
    info!("✅ Loaded audio file: {:.1}s", audio.duration_seconds);

    // 4. Send audio in chunks (simulate real-time)
    let chunk_size = 16000 * 5; // 5 seconds at 16kHz
    let mut chunk_index = 0;

    for chunk_start in (0..audio.samples.len()).step_by(chunk_size) {
        let chunk_end = (chunk_start + chunk_size).min(audio.samples.len());
        let chunk_samples = &audio.samples[chunk_start..chunk_end];

        // Convert to bytes
        let pcm_bytes: Vec<u8> = chunk_samples
            .iter()
            .flat_map(|&s| s.to_le_bytes())
            .collect();

        let is_final = chunk_end >= audio.samples.len();

        nats.publish_audio_frame(
            &pcm_bytes,
            audio.sample_rate,
            audio.channels,
            chunk_index,
            is_final,
        )
        .await?;

        info!(
            "📤 Sent chunk {} ({} samples, final={})",
            chunk_index,
            chunk_samples.len(),
            is_final
        );

        // Wait a bit (simulate real-time)
        sleep(Duration::from_millis(500)).await;

        // Check for transcripts (non-blocking with timeout)
        match tokio::time::timeout(Duration::from_millis(100), subscriber.next()).await {
            Ok(Some(msg)) => {
                if let Ok(transcript) = serde_json::from_slice::<TranscriptMessage>(&msg.payload) {
                    let conf_str = transcript
                        .confidence
                        .map(|c| format!("{:.2}", c))
                        .unwrap_or_else(|| "N/A".to_string());
                    info!(
                        "📝 Transcript: {} (confidence: {}, partial: {})",
                        transcript.text, conf_str, transcript.partial
                    );
                }
            }
            Ok(None) => break, // Subscription closed
            Err(_) => {} // Timeout - no transcript yet
        }

        chunk_index += 1;
    }

    info!("✅ All chunks sent");

    // Wait for final transcripts
    info!("⏳ Waiting for final transcripts (5s timeout)...");
    let timeout = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(msg) = subscriber.next().await {
            match serde_json::from_slice::<TranscriptMessage>(&msg.payload) {
                Ok(transcript) => {
                    let conf_str = transcript
                        .confidence
                        .map(|c| format!("{:.2}", c))
                        .unwrap_or_else(|| "N/A".to_string());
                    info!(
                        "📝 Final transcript: {} (confidence: {})",
                        transcript.text, conf_str
                    );
                }
                Err(e) => {
                    eprintln!("Failed to parse transcript: {}", e);
                }
            }
        }
    })
    .await;

    match timeout {
        Ok(_) => info!("✅ Test complete - all transcripts received"),
        Err(_) => info!("⏱️  Timeout reached - test complete"),
    }

    Ok(())
}
