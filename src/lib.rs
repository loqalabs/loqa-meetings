pub mod audio;
pub mod config;
pub mod http;
pub mod nats;
pub mod screencapture;
pub mod session;

pub use audio::{
    AudioBackend, AudioBackendConfig, AudioBackendFactory, AudioFile, AudioFrame, AudioSource,
    AudioStreamSource, ChunkConfig, ChunkMetadata, ChunkedRecorder,
};
pub use config::Config;
pub use http::{create_router, AppState};
pub use nats::{AudioFrameMessage, NatsClient, TranscriptMessage};
pub use session::{RecordingSession, SessionConfig, SessionStats, TranscriptSegment};
