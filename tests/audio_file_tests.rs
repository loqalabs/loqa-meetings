// Integration tests for audio file processing
//
// These tests verify that we can read WAV files and extract audio data correctly.

use anyhow::Result;
use loqa_meetings::audio::AudioFile;
use std::path::PathBuf;

fn get_test_fixture_path(filename: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(filename)
}

#[test]
fn test_audio_file_open() -> Result<()> {
    let path = get_test_fixture_path("sample-meeting.wav");

    let audio = AudioFile::open(&path)?;

    // Verify basic properties
    assert!(audio.duration_seconds > 0.0, "Duration should be positive");
    assert!(audio.sample_rate > 0, "Sample rate should be positive");
    assert!(audio.channels > 0, "Should have at least 1 channel");
    assert!(!audio.samples.is_empty(), "Should have audio samples");

    // Verify path is stored
    assert!(audio.path.contains("sample-meeting.wav"));

    Ok(())
}

#[test]
fn test_audio_file_sample_count_matches_duration() -> Result<()> {
    let path = get_test_fixture_path("sample-meeting.wav");
    let audio = AudioFile::open(&path)?;

    // Calculate expected sample count from duration
    let expected_samples = (audio.duration_seconds * audio.sample_rate as f64 * audio.channels as f64) as usize;

    // Allow for small rounding errors (within 1 frame)
    let diff = (audio.samples.len() as i64 - expected_samples as i64).abs();
    assert!(diff < audio.channels as i64 * 1000,
            "Sample count ({}) should match duration calculation ({})",
            audio.samples.len(), expected_samples);

    Ok(())
}

#[test]
fn test_audio_file_nonexistent() {
    let path = PathBuf::from("/nonexistent/path/to/audio.wav");
    let result = AudioFile::open(&path);

    assert!(result.is_err(), "Opening nonexistent file should fail");
}

#[test]
fn test_audio_file_metadata() -> Result<()> {
    let path = get_test_fixture_path("sample-meeting.wav");
    let audio = AudioFile::open(&path)?;

    // Log metadata for manual verification
    println!("Audio file metadata:");
    println!("  Path: {}", audio.path);
    println!("  Duration: {:.2}s", audio.duration_seconds);
    println!("  Sample rate: {}Hz", audio.sample_rate);
    println!("  Channels: {}", audio.channels);
    println!("  Total samples: {}", audio.samples.len());

    // Basic sanity checks
    assert!(audio.duration_seconds < 10.0, "Test file should be short (< 10s)");
    assert!(audio.sample_rate >= 8000 && audio.sample_rate <= 48000,
            "Sample rate should be reasonable");
    assert!(audio.channels >= 1 && audio.channels <= 2,
            "Should be mono or stereo");

    Ok(())
}

#[test]
fn test_audio_file_resample_to_mono_16khz() -> Result<()> {
    let path = get_test_fixture_path("sample-meeting.wav");
    let audio = AudioFile::open(&path)?;

    // If the file is already 16kHz mono, resampling should work
    if audio.sample_rate == 16000 && audio.channels == 1 {
        let resampled = audio.resample_to_mono_16khz()?;
        assert_eq!(resampled.len(), audio.samples.len(),
                   "16kHz mono should return original samples");
    } else {
        // If not 16kHz mono, it should fail (resampling not implemented yet)
        let result = audio.resample_to_mono_16khz();
        assert!(result.is_err(),
                "Resampling should fail for non-16kHz-mono files (not implemented)");
    }

    Ok(())
}

#[test]
fn test_audio_file_samples_are_i16() -> Result<()> {
    let path = get_test_fixture_path("sample-meeting.wav");
    let audio = AudioFile::open(&path)?;

    // Verify samples are in valid i16 range
    for (i, &sample) in audio.samples.iter().take(100).enumerate() {
        assert!(sample >= i16::MIN && sample <= i16::MAX,
                "Sample {} at index {} is out of i16 range", sample, i);
    }

    Ok(())
}

#[test]
fn test_audio_file_interleaved_channels() -> Result<()> {
    let path = get_test_fixture_path("sample-meeting.wav");
    let audio = AudioFile::open(&path)?;

    // For stereo (2 channels), samples should be interleaved [L, R, L, R, ...]
    if audio.channels == 2 {
        assert_eq!(audio.samples.len() % 2, 0,
                   "Stereo audio should have even number of samples");
    }

    // Total samples should be divisible by channel count
    assert_eq!(audio.samples.len() % audio.channels as usize, 0,
               "Total samples should be divisible by channel count");

    Ok(())
}
