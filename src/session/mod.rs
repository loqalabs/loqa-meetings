//! Recording session management
//!
//! This module provides the `RecordingSession` abstraction that manages:
//! - Audio capture from system/microphone
//! - Audio processing (downsampling, mono conversion)
//! - NATS publishing for STT service
//! - Transcript collection and storage
//! - Session statistics and state management

mod config;
mod session;
mod stats;

pub use config::SessionConfig;
pub use session::RecordingSession;
pub use stats::{SessionStats, TranscriptSegment};
