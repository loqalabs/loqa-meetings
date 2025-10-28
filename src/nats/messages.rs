use serde::{Deserialize, Serialize};

/// Audio frame message published to NATS
#[derive(Debug, Serialize, Deserialize)]
pub struct AudioFrameMessage {
    pub session_id: String,
    pub sequence: u32,  // Frame sequence number (matches loqa-core protocol)
    pub pcm: String,  // Base64-encoded PCM bytes
    pub sample_rate: u32,
    pub channels: u16,
    pub timestamp: String,  // RFC3339 timestamp
    #[serde(rename = "final")]
    pub final_frame: bool,
}

/// Transcript message received from STT service
#[derive(Debug, Serialize, Deserialize)]
pub struct TranscriptMessage {
    pub session_id: String,
    pub text: String,
    pub partial: bool,
    pub timestamp: String,
    #[serde(default)]
    pub confidence: Option<f32>,
}
