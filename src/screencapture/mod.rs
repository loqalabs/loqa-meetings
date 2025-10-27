// Rust FFI interface to Swift ScreenCaptureKit bridge
//
// Platform: macOS 13.0+ only
//
// This module provides a safe Rust interface to capture system audio
// on macOS using ScreenCaptureKit via Swift FFI.

use anyhow::{bail, Result};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tracing::{error, info};

use crate::audio::backend::{AudioFrame, AudioStreamSource};

// MARK: - FFI declarations

#[cfg(target_os = "macos")]
#[link(name = "loqa_screencapture", kind = "static")]
extern "C" {
    fn loqa_screencapture_is_available() -> bool;

    fn loqa_screencapture_start(
        sample_rate: u32,
        channels: u16,
        callback: extern "C" fn(*const i16, i32, u32, u16, u8),
    ) -> i32;

    fn loqa_screencapture_stop() -> i32;
}

// MARK: - Safe Rust interface

/// Check if ScreenCaptureKit is available on this system
#[cfg(target_os = "macos")]
pub fn is_available() -> bool {
    unsafe { loqa_screencapture_is_available() }
}

#[cfg(not(target_os = "macos"))]
pub fn is_available() -> bool {
    false
}

/// ScreenCaptureKit audio capture session
#[cfg(target_os = "macos")]
pub struct ScreenCaptureSession {
    sample_rate: u32,
    channels: u16,
    audio_tx: Option<mpsc::Sender<AudioFrame>>,
    start_time_ms: Arc<Mutex<Option<u64>>>,
}

#[cfg(target_os = "macos")]
impl ScreenCaptureSession {
    /// Create a new capture session
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        Self {
            sample_rate,
            channels,
            audio_tx: None,
            start_time_ms: Arc::new(Mutex::new(None)),
        }
    }

    /// Start capturing system audio
    ///
    /// Returns a channel receiver that will receive audio frames
    pub fn start(&mut self) -> Result<mpsc::Receiver<AudioFrame>> {
        if !is_available() {
            bail!("ScreenCaptureKit is not available (requires macOS 13.0+)");
        }

        info!(
            "Starting ScreenCaptureKit capture ({}Hz, {} channels)",
            self.sample_rate, self.channels
        );

        // Create channel for audio frames
        let (tx, rx) = mpsc::channel(100);
        self.audio_tx = Some(tx.clone());

        // Initialize start time
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        *self.start_time_ms.lock().unwrap() = Some(now_ms);

        // Store context for callback
        let tx_ptr = Box::into_raw(Box::new(tx));
        let start_time_ptr = Arc::into_raw(Arc::clone(&self.start_time_ms));

        unsafe {
            GLOBAL_AUDIO_TX = tx_ptr;
            GLOBAL_START_TIME = start_time_ptr as *mut _;
        }

        // Start capture
        let result = unsafe {
            loqa_screencapture_start(
                self.sample_rate,
                self.channels,
                audio_callback,
            )
        };

        if result != 0 {
            bail!("Failed to start ScreenCaptureKit capture (error code: {})", result);
        }

        info!("ScreenCaptureKit capture started successfully");

        Ok(rx)
    }

    /// Stop capturing audio
    pub fn stop(&mut self) -> Result<()> {
        info!("Stopping ScreenCaptureKit capture");

        let result = unsafe { loqa_screencapture_stop() };

        // Clean up global pointers
        unsafe {
            if !GLOBAL_AUDIO_TX.is_null() {
                let _ = Box::from_raw(GLOBAL_AUDIO_TX);
                GLOBAL_AUDIO_TX = std::ptr::null_mut();
            }
            if !GLOBAL_START_TIME.is_null() {
                let _ = Arc::from_raw(GLOBAL_START_TIME);
                GLOBAL_START_TIME = std::ptr::null_mut();
            }
        }

        self.audio_tx = None;
        *self.start_time_ms.lock().unwrap() = None;

        if result != 0 {
            bail!("Failed to stop ScreenCaptureKit capture (error code: {})", result);
        }

        info!("ScreenCaptureKit capture stopped successfully");

        Ok(())
    }

    /// Check if currently capturing
    pub fn is_capturing(&self) -> bool {
        self.audio_tx.is_some()
    }
}

// MARK: - Audio callback

#[cfg(target_os = "macos")]
static mut GLOBAL_AUDIO_TX: *mut mpsc::Sender<AudioFrame> = std::ptr::null_mut();

#[cfg(target_os = "macos")]
static mut GLOBAL_START_TIME: *mut Mutex<Option<u64>> = std::ptr::null_mut();

#[cfg(target_os = "macos")]
extern "C" fn audio_callback(
    samples_ptr: *const i16,
    sample_count: i32,
    sample_rate: u32,
    channels: u16,
    stream_type: u8,
) {
    if samples_ptr.is_null() || sample_count <= 0 {
        return;
    }

    unsafe {
        // Get global sender
        if GLOBAL_AUDIO_TX.is_null() {
            error!("Audio callback called but sender is null");
            return;
        }

        let tx = &*GLOBAL_AUDIO_TX;

        // Get start time
        let start_time_ms = if GLOBAL_START_TIME.is_null() {
            0
        } else {
            (*GLOBAL_START_TIME).lock().unwrap().unwrap_or(0)
        };

        // Calculate timestamp
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let timestamp_ms = now_ms - start_time_ms;

        // Copy samples
        let samples = std::slice::from_raw_parts(samples_ptr, sample_count as usize).to_vec();

        // Determine stream source (0 = system, 1 = microphone)
        let source = if stream_type == 1 {
            AudioStreamSource::Microphone
        } else {
            AudioStreamSource::System
        };

        // Create audio frame
        let frame = AudioFrame {
            samples,
            sample_rate,
            channels,
            timestamp_ms,
            source,
        };

        // Send to channel (non-blocking)
        if let Err(e) = tx.try_send(frame) {
            error!("Failed to send audio frame: {}", e);
        }
    }
}

// MARK: - Placeholder for non-macOS platforms

#[cfg(not(target_os = "macos"))]
pub struct ScreenCaptureSession;

#[cfg(not(target_os = "macos"))]
impl ScreenCaptureSession {
    pub fn new(_sample_rate: u32, _channels: u16) -> Self {
        Self
    }

    pub fn start(&mut self) -> Result<mpsc::Receiver<AudioFrame>> {
        bail!("ScreenCaptureKit is only available on macOS")
    }

    pub fn stop(&mut self) -> Result<()> {
        Ok(())
    }

    pub fn is_capturing(&self) -> bool {
        false
    }
}
