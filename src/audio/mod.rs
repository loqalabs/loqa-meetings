pub mod backend;
pub mod chunk;
pub mod file;
pub mod mixer;

#[cfg(target_os = "macos")]
pub mod macos;

pub use backend::{AudioBackend, AudioBackendConfig, AudioBackendFactory, AudioFrame, AudioSource, AudioStreamSource};
pub use chunk::{ChunkConfig, ChunkMetadata, ChunkedRecorder};
pub use file::AudioFile;
pub use mixer::{AudioMixer, MixerConfig};
