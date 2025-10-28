use anyhow::Result;
use tokio::sync::mpsc;

/// Audio stream source type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioStreamSource {
    /// System audio (applications, browser, etc.)
    System,
    /// Microphone input
    Microphone,
}

/// Audio sample data (16-bit PCM, interleaved)
#[derive(Debug, Clone)]
pub struct AudioFrame {
    /// Raw audio samples (i16 PCM, interleaved)
    pub samples: Vec<i16>,
    /// Sample rate in Hz
    pub sample_rate: u32,
    /// Number of channels
    pub channels: u16,
    /// Timestamp in milliseconds since recording started
    pub timestamp_ms: u64,
    /// Audio stream source (system or microphone)
    pub source: AudioStreamSource,
}

/// Configuration for audio backend
#[derive(Debug, Clone)]
pub struct AudioBackendConfig {
    /// Target sample rate (will resample if needed)
    pub target_sample_rate: u32,
    /// Target channel count (1 = mono, 2 = stereo)
    pub target_channels: u16,
    /// Buffer size in milliseconds (affects latency)
    pub buffer_duration_ms: u64,
}

impl Default for AudioBackendConfig {
    fn default() -> Self {
        Self {
            target_sample_rate: 16000, // 16kHz for Whisper
            target_channels: 1,        // Mono
            buffer_duration_ms: 100,   // 100ms buffers
        }
    }
}

/// Audio capture backend trait
///
/// Platform-specific implementations:
/// - macOS: ScreenCaptureKit for system audio + cpal for microphone
/// - iOS: cpal for microphone only (system audio not available)
/// - File: Read from audio file (for testing/batch processing)
#[async_trait::async_trait]
pub trait AudioBackend: Send + Sync {
    /// Start capturing audio
    ///
    /// Returns a channel receiver that will receive audio frames
    async fn start(&mut self) -> Result<mpsc::Receiver<AudioFrame>>;

    /// Stop capturing audio
    async fn stop(&mut self) -> Result<()>;

    /// Check if backend is currently capturing
    fn is_capturing(&self) -> bool;

    /// Get backend name for logging
    fn name(&self) -> &str;
}

/// Audio backend factory
pub struct AudioBackendFactory;

impl AudioBackendFactory {
    /// Create audio backend based on platform and configuration
    pub fn create(
        source: AudioSource,
        config: AudioBackendConfig,
    ) -> Result<Box<dyn AudioBackend>> {
        match source {
            AudioSource::System => {
                #[cfg(target_os = "macos")]
                {
                    use super::macos::MacOSBackend;
                    let backend = MacOSBackend::new(config)?;
                    Ok(Box::new(backend))
                }

                #[cfg(not(target_os = "macos"))]
                {
                    anyhow::bail!("System audio capture is only supported on macOS")
                }
            }

            AudioSource::Microphone => {
                todo!("Create cpal microphone backend")
            }

            AudioSource::File(path) => {
                todo!("Create file-based backend for path: {:?}", path)
            }
        }
    }
}

/// Audio source type
#[derive(Debug, Clone)]
pub enum AudioSource {
    /// System audio (macOS ScreenCaptureKit only)
    System,
    /// Microphone input (all platforms)
    Microphone,
    /// File input (for testing/batch processing)
    File(String),
}
