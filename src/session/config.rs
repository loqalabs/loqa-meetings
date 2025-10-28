use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Configuration for a recording session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Unique session identifier (e.g., "meeting-2025-10-28-standup")
    pub session_id: String,

    /// Duration of each audio chunk before rotating files
    /// Default: 300 seconds (5 minutes)
    pub chunk_duration: Duration,

    /// Sample rate for audio processing (Whisper expects 16kHz)
    pub sample_rate: u32,

    /// Number of audio channels (1 = mono, 2 = stereo)
    pub channels: u16,

    /// NATS server URL
    pub nats_url: String,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            session_id: format!("meeting-{}", uuid::Uuid::new_v4()),
            chunk_duration: Duration::from_secs(300), // 5 minutes
            sample_rate: 16000,                       // Whisper expects 16kHz
            channels: 1,                              // Mono
            nats_url: "nats://localhost:4222".to_string(),
        }
    }
}
