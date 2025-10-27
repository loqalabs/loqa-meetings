// Swift ScreenCaptureKit bridge for capturing system audio on macOS 12.3+
// This module provides FFI functions callable from Rust

import Foundation
import ScreenCaptureKit
import CoreAudio
import AVFoundation

// MARK: - C-compatible types for FFI

@_cdecl("loqa_screencapture_is_available")
public func isAvailable() -> Bool {
    if #available(macOS 13.0, *) {
        return true
    }
    return false
}

// MARK: - Audio capture session

@available(macOS 13.0, *)
class AudioCaptureSession: NSObject, SCStreamDelegate, SCStreamOutput {
    private var stream: SCStream?
    private var callback: (@convention(c) (UnsafePointer<Int16>?, Int32, UInt32, UInt16, UInt8) -> Void)?
    private let sampleRate: UInt32
    private let channels: UInt16

    init(sampleRate: UInt32, channels: UInt16) {
        self.sampleRate = sampleRate
        self.channels = channels
        super.init()
    }


    func start(callback: @escaping @convention(c) (UnsafePointer<Int16>?, Int32, UInt32, UInt16, UInt8) -> Void) async throws {
        self.callback = callback

        // Get available content (windows, displays)
        let content = try await SCShareableContent.excludingDesktopWindows(
            false,
            onScreenWindowsOnly: true
        )

        // Use the main display for system audio capture
        guard let display = content.displays.first else {
            throw NSError(domain: "ScreenCapture", code: 1, userInfo: [
                NSLocalizedDescriptionKey: "No displays available"
            ])
        }

        // Create filter to capture display (system audio)
        let filter = SCContentFilter(display: display, excludingWindows: [])

        // Configure stream for audio only
        let config = SCStreamConfiguration()
        config.capturesAudio = true
        config.excludesCurrentProcessAudio = true  // Don't capture our own audio
        // Don't set sampleRate/channelCount - let ScreenCaptureKit use native rate
        // We'll resample in the callback to get consistent output

        // Enable microphone capture on macOS 15.0+
        if #available(macOS 15.0, *) {
            config.captureMicrophone = true
            config.microphoneCaptureDeviceID = nil  // Use default microphone
            NSLog("ScreenCaptureKit: Microphone capture enabled (macOS 15.0+)")
        } else {
            NSLog("ScreenCaptureKit: Microphone capture not available (requires macOS 15.0+)")
        }

        // Create and start stream
        stream = SCStream(filter: filter, configuration: config, delegate: self)

        // Add audio output
        try stream?.addStreamOutput(self, type: .audio, sampleHandlerQueue: DispatchQueue.global(qos: .userInitiated))

        // Add microphone output on macOS 15.0+
        if #available(macOS 15.0, *) {
            do {
                try stream?.addStreamOutput(self, type: .microphone, sampleHandlerQueue: DispatchQueue.global(qos: .userInitiated))
                NSLog("ScreenCaptureKit: Microphone stream output added")
            } catch {
                NSLog("ScreenCaptureKit: Failed to add microphone output: \(error)")
            }
        }

        // Start capture
        try await stream?.startCapture()
    }

    func stop() async throws {
        try await stream?.stopCapture()
        stream = nil
        callback = nil
    }

    // MARK: - SCStreamOutput (audio callback)

    func stream(
        _ stream: SCStream,
        didOutputSampleBuffer sampleBuffer: CMSampleBuffer,
        of outputType: SCStreamOutputType
    ) {
        // Handle both .audio (system) and .microphone streams
        var shouldProcess = outputType == .audio
        var streamType: UInt8 = 0  // 0 = system, 1 = microphone
        if #available(macOS 15.0, *) {
            if outputType == .microphone {
                shouldProcess = true
                streamType = 1
            }
        }
        guard shouldProcess else { return }
        guard let callback = self.callback else { return }

        // Check audio format to get ACTUAL sample rate
        guard let formatDescription = CMSampleBufferGetFormatDescription(sampleBuffer) else {
            NSLog("ScreenCaptureKit: No format description")
            return
        }

        guard let asbd = CMAudioFormatDescriptionGetStreamBasicDescription(formatDescription)?.pointee else {
            NSLog("ScreenCaptureKit: No ASBD")
            return
        }

        // Use the ACTUAL sample rate from the buffer, not what we requested
        let actualSampleRate = UInt32(asbd.mSampleRate)
        let actualChannels = UInt16(asbd.mChannelsPerFrame)

        // Check if audio is interleaved or non-interleaved
        let isNonInterleaved = (asbd.mFormatFlags & kAudioFormatFlagIsNonInterleaved) != 0

        // Extract audio data from CMSampleBuffer
        guard let blockBuffer = CMSampleBufferGetDataBuffer(sampleBuffer) else {
            return
        }

        var length: Int = 0
        var dataPointer: UnsafeMutablePointer<Int8>?

        let status = CMBlockBufferGetDataPointer(
            blockBuffer,
            atOffset: 0,
            lengthAtOffsetOut: nil,
            totalLengthOut: &length,
            dataPointerOut: &dataPointer
        )

        guard status == kCMBlockBufferNoErr,
              let data = dataPointer else {
            return
        }

        // ScreenCaptureKit gives us Float32 PCM
        let floatSamples = data.withMemoryRebound(to: Float32.self, capacity: length / 4) {
            Array(UnsafeBufferPointer(start: $0, count: length / 4))
        }

        // Convert multi-channel to mono if needed
        let monoFloats: [Float32]
        if actualChannels > 1 {
            let frameCount = floatSamples.count / Int(actualChannels)
            var mono = [Float32]()
            mono.reserveCapacity(frameCount)

            if isNonInterleaved {
                // Non-interleaved: [L0,L1,L2,...,Ln, R0,R1,R2,...,Rn]
                // First half is channel 0, second half is channel 1, etc.
                for frame in 0..<frameCount {
                    var sum: Float32 = 0
                    for ch in 0..<Int(actualChannels) {
                        let idx = ch * frameCount + frame
                        if idx < floatSamples.count {
                            sum += floatSamples[idx]
                        }
                    }
                    mono.append(sum / Float32(actualChannels))
                }
            } else {
                // Interleaved: [L0,R0,L1,R1,L2,R2,...]
                for frame in 0..<frameCount {
                    var sum: Float32 = 0
                    for ch in 0..<Int(actualChannels) {
                        sum += floatSamples[frame * Int(actualChannels) + ch]
                    }
                    mono.append(sum / Float32(actualChannels))
                }
            }
            monoFloats = mono
        } else {
            monoFloats = floatSamples
        }

        // Convert Float32 (-1.0 to 1.0) to Int16 (-32768 to 32767)
        // Keep native sample rate - no resampling (will handle later in Rust if needed)
        var int16Samples = [Int16](repeating: 0, count: monoFloats.count)
        for i in 0..<monoFloats.count {
            let clamped = max(-1.0, min(1.0, monoFloats[i]))
            int16Samples[i] = Int16(clamped * 32767.0)
        }

        // Call Rust callback with converted audio at native sample rate
        int16Samples.withUnsafeBufferPointer { buffer in
            callback(buffer.baseAddress, Int32(buffer.count), actualSampleRate, 1, streamType)
        }
    }

    // MARK: - SCStreamDelegate

    func stream(_ stream: SCStream, didStopWithError error: Error) {
        NSLog("ScreenCaptureKit stream stopped with error: \(error)")
    }
}

// MARK: - Global session management (for FFI)

@available(macOS 13.0, *)
private var globalSession: AudioCaptureSession?

@_cdecl("loqa_screencapture_start")
public func startCapture(
    sampleRate: UInt32,
    channels: UInt16,
    callback: @escaping @convention(c) (UnsafePointer<Int16>?, Int32, UInt32, UInt16, UInt8) -> Void
) -> Int32 {
    guard #available(macOS 13.0, *) else {
        return -1  // Not available
    }

    do {
        let session = AudioCaptureSession(sampleRate: sampleRate, channels: channels)

        // Start capture (async, but we'll block here for FFI simplicity)
        let group = DispatchGroup()
        var error: Error?

        group.enter()
        Task {
            do {
                try await session.start(callback: callback)
            } catch let e {
                error = e
            }
            group.leave()
        }
        group.wait()

        if let error = error {
            NSLog("Failed to start capture: \(error)")
            return -2  // Start failed
        }

        globalSession = session
        return 0  // Success

    } catch {
        NSLog("Failed to create capture session: \(error)")
        return -2  // Start failed
    }
}

@_cdecl("loqa_screencapture_stop")
public func stopCapture() -> Int32 {
    guard #available(macOS 13.0, *) else {
        return -1  // Not available
    }

    guard let session = globalSession else {
        return -3  // Not started
    }

    let group = DispatchGroup()
    var error: Error?

    group.enter()
    Task {
        do {
            try await session.stop()
        } catch let e {
            error = e
        }
        group.leave()
    }
    group.wait()

    globalSession = nil

    return error == nil ? 0 : -4  // Success or stop failed
}
