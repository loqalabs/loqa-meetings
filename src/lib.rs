pub mod audio;
pub mod config;
pub mod nats;
pub mod recording;
pub mod screencapture;

pub use audio::{
    AudioBackend, AudioBackendConfig, AudioBackendFactory, AudioFile, AudioFrame, AudioMixer,
    AudioSource, AudioStreamSource, ChunkConfig, ChunkMetadata, ChunkedRecorder, MixerConfig,
};
pub use config::Config;
pub use nats::{AudioFrameMessage, NatsClient, TranscriptMessage};
pub use recording::RecordingSession;
