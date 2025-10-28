use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Statistics about a recording session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStats {
    /// Whether recording is currently active
    pub is_recording: bool,

    /// When the recording started
    pub started_at: DateTime<Utc>,

    /// Total duration in seconds
    pub duration_secs: f64,

    /// Number of audio chunks recorded so far
    pub chunks_count: usize,

    /// Number of transcript segments received
    pub transcript_segments_count: usize,
}

/// A single transcript segment from the STT service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    /// Transcribed text
    pub text: String,

    /// When this segment was received
    pub timestamp: DateTime<Utc>,

    /// Confidence score (0.0 to 1.0), if available
    pub confidence: Option<f32>,

    /// Whether this is a partial (interim) result
    pub partial: bool,
}
