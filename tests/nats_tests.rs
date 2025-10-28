use base64::Engine;
use loqa_meetings::nats::messages::{AudioFrameMessage, TranscriptMessage};

#[test]
fn test_audio_frame_serialization() {
    let msg = AudioFrameMessage {
        session_id: "test-meeting".to_string(),
        sequence: 0,
        pcm: base64::engine::general_purpose::STANDARD.encode(&[0u8; 100]),
        sample_rate: 16000,
        channels: 1,
        timestamp: "2025-10-27T14:30:00Z".to_string(),
        final_frame: false,
    };

    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("test-meeting"));
    assert!(json.contains("16000"));
    assert!(json.contains("\"final\":false"));
    assert!(json.contains("\"sequence\":0"));

    let deserialized: AudioFrameMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.session_id, "test-meeting");
    assert_eq!(deserialized.sample_rate, 16000);
    assert_eq!(deserialized.channels, 1);
    assert_eq!(deserialized.sequence, 0);
    assert!(!deserialized.final_frame);
}

#[test]
fn test_audio_frame_final_marker() {
    let msg = AudioFrameMessage {
        session_id: "test-meeting".to_string(),
        sequence: 10,
        pcm: String::new(), // Empty for final marker
        sample_rate: 16000,
        channels: 1,
        timestamp: "2025-10-27T14:30:00Z".to_string(),
        final_frame: true,
    };

    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"final\":true"));

    let deserialized: AudioFrameMessage = serde_json::from_str(&json).unwrap();
    assert!(deserialized.final_frame);
    assert!(deserialized.pcm.is_empty());
    assert_eq!(deserialized.sequence, 10);
}

#[test]
fn test_transcript_deserialization() {
    let json = r#"{
        "session_id": "test-meeting",
        "text": "Hello world",
        "partial": false,
        "timestamp": "2025-10-27T14:30:05Z",
        "confidence": 0.95
    }"#;

    let msg: TranscriptMessage = serde_json::from_str(json).unwrap();
    assert_eq!(msg.session_id, "test-meeting");
    assert_eq!(msg.text, "Hello world");
    assert!(!msg.partial);
    assert_eq!(msg.confidence, Some(0.95));
    assert_eq!(msg.timestamp, "2025-10-27T14:30:05Z");
}

#[test]
fn test_transcript_partial() {
    let json = r#"{
        "session_id": "test-meeting",
        "text": "This is a partial",
        "partial": true,
        "timestamp": "2025-10-27T14:30:05Z",
        "confidence": 0.87
    }"#;

    let msg: TranscriptMessage = serde_json::from_str(json).unwrap();
    assert!(msg.partial);
    assert_eq!(msg.text, "This is a partial");
    assert_eq!(msg.confidence, Some(0.87));
}

#[test]
fn test_transcript_no_confidence() {
    let json = r#"{
        "session_id": "test-meeting",
        "text": "No confidence score",
        "partial": false,
        "timestamp": "2025-10-27T14:30:05Z"
    }"#;

    let msg: TranscriptMessage = serde_json::from_str(json).unwrap();
    assert_eq!(msg.text, "No confidence score");
    assert_eq!(msg.confidence, None);
}

#[test]
fn test_pcm_encoding_roundtrip() {
    let original_samples: Vec<i16> = vec![100, -200, 300, -400];

    // Convert to bytes
    let pcm_bytes: Vec<u8> = original_samples.iter()
        .flat_map(|&s| s.to_le_bytes())
        .collect();

    // Encode to base64
    let encoded = base64::engine::general_purpose::STANDARD.encode(&pcm_bytes);

    // Create message
    let msg = AudioFrameMessage {
        session_id: "test".to_string(),
        sequence: 0,
        pcm: encoded,
        sample_rate: 16000,
        channels: 1,
        timestamp: "2025-10-27T14:30:00Z".to_string(),
        final_frame: false,
    };

    // Serialize and deserialize
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: AudioFrameMessage = serde_json::from_str(&json).unwrap();

    // Decode base64
    let decoded_bytes = base64::engine::general_purpose::STANDARD
        .decode(&deserialized.pcm)
        .unwrap();

    // Convert back to i16 samples
    let decoded_samples: Vec<i16> = decoded_bytes
        .chunks_exact(2)
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();

    assert_eq!(decoded_samples, original_samples);
}
