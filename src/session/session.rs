use super::config::SessionConfig;
use super::stats::{SessionStats, TranscriptSegment};
use crate::audio::{AudioBackendConfig, AudioBackendFactory, AudioFrame, AudioSource};
use crate::nats::{NatsClient, TranscriptMessage};
use anyhow::{Context, Result};
use chrono::Utc;
use futures::stream::StreamExt;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

/// A recording session that manages audio capture, NATS publishing, and transcript collection
pub struct RecordingSession {
    /// Session configuration
    config: SessionConfig,

    /// NATS client for publishing audio and receiving transcripts
    nats_client: Arc<NatsClient>,

    /// When the session started
    started_at: chrono::DateTime<chrono::Utc>,

    /// Whether recording is currently active
    is_recording: Arc<AtomicBool>,

    /// Number of chunks recorded
    chunks_recorded: Arc<AtomicUsize>,

    /// Accumulated transcript segments
    transcript_segments: Arc<Mutex<Vec<TranscriptSegment>>>,

    /// Handle for the audio processing task
    audio_task_handle: Arc<Mutex<Option<JoinHandle<()>>>>,

    /// Handle for the transcript receiving task
    transcript_task_handle: Arc<Mutex<Option<JoinHandle<()>>>>,

    /// Frame sequence counter
    frame_sequence: Arc<AtomicUsize>,
}

impl RecordingSession {
    /// Create a new recording session
    pub async fn new(config: SessionConfig) -> Result<Self> {
        info!("Creating recording session: {}", config.session_id);

        // Connect to NATS
        let nats_client = Arc::new(
            NatsClient::connect(&config.nats_url, config.session_id.clone())
                .await
                .context("Failed to connect to NATS")?,
        );

        Ok(Self {
            config,
            nats_client,
            started_at: Utc::now(),
            is_recording: Arc::new(AtomicBool::new(false)),
            chunks_recorded: Arc::new(AtomicUsize::new(0)),
            transcript_segments: Arc::new(Mutex::new(Vec::new())),
            audio_task_handle: Arc::new(Mutex::new(None)),
            transcript_task_handle: Arc::new(Mutex::new(None)),
            frame_sequence: Arc::new(AtomicUsize::new(0)),
        })
    }

    /// Start recording
    pub async fn start(&self) -> Result<()> {
        if self.is_recording.load(Ordering::SeqCst) {
            warn!("Recording already started");
            return Ok(());
        }

        info!("Starting recording session: {}", self.config.session_id);

        // Mark as recording
        self.is_recording.store(true, Ordering::SeqCst);

        // Create audio backend
        let backend_config = AudioBackendConfig {
            target_sample_rate: self.config.sample_rate,
            target_channels: self.config.channels,
            buffer_duration_ms: 100, // 100ms latency
        };

        let mut audio_backend = AudioBackendFactory::create(AudioSource::System, backend_config)
            .context("Failed to create audio backend")?;

        // Start capturing audio
        let mut audio_rx = audio_backend
            .start()
            .await
            .context("Failed to start audio capture")?;

        // Spawn audio processing task
        let nats_client = Arc::clone(&self.nats_client);
        let is_recording = Arc::clone(&self.is_recording);
        let frame_sequence = Arc::clone(&self.frame_sequence);
        let chunks_recorded = Arc::clone(&self.chunks_recorded);
        let sample_rate = self.config.sample_rate;
        let channels = self.config.channels;

        let audio_task = tokio::spawn(async move {
            info!("Audio processing task started");

            while let Some(frame) = audio_rx.recv().await {
                if !is_recording.load(Ordering::SeqCst) {
                    break;
                }

                // Process frame: downsample and convert to mono if needed
                let processed_frame = Self::process_frame(frame, sample_rate, channels);

                // Convert to PCM bytes
                let pcm_bytes: Vec<u8> = processed_frame
                    .samples
                    .iter()
                    .flat_map(|s| s.to_le_bytes())
                    .collect();

                // Get sequence number
                let seq = frame_sequence.fetch_add(1, Ordering::SeqCst);

                // Publish to NATS
                if let Err(e) = nats_client
                    .publish_audio_frame(&pcm_bytes, sample_rate, channels, seq as u32, false)
                    .await
                {
                    error!("Failed to publish audio frame: {}", e);
                }

                // Update chunks count every 100 frames (~10 seconds at 10 frames/sec)
                if seq % 100 == 0 {
                    chunks_recorded.store(seq / 100, Ordering::SeqCst);
                }
            }

            info!("Audio processing task stopped");

            // Send final frame
            if let Err(e) = nats_client
                .publish_audio_frame(
                    &[],
                    sample_rate,
                    channels,
                    frame_sequence.load(Ordering::SeqCst) as u32,
                    true,
                )
                .await
            {
                error!("Failed to send final frame: {}", e);
            }

            // Stop the backend
            if let Err(e) = audio_backend.stop().await {
                error!("Failed to stop audio backend: {}", e);
            }
        });

        {
            let mut handle = self.audio_task_handle.lock().await;
            *handle = Some(audio_task);
        }

        // Subscribe to transcripts
        let mut transcript_sub = self
            .nats_client
            .subscribe_transcripts()
            .await
            .context("Failed to subscribe to transcripts")?;

        // Spawn transcript receiving task
        let transcript_segments = Arc::clone(&self.transcript_segments);
        let session_id = self.config.session_id.clone();
        let is_recording = Arc::clone(&self.is_recording);

        let transcript_task = tokio::spawn(async move {
            info!("Transcript receiving task started");

            while let Some(msg) = transcript_sub.next().await {
                if !is_recording.load(Ordering::SeqCst) {
                    break;
                }

                // Parse transcript message
                match serde_json::from_slice::<TranscriptMessage>(&msg.payload) {
                    Ok(transcript) => {
                        // Filter by session_id
                        if transcript.session_id != session_id {
                            continue;
                        }

                        // Create segment
                        let segment = TranscriptSegment {
                            text: transcript.text.clone(),
                            timestamp: Utc::now(),
                            confidence: transcript.confidence,
                            partial: transcript.partial,
                        };

                        // Store segment
                        {
                            let mut segments = transcript_segments.lock().await;
                            segments.push(segment);
                        }

                        // Log to console
                        if transcript.partial {
                            print!("\r{}", transcript.text);
                            std::io::Write::flush(&mut std::io::stdout()).ok();
                        } else {
                            println!("\n{}", transcript.text);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse transcript message: {}", e);
                    }
                }
            }

            info!("Transcript receiving task stopped");
        });

        {
            let mut handle = self.transcript_task_handle.lock().await;
            *handle = Some(transcript_task);
        }

        info!("Recording session started successfully");

        Ok(())
    }

    /// Stop recording
    pub async fn stop(&self) -> Result<SessionStats> {
        if !self.is_recording.load(Ordering::SeqCst) {
            warn!("Recording not active");
            return self.get_stats().await;
        }

        info!("Stopping recording session: {}", self.config.session_id);

        // Mark as stopped (this will signal tasks to finish)
        self.is_recording.store(false, Ordering::SeqCst);

        // Wait for audio task to finish
        {
            let mut handle = self.audio_task_handle.lock().await;
            if let Some(task) = handle.take() {
                if let Err(e) = task.await {
                    error!("Audio task panicked: {}", e);
                }
            }
        }

        // Wait for transcript task to finish
        {
            let mut handle = self.transcript_task_handle.lock().await;
            if let Some(task) = handle.take() {
                if let Err(e) = task.await {
                    error!("Transcript task panicked: {}", e);
                }
            }
        }

        info!("Recording session stopped successfully");

        // Return final stats
        self.get_stats().await
    }

    /// Get current session statistics
    pub async fn get_stats(&self) -> Result<SessionStats> {
        let duration = Utc::now().signed_duration_since(self.started_at);

        let transcript_count = {
            let segments = self.transcript_segments.lock().await;
            segments.len()
        };

        Ok(SessionStats {
            is_recording: self.is_recording.load(Ordering::SeqCst),
            started_at: self.started_at,
            duration_secs: duration.num_milliseconds() as f64 / 1000.0,
            chunks_count: self.chunks_recorded.load(Ordering::SeqCst),
            transcript_segments_count: transcript_count,
        })
    }

    /// Get accumulated transcript
    pub async fn get_transcript(&self) -> Vec<TranscriptSegment> {
        let segments = self.transcript_segments.lock().await;
        segments.clone()
    }

    /// Process audio frame: downsample and convert to target format
    fn process_frame(
        frame: AudioFrame,
        target_sample_rate: u32,
        target_channels: u16,
    ) -> AudioFrame {
        let mut processed = frame;

        // Downsample if needed
        if processed.sample_rate != target_sample_rate {
            processed = Self::downsample_frame(processed, target_sample_rate);
        }

        // Convert to mono if needed
        if processed.channels != target_channels && target_channels == 1 {
            processed = Self::stereo_to_mono(processed);
        }

        processed
    }

    /// Downsample audio frame by decimation
    fn downsample_frame(frame: AudioFrame, target_rate: u32) -> AudioFrame {
        if frame.sample_rate == target_rate {
            return frame;
        }

        let ratio = frame.sample_rate / target_rate;
        if ratio <= 1 {
            return frame; // Can't upsample
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

    /// Convert stereo to mono by summing channels
    fn stereo_to_mono(frame: AudioFrame) -> AudioFrame {
        if frame.channels == 1 {
            return frame;
        }

        if frame.channels != 2 {
            return frame; // Only support stereo -> mono
        }

        let mut mono_samples = Vec::with_capacity(frame.samples.len() / 2);

        // Sum left and right channels (no division to preserve volume)
        for chunk in frame.samples.chunks_exact(2) {
            let left = chunk[0] as i32;
            let right = chunk[1] as i32;
            let sum = left + right;
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
}
