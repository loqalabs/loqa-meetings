# Audio Mixing

The audio mixer combines multiple audio streams (system audio, microphone, etc.) into a single output stream.

## Design

The mixer uses a **flexible source filtering** approach:
- Configure which sources to include using a `HashSet<AudioStreamSource>`
- Empty set = include all sources
- Non-empty set = only include specified sources
- Extensible for future audio sources

## Usage Examples

### Example 1: Record both system audio and microphone (default)

```rust
use loqa_meetings::{AudioMixer, MixerConfig};
use std::collections::HashSet;

let config = MixerConfig::default(); // Includes System + Microphone
let mixer = AudioMixer::new(config);
```

### Example 2: System audio only

```rust
use loqa_meetings::{AudioMixer, AudioStreamSource, MixerConfig};
use std::collections::HashSet;

let mut enabled_sources = HashSet::new();
enabled_sources.insert(AudioStreamSource::System);

let config = MixerConfig {
    sample_rate: 16000,
    channels: 1,
    max_buffer_delay_ms: 200,
    enabled_sources,
};

let mixer = AudioMixer::new(config);
```

### Example 3: Microphone only

```rust
use loqa_meetings::{AudioMixer, AudioStreamSource, MixerConfig};
use std::collections::HashSet;

let mut enabled_sources = HashSet::new();
enabled_sources.insert(AudioStreamSource::Microphone);

let config = MixerConfig {
    sample_rate: 16000,
    channels: 1,
    max_buffer_delay_ms: 200,
    enabled_sources,
};

let mixer = AudioMixer::new(config);
```

### Example 4: Future extensibility - Multiple microphones

```rust
// When additional sources are added in the future:
use loqa_meetings::{AudioMixer, AudioStreamSource, MixerConfig};
use std::collections::HashSet;

let mut enabled_sources = HashSet::new();
enabled_sources.insert(AudioStreamSource::System);
enabled_sources.insert(AudioStreamSource::Microphone);
// Future: enabled_sources.insert(AudioStreamSource::UsbMicrophone);
// Future: enabled_sources.insert(AudioStreamSource::AirPods);

let config = MixerConfig {
    sample_rate: 16000,
    channels: 1,
    max_buffer_delay_ms: 200,
    enabled_sources,
};

let mixer = AudioMixer::new(config);
```

## How Mixing Works

1. **Source Tagging**: Each audio frame is tagged with its source (System or Microphone)
2. **Buffering**: Frames are buffered per-source in separate queues
3. **Filtering**: Only frames from enabled sources are processed
4. **Time Alignment**: Frames are aligned by timestamp
5. **Sample Mixing**: Audio samples are added together with automatic clipping to prevent overflow
6. **Output**: Single mixed stream with all enabled sources combined

## Platform Support

- **macOS 15.0+**: System audio + microphone via ScreenCaptureKit
- **macOS 13.0-14.x**: System audio only
- **Other platforms**: Not yet implemented

## Performance Considerations

- **Buffer Delay**: Default 200ms max buffering prevents unbounded memory growth
- **Clipping**: Automatic i16 overflow protection during mixing
- **Zero-copy**: Frames passed via channels minimize allocations
