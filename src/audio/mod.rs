pub mod backend;
pub mod chunk;
pub mod file;

#[cfg(target_os = "macos")]
pub mod macos;

pub use backend::{AudioBackend, AudioBackendConfig, AudioBackendFactory, AudioFrame, AudioSource};
pub use chunk::{ChunkConfig, ChunkMetadata, ChunkedRecorder};
pub use file::AudioFile;
