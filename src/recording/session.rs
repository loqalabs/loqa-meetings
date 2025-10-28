use anyhow::{Context, Result};
use futures::stream::StreamExt;
use tokio::sync::mpsc;
use tracing::{error, info};

use crate::audio::{AudioBackend, ChunkConfig, ChunkMetadata, ChunkedRecorder};
use crate::nats::{NatsClient, TranscriptMessage};

pub struct RecordingSession {
    meeting_id: String,
    nats_client: NatsClient,
    chunk_config: ChunkConfig,
    transcript_rx: mpsc::Receiver<TranscriptMessage>,
}

impl RecordingSession {
    /// Create new recording session with NATS integration
    pub async fn new(
        meeting_id: String,
        nats_url: &str,
        chunk_config: ChunkConfig,
    ) -> Result<Self> {
        info!("Creating recording session: {}", meeting_id);

        // Connect to NATS
        let nats_client = NatsClient::connect(nats_url, meeting_id.clone()).await?;

        // Subscribe to transcripts
        let mut subscriber = nats_client.subscribe_transcripts().await?;
        let (transcript_tx, transcript_rx) = mpsc::channel(100);

        // Spawn transcript listener task
        tokio::spawn(async move {
            while let Some(msg) = subscriber.next().await {
                match serde_json::from_slice::<TranscriptMessage>(&msg.payload) {
                    Ok(transcript) => {
                        info!(
                            "Received transcript: {} (partial={}, confidence={:.2})",
                            transcript.text, transcript.partial, transcript.confidence
                        );
                        if let Err(e) = transcript_tx.send(transcript).await {
                            error!("Failed to forward transcript: {}", e);
                            break;
                        }
                    }
                    Err(e) => {
                        error!("Failed to parse transcript message: {}", e);
                    }
                }
            }
        });

        Ok(Self {
            meeting_id,
            nats_client,
            chunk_config,
            transcript_rx,
        })
    }

    /// Start recording and publish frames to NATS
    ///
    /// This method takes ownership of the audio backend and processes frames until stopped.
    pub async fn record(
        self,
        mut backend: Box<dyn AudioBackend>,
    ) -> Result<Vec<ChunkMetadata>> {
        info!("Starting recording session: {}", self.meeting_id);

        // Start audio backend and get frame receiver
        let mut audio_rx = backend.start().await?;

        // Create chunked recorder
        let mut recorder = ChunkedRecorder::new(self.chunk_config)?;

        // Process frames: save to disk AND publish to NATS
        let (chunk_tx, chunk_rx) = mpsc::channel(100);
        let nats_client = self.nats_client;
        let meeting_id = self.meeting_id.clone();

        // Spawn frame processor task
        let publish_handle = tokio::spawn(async move {
            let mut chunk_index = 0;

            while let Some(frame) = audio_rx.recv().await {
                // Forward to chunked recorder
                if let Err(e) = chunk_tx.send(frame.clone()).await {
                    error!("Failed to forward frame to recorder: {}", e);
                    break;
                }

                // Publish to NATS (convert samples to bytes)
                let pcm_bytes: Vec<u8> = frame.samples.iter().flat_map(|&s| s.to_le_bytes()).collect();

                if let Err(e) = nats_client
                    .publish_audio_frame(
                        &pcm_bytes,
                        frame.sample_rate,
                        frame.channels,
                        chunk_index,
                        false, // not final yet
                    )
                    .await
                {
                    error!("Failed to publish audio frame to NATS: {}", e);
                    // Continue recording even if NATS publish fails
                }

                chunk_index += 1;
            }

            // Send final frame marker
            if let Err(e) = nats_client
                .publish_audio_frame(&[], 16000, 1, chunk_index, true)
                .await
            {
                error!("Failed to publish final frame marker: {}", e);
            }

            drop(chunk_tx); // Close channel to signal recorder to stop
        });

        // Run chunked recorder
        let metadata = recorder.record(chunk_rx).await?;

        // Wait for publish task to complete
        publish_handle.await.context("Publish task panicked")?;

        info!(
            "Recording session complete: {} ({} chunks)",
            meeting_id,
            metadata.len()
        );

        Ok(metadata)
    }

    /// Get next transcript (blocking)
    pub async fn next_transcript(&mut self) -> Option<TranscriptMessage> {
        self.transcript_rx.recv().await
    }

    /// Stop recording (called externally if record() is running in background)
    pub async fn stop(self) -> Result<()> {
        info!("Stopping recording session: {}", self.meeting_id);
        self.nats_client.close().await?;
        Ok(())
    }
}
