use anyhow::{Context, Result};
use std::fs::File;
use std::path::Path;
use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use tracing::info;

pub struct AudioFile {
    pub path: String,
    pub duration_seconds: f64,
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<i16>,
}

impl AudioFile {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        info!("Opening audio file: {}", path.display());

        // Open the file
        let file = File::open(path).context("Failed to open audio file")?;

        // Create a media source stream
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        // Create a hint to help the format registry guess the format
        let mut hint = Hint::new();
        if let Some(ext) = path.extension() {
            if let Some(ext_str) = ext.to_str() {
                hint.with_extension(ext_str);
            }
        }

        // Probe the media source for a format
        let probed = symphonia::default::get_probe()
            .format(
                &hint,
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .context("Failed to probe audio format")?;

        let mut format = probed.format;

        // Find the first audio track
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .context("No audio tracks found")?;

        let track_id = track.id;
        let sample_rate = track
            .codec_params
            .sample_rate
            .context("Sample rate not specified")?;

        // Create a decoder for the track
        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .context("Failed to create decoder")?;

        // Decode all packets and collect samples
        let mut samples = Vec::new();
        let mut channels: Option<u16> = track.codec_params.channels.map(|ch| ch.count() as u16);

        loop {
            // Get the next packet from the format reader
            let packet = match format.next_packet() {
                Ok(packet) => packet,
                Err(SymphoniaError::IoError(e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    break;
                }
                Err(SymphoniaError::ResetRequired) => {
                    // The decoder needs to be reset, but we'll just stop here
                    break;
                }
                Err(e) => return Err(e).context("Error reading packet"),
            };

            // Skip packets that don't belong to our track
            if packet.track_id() != track_id {
                continue;
            }

            // Decode the packet
            match decoder.decode(&packet) {
                Ok(decoded) => {
                    // Get channel count from first decoded buffer if not already known
                    if channels.is_none() {
                        channels = Some(decoded.spec().channels.count() as u16);
                    }
                    // Convert decoded audio to i16 samples
                    convert_audio_buffer_to_i16(&decoded, &mut samples);
                }
                Err(SymphoniaError::DecodeError(e)) => {
                    // Decode errors are not fatal
                    tracing::warn!("Decode error: {}", e);
                    continue;
                }
                Err(e) => return Err(e).context("Error decoding packet"),
            }
        }

        let channels = channels.context("Could not determine channel count from audio")?;
        let duration_seconds = samples.len() as f64 / (sample_rate as f64 * channels as f64);

        info!(
            "Audio file loaded: {:.1}s, {}Hz, {} channels, {} samples",
            duration_seconds,
            sample_rate,
            channels,
            samples.len()
        );

        Ok(Self {
            path: path.display().to_string(),
            duration_seconds,
            sample_rate,
            channels,
            samples,
        })
    }

    pub fn resample_to_mono_16khz(&self) -> Result<Vec<i16>> {
        // TODO: Implement resampling for Whisper (16kHz mono)
        // For Week 1, just return original samples if already 16kHz mono
        if self.sample_rate == 16000 && self.channels == 1 {
            Ok(self.samples.clone())
        } else {
            anyhow::bail!(
                "Resampling not implemented yet. Expected 16kHz mono, got {}Hz {}ch",
                self.sample_rate,
                self.channels
            )
        }
    }
}

/// Convert Symphonia's AudioBufferRef to a Vec<i16>
/// Interleaves all channels into a single stream
fn convert_audio_buffer_to_i16(buffer: &AudioBufferRef, output: &mut Vec<i16>) {
    let num_channels = buffer.spec().channels.count();
    let num_frames = buffer.frames();

    match buffer {
        AudioBufferRef::U8(buf) => {
            for frame in 0..num_frames {
                for ch in 0..num_channels {
                    let sample = buf.chan(ch)[frame];
                    output.push(((sample as i32 - 128) * 256) as i16);
                }
            }
        }
        AudioBufferRef::U16(buf) => {
            for frame in 0..num_frames {
                for ch in 0..num_channels {
                    let sample = buf.chan(ch)[frame];
                    output.push((sample as i32 - 32768) as i16);
                }
            }
        }
        AudioBufferRef::U24(buf) => {
            for frame in 0..num_frames {
                for ch in 0..num_channels {
                    let sample = buf.chan(ch)[frame];
                    output.push((sample.inner() >> 8) as i16);
                }
            }
        }
        AudioBufferRef::U32(buf) => {
            for frame in 0..num_frames {
                for ch in 0..num_channels {
                    let sample = buf.chan(ch)[frame];
                    output.push(((sample as i64 - 2147483648) / 65536) as i16);
                }
            }
        }
        AudioBufferRef::S8(buf) => {
            for frame in 0..num_frames {
                for ch in 0..num_channels {
                    let sample = buf.chan(ch)[frame];
                    output.push((sample as i16) * 256);
                }
            }
        }
        AudioBufferRef::S16(buf) => {
            for frame in 0..num_frames {
                for ch in 0..num_channels {
                    let sample = buf.chan(ch)[frame];
                    output.push(sample);
                }
            }
        }
        AudioBufferRef::S24(buf) => {
            for frame in 0..num_frames {
                for ch in 0..num_channels {
                    let sample = buf.chan(ch)[frame];
                    output.push((sample.inner() >> 8) as i16);
                }
            }
        }
        AudioBufferRef::S32(buf) => {
            for frame in 0..num_frames {
                for ch in 0..num_channels {
                    let sample = buf.chan(ch)[frame];
                    output.push((sample >> 16) as i16);
                }
            }
        }
        AudioBufferRef::F32(buf) => {
            for frame in 0..num_frames {
                for ch in 0..num_channels {
                    let sample = buf.chan(ch)[frame];
                    output.push((sample * 32767.0) as i16);
                }
            }
        }
        AudioBufferRef::F64(buf) => {
            for frame in 0..num_frames {
                for ch in 0..num_channels {
                    let sample = buf.chan(ch)[frame];
                    output.push((sample * 32767.0) as i16);
                }
            }
        }
    }
}
