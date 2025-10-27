# Testing Guide - Week 2 ScreenCaptureKit Implementation

This guide walks through testing the macOS system audio and microphone capture implementation.

## Prerequisites

- **macOS Version**: 13.0+ (Ventura, October 2022 or later)
  - **Microphone capture**: Requires macOS 15.0+ (Sequoia, October 2024)
- **Xcode Command Line Tools**: Installed (`xcode-select --install`)
- **Rust**: 1.70+ with Apple Silicon support

## Step 1: Grant Screen Recording Permission

ScreenCaptureKit requires Screen Recording permission to capture system audio.

### For Terminal/CLI Testing:

1. Open **System Settings** → **Privacy & Security** → **Screen Recording**
2. Enable **Terminal** (if running from terminal)
3. Or enable **Visual Studio Code** / **Your IDE** (if running from IDE terminal)

**Note**: You may need to restart your terminal/IDE after granting permission.

### Verify Permissions:

```bash
# This command should list your terminal or IDE
sqlite3 ~/Library/Application\ Support/com.apple.TCC/TCC.db \
  "SELECT client FROM access WHERE service='kTCCServiceScreenCapture';"
```

## Step 2: Build the Example

```bash
cd /Users/anna/code/loqalabs/loqa-meetings
cargo build --example record_chunks
```

Expected output: `Finished dev profile [unoptimized + debuginfo] target(s)`

## Step 3: Test Audio Capture

### Quick Test (10 seconds):

```bash
cargo run --example record_chunks -- --duration 10 --chunk-duration 30
```

This will:
- Record for 10 seconds
- Create chunks every 30 seconds (so just 1 chunk)
- Save to `~/.loqa/recordings/test-meeting/`

### Expected Output:

```
INFO loqa_meetings: Loqa Meetings - Chunked Recording Example
INFO loqa_meetings: Recording for 10 seconds
INFO loqa_meetings: Creating macOS ScreenCaptureKit backend...
INFO loqa_meetings::audio::macos: macOS backend initialized (16000Hz, 1 channels)
INFO loqa_meetings: Backend created: macOS ScreenCaptureKit
INFO loqa_meetings::audio::chunk: Chunked recorder initialized: test-meeting (chunks: 30s each)
INFO loqa_meetings::screencapture: Starting ScreenCaptureKit capture (16000Hz, 1 channels)
INFO loqa_meetings: Recording started! Press Ctrl+C to stop early, or wait 10 seconds
INFO loqa_meetings::audio::chunk: Starting chunked recording
INFO loqa_meetings::audio::chunk: Chunk 0 complete: 0.0s - 10.0s (160000 samples)
INFO loqa_meetings: Recording complete!
INFO loqa_meetings: Saved 1 chunks:
  - Chunk 0: ~/.loqa/recordings/test-meeting/test-meeting-chunk-000.wav (0.0s - 10.0s, 160000 samples)
```

### Full Test (5-minute chunks):

```bash
cargo run --example record_chunks -- --duration 60 --chunk-duration 30
```

This will create 2 chunks (each 30 seconds).

## Step 4: Verify WAV Files

Check that WAV files were created:

```bash
ls -lh ~/.loqa/recordings/test-meeting/
```

Expected output:
```
test-meeting-chunk-000.wav
test-meeting-chunk-001.wav (if duration > chunk-duration)
```

### Inspect WAV Metadata:

```bash
# Install sox if needed: brew install sox
soxi ~/.loqa/recordings/test-meeting/test-meeting-chunk-000.wav
```

Expected metadata:
- **Sample Rate**: 16000 Hz
- **Channels**: 1 (mono)
- **Precision**: 16-bit
- **Duration**: ~10-30 seconds (depending on test)

### Play Audio:

```bash
afplay ~/.loqa/recordings/test-meeting/test-meeting-chunk-000.wav
```

You should hear the system audio that was playing during the test.

## Step 5: Test Chunking Behavior

Test that chunks are split correctly at boundaries:

```bash
# Record for 90 seconds with 30-second chunks (should create 3 chunks)
cargo run --example record_chunks -- --duration 90 --chunk-duration 30
```

Expected chunks:
- `test-meeting-chunk-000.wav` (0s - 30s)
- `test-meeting-chunk-001.wav` (30s - 60s)
- `test-meeting-chunk-002.wav` (60s - 90s)

## Troubleshooting

### Error: "ScreenCaptureKit is not available"

**Cause**: Running on macOS < 13.0

**Solution**: Upgrade to macOS 13.0 (Ventura) or later

### Error: "Failed to start ScreenCaptureKit capture"

**Cause**: Missing Screen Recording permission

**Solution**:
1. Open System Settings → Privacy & Security → Screen Recording
2. Enable your terminal/IDE
3. Restart terminal/IDE

### Error: "No audio in WAV files"

**Possible causes**:
1. No system audio was playing during capture
2. Permission not granted correctly
3. Audio routing issue

**Solution**:
1. Play music/video during test
2. Re-grant Screen Recording permission
3. Check System Settings → Sound → Output

### Swift Compilation Errors

**Cause**: Xcode Command Line Tools not installed or outdated

**Solution**:
```bash
xcode-select --install
# Or update: sudo rm -rf /Library/Developer/CommandLineTools && xcode-select --install
```

## Success Criteria

✅ Example builds without errors
✅ Screen Recording permission granted
✅ WAV files created in output directory
✅ Audio quality is clear (16kHz mono)
✅ Chunks split at correct boundaries
✅ Metadata shows correct sample rate/channels

## Next Steps

Once testing is complete:
1. Clean up test files: `rm -rf ~/.loqa/recordings/test-meeting/`
2. Move to Week 3: NATS integration with loqa-core STT service

## Microphone Capture (macOS 15.0+)

On macOS 15.0+ (Sequoia), ScreenCaptureKit can also capture microphone input:

**What's Captured:**
- ✅ System audio output (apps, videos, Zoom participants)
- ✅ Microphone input (your voice) - **macOS 15.0+ only**

**Permissions:**
- **Screen Recording**: Required for system audio
- **Microphone**: Automatically requested on first run (macOS 15.0+)

**Current Behavior:**
- Both streams are captured but **not yet mixed**
- Audio files will contain system audio + microphone sequentially
- Future enhancement: Real-time mixing for synchronized playback

**Testing Microphone Capture:**
```bash
# Run a test and speak into your microphone while playing system audio
cargo run --example record_chunks -- --duration 10

# File size should be ~4x larger than system audio alone
ls -lh ~/.loqa/recordings/test-meeting/
```

## Notes

- **Performance**: ScreenCaptureKit has low CPU overhead (~1-2%)
- **Latency**: 100ms buffer = minimal latency
- **Quality**: 16kHz mono is optimal for Whisper STT
- **Storage**: ~2MB per minute of system audio (16kHz mono WAV)
- **Storage with mic**: ~8MB per minute when capturing both streams (pre-mixing)
