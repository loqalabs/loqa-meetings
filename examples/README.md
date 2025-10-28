# Loqa Meetings Examples

This directory contains example programs demonstrating the capabilities of the Loqa Meetings audio capture and transcription system.

## Prerequisites

### macOS Permissions (Required)
All examples require macOS system permissions:

1. **Screen Recording** (for system audio capture)
   - System Settings → Privacy & Security → Screen Recording
   - Add Terminal, VS Code, or your IDE

2. **Microphone** (for microphone capture)
   - System Settings → Privacy & Security → Microphone
   - Add Terminal, VS Code, or your IDE

### External Services (For Some Examples)
Some examples require external services to be running:

- **NATS Server** (for transcription examples)
  ```bash
  docker run -p 4222:4222 nats
  ```

- **loqa-core STT Service** (for transcription examples)
  ```bash
  cd ../loqa-core
  cargo run
  ```

## Examples

### 1. `multichannel_test.rs` - Test Multichannel Audio Capture

**Purpose**: Tests the core audio capture system with multichannel mixing (system audio + microphone).

**What it does**:
- Captures system audio (LEFT channel) + microphone (RIGHT channel) for 15 seconds
- Uses Swift-based AVAudioEngine with ring buffers (handles silent sources correctly)
- Saves stereo WAV file to `~/.loqa/recordings/multichannel-test.wav`
- Reports capture statistics and audio quality

**Requirements**:
- macOS 15.0+ (for microphone capture via ScreenCaptureKit)
- Screen Recording + Microphone permissions

**Usage**:
```bash
cargo run --example multichannel_test
```

**Expected output**:
```
=== Multichannel Audio Capture Test ===

✅ ScreenCaptureKit available
📁 Output file: ~/.loqa/recordings/multichannel-test.wav

🎙️  Starting multichannel audio capture...
   - Recording for 15 seconds
   - Format: 48kHz stereo
   - System audio → LEFT channel
   - Microphone → RIGHT channel

💡 TIP: Play some audio AND speak to test both channels!
```

**Test scenarios**:
- System audio only → Left channel works, right channel silent
- Microphone only → Right channel works, left channel silent
- Both sources → Both channels work simultaneously

---

### 2. `record_chunks.rs` - Record in Time-Based Chunks

**Purpose**: Demonstrates chunked recording (5-minute segments) for long-form meetings.

**What it does**:
- Captures system audio continuously
- Splits recording into chunks (configurable duration, default 5 minutes)
- Saves each chunk as a separate WAV file
- Records metadata for each chunk (timestamps, sample count, etc.)

**Requirements**:
- macOS 13.0+ (Ventura)
- Screen Recording permission

**Usage**:
```bash
# Record for 30 seconds (creates 1-2 chunks depending on chunk size)
cargo run --example record_chunks -- --duration 30

# Default: records until stopped with Ctrl+C
cargo run --example record_chunks
```

**Expected output**:
```
Starting audio capture in 5-minute chunks...
Chunk 0: /Users/anna/.loqa/recordings/test-meeting/chunk-0001730123456-5m.wav
  - Samples: 14400000 (5 minutes)
  - Size: 28.8 MB
```

---

### 3. `nats_test.rs` - Test NATS Integration

**Purpose**: Tests NATS message bus integration for audio streaming and transcript delivery.

**What it does**:
- Connects to NATS server
- Publishes pre-recorded audio file to NATS
- Subscribes to transcripts from loqa-core
- Displays received transcripts with confidence scores

**Requirements**:
- NATS server running on `localhost:4222`
- loqa-core STT service running

**Usage**:
```bash
# Start NATS (in separate terminal)
docker run -p 4222:4222 nats

# Start loqa-core (in separate terminal)
cd ../loqa-core && cargo run

# Run test
cargo run --example nats_test
```

**Expected output**:
```
🧪 Testing NATS integration
✅ Connected to NATS
✅ Subscribed to transcripts
📤 Published audio file
📝 [FINAL] #1: "hello world" (confidence: 95.30%)
✅ Received 1 transcripts total
```

---

### 4. `live_transcription.rs` - End-to-End Live Transcription

**Purpose**: Demonstrates the complete Week 3 pipeline: capture → NATS → transcription → display.

**What it does**:
- Captures system audio + microphone in real-time (15 seconds)
- Downsamples from 48kHz stereo to 16kHz stereo for Whisper
- Publishes audio frames to NATS
- Receives and displays transcripts as they arrive
- Shows both PARTIAL and FINAL transcripts

**Requirements**:
- macOS 15.0+ (for microphone capture)
- Screen Recording + Microphone permissions
- NATS server running
- loqa-core STT service running

**Usage**:
```bash
# Start NATS (in separate terminal)
docker run -p 4222:4222 nats

# Start loqa-core (in separate terminal)
cd ../loqa-core && cargo run

# Run live transcription
cargo run --example live_transcription
```

**Expected output**:
```
🎙️  Starting live recording with real-time transcription
✅ Connected to NATS (meeting: live-test-1730123456)
✅ Subscribed to transcripts
✅ Audio backend ready: ScreenCaptureKit (48kHz stereo → 16kHz)
   System audio → LEFT channel
   Microphone → RIGHT channel

🎤 Starting audio capture for 15 seconds...
💬 Speak into your microphone AND/OR play system audio!

📤 Published frame 0 (0.0s elapsed)
📝 [PARTIAL] #1: "hello" (confidence: 85.20%)
📝 [FINAL  ] #2: "hello world" (confidence: 95.30%)
📤 Published frame 10 (2.1s elapsed)
...
✅ Received 5 transcripts total
🏁 Live recording test complete!
```

---

## Troubleshooting

### "ScreenCaptureKit not available"
- **Solution**: Requires macOS 13.0+ (Ventura or later)

### "No frames captured" or "Buffer full" errors
- **Solution**: Check that macOS permissions are granted for Screen Recording and Microphone
- Go to System Settings → Privacy & Security and verify permissions

### "Connection refused" (NATS examples)
- **Solution**: Ensure NATS server is running: `docker run -p 4222:4222 nats`

### "No transcripts received" (transcription examples)
- **Solution**: Check that loqa-core is running: `cd ../loqa-core && cargo run`
- Verify NATS server is accessible

### Audio playback is choppy or wrong speed
- **Issue**: This indicates a problem with the audio mixing pipeline
- **Solution**: This should not happen with the current Swift-based implementation
- If you encounter this, please report it as a bug

## Architecture Notes

### Multichannel Audio (Week 2 Fix)

The system uses a **multichannel approach** to handle silent sources correctly:

- **Swift-side**: AVAudioSourceNode with ring buffers
  - System audio → mono samples written to ring buffer
  - Microphone → mono samples written to ring buffer
  - Pull-based rendering (always produces frames)
  - Zero-fills when a source is silent (prevents starvation)

- **Stereo output**: System audio (LEFT) + Microphone (RIGHT)
  - Enables downstream diarization and speaker separation
  - Works correctly when either or both sources are silent
  - No complex Rust-side synchronization needed

### Why This Approach?

ScreenCaptureKit only sends audio buffers when a source is **actively producing sound**. Silent sources don't send buffers, which caused the old Rust mixer to wait indefinitely. The Swift AVAudioSourceNode approach solves this by:

1. **Decoupling** ScreenCaptureKit's event-driven push model
2. **Always rendering** frames via pull-based model
3. **Zero-filling** when sources are silent

This is the recommended approach from the macOS audio engineering community.

## Development

### Adding New Examples

When creating new examples:

1. Add a descriptive comment block at the top
2. Document prerequisites and requirements
3. Include usage instructions
4. Add appropriate logging with `tracing::info!`
5. Update this README with the new example

### Testing

Run all examples to verify nothing breaks:

```bash
cargo build --examples
cargo run --example multichannel_test
# ... test others as needed
```

## Related Documentation

- [Audio Mixing Documentation](../docs/audio-mixing.md)
- [NATS Protocol](../docs/nats-protocol.md)
- [ScreenCaptureKit Bridge](../src/screencapture/bridge.swift)
- [Main README](../README.md)
