// macOS audio backend using ScreenCaptureKit for system audio

use anyhow::{bail, Result};
use tokio::sync::mpsc;
use tracing::info;

use super::backend::{AudioBackend, AudioBackendConfig, AudioFrame};
use crate::screencapture;

/// macOS audio backend
///
/// Captures system audio using ScreenCaptureKit (macOS 13.0+)
pub struct MacOSBackend {
    config: AudioBackendConfig,
    session: Option<screencapture::ScreenCaptureSession>,
    capturing: bool,
}

impl MacOSBackend {
    pub fn new(config: AudioBackendConfig) -> Result<Self> {
        // Check if ScreenCaptureKit is available
        if !screencapture::is_available() {
            bail!(
                "ScreenCaptureKit is not available on this system. \
                Requires macOS 13.0 (Ventura, October 2022) or later."
            );
        }

        info!(
            "macOS backend initialized ({}Hz, {} channels)",
            config.target_sample_rate, config.target_channels
        );

        Ok(Self {
            config,
            session: None,
            capturing: false,
        })
    }
}

#[async_trait::async_trait]
impl AudioBackend for MacOSBackend {
    async fn start(&mut self) -> Result<mpsc::Receiver<AudioFrame>> {
        if self.capturing {
            bail!("Already capturing");
        }

        info!("Starting macOS ScreenCaptureKit audio capture");

        // Create capture session
        let mut session = screencapture::ScreenCaptureSession::new(
            self.config.target_sample_rate,
            self.config.target_channels,
        );

        // Start capture
        let rx = session.start()?;

        self.session = Some(session);
        self.capturing = true;

        info!("macOS audio capture started successfully");

        Ok(rx)
    }

    async fn stop(&mut self) -> Result<()> {
        if !self.capturing {
            return Ok(());
        }

        info!("Stopping macOS audio capture");

        if let Some(mut session) = self.session.take() {
            session.stop()?;
        }

        self.capturing = false;

        info!("macOS audio capture stopped");

        Ok(())
    }

    fn is_capturing(&self) -> bool {
        self.capturing
    }

    fn name(&self) -> &str {
        "macOS ScreenCaptureKit"
    }
}
