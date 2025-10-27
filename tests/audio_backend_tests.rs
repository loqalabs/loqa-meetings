// Unit tests for audio backend abstractions
//
// These tests verify the core audio types and interfaces work correctly.

use loqa_meetings::audio::{AudioBackendConfig, AudioFrame};

#[test]
fn test_audio_frame_creation() {
    let frame = AudioFrame {
        samples: vec![100, 200, 300],
        sample_rate: 16000,
        channels: 1,
        timestamp_ms: 1000,
    };

    assert_eq!(frame.samples.len(), 3);
    assert_eq!(frame.sample_rate, 16000);
    assert_eq!(frame.channels, 1);
    assert_eq!(frame.timestamp_ms, 1000);
}

#[test]
fn test_audio_frame_clone() {
    let frame = AudioFrame {
        samples: vec![1, 2, 3, 4, 5],
        sample_rate: 48000,
        channels: 2,
        timestamp_ms: 500,
    };

    let cloned = frame.clone();

    assert_eq!(frame.samples, cloned.samples);
    assert_eq!(frame.sample_rate, cloned.sample_rate);
    assert_eq!(frame.channels, cloned.channels);
    assert_eq!(frame.timestamp_ms, cloned.timestamp_ms);
}

#[test]
fn test_audio_backend_config_default() {
    let config = AudioBackendConfig::default();

    assert_eq!(config.target_sample_rate, 16000, "Default should be 16kHz for Whisper");
    assert_eq!(config.target_channels, 1, "Default should be mono");
    assert_eq!(config.buffer_duration_ms, 100, "Default buffer should be 100ms");
}

#[test]
fn test_audio_backend_config_custom() {
    let config = AudioBackendConfig {
        target_sample_rate: 48000,
        target_channels: 2,
        buffer_duration_ms: 200,
    };

    assert_eq!(config.target_sample_rate, 48000);
    assert_eq!(config.target_channels, 2);
    assert_eq!(config.buffer_duration_ms, 200);
}

#[test]
fn test_audio_backend_config_clone() {
    let config = AudioBackendConfig {
        target_sample_rate: 16000,
        target_channels: 1,
        buffer_duration_ms: 100,
    };

    let cloned = config.clone();

    assert_eq!(config.target_sample_rate, cloned.target_sample_rate);
    assert_eq!(config.target_channels, cloned.target_channels);
    assert_eq!(config.buffer_duration_ms, cloned.buffer_duration_ms);
}

#[test]
fn test_audio_frame_stereo_interleaved() {
    // Stereo audio: samples should be interleaved [L, R, L, R, ...]
    let frame = AudioFrame {
        samples: vec![100, 200, 150, 250, 175, 275], // 3 frames, 2 channels
        sample_rate: 44100,
        channels: 2,
        timestamp_ms: 0,
    };

    assert_eq!(frame.samples.len(), 6);
    assert_eq!(frame.channels, 2);
    // With 2 channels, we have 3 frames (6 samples / 2 channels)
    let num_frames = frame.samples.len() / frame.channels as usize;
    assert_eq!(num_frames, 3);
}

#[test]
fn test_audio_frame_timing_calculation() {
    // Test that we can calculate duration from sample count
    let sample_rate = 16000;
    let samples_per_frame = 1600; // 100ms at 16kHz

    let frame = AudioFrame {
        samples: vec![0i16; samples_per_frame],
        sample_rate,
        channels: 1,
        timestamp_ms: 0,
    };

    // Duration in seconds = samples / (sample_rate * channels)
    let duration_secs = frame.samples.len() as f64 / (frame.sample_rate as f64 * frame.channels as f64);
    assert!((duration_secs - 0.1).abs() < 0.001, "Duration should be 100ms");
}

#[test]
fn test_audio_backend_config_for_whisper() {
    // Whisper requires 16kHz mono
    let whisper_config = AudioBackendConfig {
        target_sample_rate: 16000,
        target_channels: 1,
        buffer_duration_ms: 100,
    };

    assert_eq!(whisper_config.target_sample_rate, 16000);
    assert_eq!(whisper_config.target_channels, 1);
}

#[test]
fn test_audio_backend_config_for_hifi() {
    // High-fidelity recording: 48kHz stereo
    let hifi_config = AudioBackendConfig {
        target_sample_rate: 48000,
        target_channels: 2,
        buffer_duration_ms: 50, // Lower latency for live monitoring
    };

    assert_eq!(hifi_config.target_sample_rate, 48000);
    assert_eq!(hifi_config.target_channels, 2);
    assert_eq!(hifi_config.buffer_duration_ms, 50);
}
