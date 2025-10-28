use anyhow::{Context, Result};
use async_nats::Client;
use base64::Engine;
use tracing::info;

pub struct NatsClient {
    client: Client,
    meeting_id: String,
}

impl NatsClient {
    /// Connect to NATS server
    pub async fn connect(url: &str, meeting_id: String) -> Result<Self> {
        info!("Connecting to NATS at {}", url);

        let client = async_nats::connect(url)
            .await
            .context("Failed to connect to NATS")?;

        info!("Connected to NATS successfully");

        Ok(Self { client, meeting_id })
    }

    /// Publish audio frame to NATS
    pub async fn publish_audio_frame(
        &self,
        pcm_bytes: &[u8],
        sample_rate: u32,
        channels: u16,
        chunk_index: u32,
        is_final: bool,
    ) -> Result<()> {
        let subject = format!("audio.frame.meeting-{}", self.meeting_id);

        let message = super::messages::AudioFrameMessage {
            session_id: self.meeting_id.clone(),
            sequence: chunk_index,
            pcm: base64::engine::general_purpose::STANDARD.encode(pcm_bytes),
            sample_rate,
            channels,
            timestamp: chrono::Utc::now().to_rfc3339(),
            final_frame: is_final,
        };

        let payload = serde_json::to_vec(&message)?;

        self.client.publish(subject.clone(), payload.into())
            .await
            .context("Failed to publish audio frame")?;

        info!(
            "Published audio frame to {} (chunk={}, bytes={}, final={})",
            subject, chunk_index, pcm_bytes.len(), is_final
        );

        Ok(())
    }

    /// Subscribe to transcript messages
    pub async fn subscribe_transcripts(&self) -> Result<async_nats::Subscriber> {
        // Subscribe to all transcripts (partial and final)
        // loqa-core publishes to stt.text.partial and stt.text.final
        // We filter by session_id in the message payload
        let subject = "stt.text.>";

        info!("Subscribing to transcripts on {}", subject);

        let subscriber = self.client.subscribe(subject)
            .await
            .context("Failed to subscribe to transcripts")?;

        info!("Subscribed to {}", subject);

        Ok(subscriber)
    }

    /// Close NATS connection
    pub async fn close(self) -> Result<()> {
        info!("Closing NATS connection");
        // async-nats handles cleanup on drop
        Ok(())
    }
}
