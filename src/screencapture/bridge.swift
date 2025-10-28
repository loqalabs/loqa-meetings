// Swift ScreenCaptureKit bridge for capturing system audio on macOS 12.3+
// This module provides FFI functions callable from Rust

import Foundation
import ScreenCaptureKit
import CoreAudio
import AVFoundation

// MARK: - Ring Buffer for audio samples

/// Simple thread-safe ring buffer for audio samples
class RingBuffer {
    private var buffer: [Float]
    private var readIndex: Int = 0
    private var writeIndex: Int = 0
    private var availableFrames: Int = 0
    private let capacity: Int
    private let lock = NSLock()

    init(capacity: Int) {
        self.capacity = capacity
        self.buffer = [Float](repeating: 0, count: capacity)
    }

    /// Write samples to the ring buffer
    /// Returns the number of frames actually written
    func write(_ samples: [Float]) -> Int {
        lock.lock()
        defer { lock.unlock() }

        let framesToWrite = min(samples.count, capacity - availableFrames)
        guard framesToWrite > 0 else { return 0 }

        for i in 0..<framesToWrite {
            buffer[writeIndex] = samples[i]
            writeIndex = (writeIndex + 1) % capacity
        }

        availableFrames += framesToWrite
        return framesToWrite
    }

    /// Read samples from the ring buffer
    /// If fewer than requested frames are available, returns what's available
    /// Remaining samples in output array are left as-is (caller should zero-fill first)
    func read(count: Int) -> [Float] {
        lock.lock()
        defer { lock.unlock() }

        let framesToRead = min(count, availableFrames)
        var result = [Float](repeating: 0, count: framesToRead)

        for i in 0..<framesToRead {
            result[i] = buffer[readIndex]
            readIndex = (readIndex + 1) % capacity
        }

        availableFrames -= framesToRead
        return result
    }

    /// Get the number of frames currently available
    var available: Int {
        lock.lock()
        defer { lock.unlock() }
        return availableFrames
    }
}

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

    // AVAudioEngine for mixing
    private let engine = AVAudioEngine()
    private var sourceNode: AVAudioSourceNode?

    // Ring buffers for each source (sized for ~2 seconds at 48kHz)
    private let systemRingBuffer = RingBuffer(capacity: 96000)
    private let micRingBuffer = RingBuffer(capacity: 96000)

    // Mix format: 48kHz stereo
    private let mixFormat = AVAudioFormat(standardFormatWithSampleRate: 48000, channels: 2)!

    init(sampleRate: UInt32, channels: UInt16) {
        self.sampleRate = sampleRate
        self.channels = channels
        super.init()
    }


    func start(callback: @escaping @convention(c) (UnsafePointer<Int16>?, Int32, UInt32, UInt16, UInt8) -> Void) async throws {
        self.callback = callback

        // Create AVAudioSourceNode that pulls from ring buffers
        let systemRB = self.systemRingBuffer
        let micRB = self.micRingBuffer
        let cb = callback

        sourceNode = AVAudioSourceNode(format: mixFormat) { _, _, frameCount, audioBufferList in
            let ablPointer = UnsafeMutableAudioBufferListPointer(audioBufferList)

            guard let buffer = ablPointer.first else { return noErr }
            guard let data = buffer.mData else { return noErr }

            // Zero-fill output buffer
            memset(data, 0, Int(buffer.mDataByteSize))

            // Pull frames from both ring buffers
            let sysFrames = systemRB.read(count: Int(frameCount))
            let micFrames = micRB.read(count: Int(frameCount))

            // Mix into stereo: system→left, mic→right
            // Apply 2x gain to boost volume
            let gain: Float = 2.0
            let floatPtr = data.assumingMemoryBound(to: Float.self)
            for i in 0..<Int(frameCount) {
                let leftSample = i < sysFrames.count ? sysFrames[i] * gain : 0.0
                let rightSample = i < micFrames.count ? micFrames[i] * gain : 0.0
                floatPtr[i * 2] = leftSample      // Left channel
                floatPtr[i * 2 + 1] = rightSample // Right channel
            }

            // Convert Float32 stereo to Int16 and call Rust callback
            var int16Samples = [Int16](repeating: 0, count: Int(frameCount) * 2)
            for i in 0..<(Int(frameCount) * 2) {
                let clamped = max(-1.0, min(1.0, floatPtr[i]))
                int16Samples[i] = Int16(clamped * 32767.0)
            }

            int16Samples.withUnsafeBufferPointer { bufferPtr in
                cb(bufferPtr.baseAddress, Int32(int16Samples.count), 48000, 2, 0)
            }

            return noErr
        }

        // Attach and connect source node to engine
        engine.attach(sourceNode!)
        engine.connect(sourceNode!, to: engine.mainMixerNode, format: mixFormat)

        // Mute the output to prevent speaker feedback (volume = 0)
        engine.mainMixerNode.outputVolume = 0.0

        // Start engine (output muted - no speaker feedback)
        try engine.start()
        NSLog("AVAudioEngine started for mixing (output muted)")

        // Start ScreenCaptureKit capture (will push to ring buffers)
        let content = try await SCShareableContent.excludingDesktopWindows(
            false,
            onScreenWindowsOnly: true
        )

        guard let display = content.displays.first else {
            throw NSError(domain: "ScreenCapture", code: 1, userInfo: [
                NSLocalizedDescriptionKey: "No displays available"
            ])
        }

        let filter = SCContentFilter(display: display, excludingWindows: [])
        let config = SCStreamConfiguration()
        config.capturesAudio = true
        config.excludesCurrentProcessAudio = true

        if #available(macOS 15.0, *) {
            config.captureMicrophone = true
            config.microphoneCaptureDeviceID = nil
            NSLog("ScreenCaptureKit: Microphone capture enabled (macOS 15.0+)")
        } else {
            NSLog("ScreenCaptureKit: Microphone capture not available (requires macOS 15.0+)")
        }

        stream = SCStream(filter: filter, configuration: config, delegate: self)
        try stream?.addStreamOutput(self, type: .audio, sampleHandlerQueue: DispatchQueue.global(qos: .userInitiated))

        if #available(macOS 15.0, *) {
            do {
                try stream?.addStreamOutput(self, type: .microphone, sampleHandlerQueue: DispatchQueue.global(qos: .userInitiated))
                NSLog("ScreenCaptureKit: Microphone stream output added")
            } catch {
                NSLog("ScreenCaptureKit: Failed to add microphone output: \(error)")
            }
        }

        try await stream?.startCapture()
    }

    func stop() async throws {
        // Stop ScreenCaptureKit
        try await stream?.stopCapture()
        stream = nil

        // Stop AVAudioEngine
        engine.stop()
        if let node = sourceNode {
            engine.detach(node)
        }
        sourceNode = nil

        callback = nil
        NSLog("AudioCaptureSession stopped")
    }

    // MARK: - SCStreamOutput (audio callback)

    func stream(
        _ stream: SCStream,
        didOutputSampleBuffer sampleBuffer: CMSampleBuffer,
        of outputType: SCStreamOutputType
    ) {
        // Determine which ring buffer to write to
        var isSystemAudio = outputType == .audio
        var isMicrophone = false
        if #available(macOS 15.0, *) {
            if outputType == .microphone {
                isMicrophone = true
                isSystemAudio = false
            }
        }
        guard isSystemAudio || isMicrophone else { return }

        // Extract audio data from CMSampleBuffer
        guard let formatDescription = CMSampleBufferGetFormatDescription(sampleBuffer),
              let asbd = CMAudioFormatDescriptionGetStreamBasicDescription(formatDescription)?.pointee,
              let blockBuffer = CMSampleBufferGetDataBuffer(sampleBuffer) else {
            return
        }

        let actualChannels = UInt16(asbd.mChannelsPerFrame)
        let isNonInterleaved = (asbd.mFormatFlags & kAudioFormatFlagIsNonInterleaved) != 0

        var length: Int = 0
        var dataPointer: UnsafeMutablePointer<Int8>?

        let status = CMBlockBufferGetDataPointer(
            blockBuffer,
            atOffset: 0,
            lengthAtOffsetOut: nil,
            totalLengthOut: &length,
            dataPointerOut: &dataPointer
        )

        guard status == kCMBlockBufferNoErr, let data = dataPointer else { return }

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
                // Non-interleaved: [L0,L1,...,Ln, R0,R1,...,Rn]
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
                // Interleaved: [L0,R0,L1,R1,...]
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

        // Write to appropriate ring buffer
        let written: Int
        if isSystemAudio {
            written = systemRingBuffer.write(monoFloats)
        } else {
            written = micRingBuffer.write(monoFloats)
        }

        if written < monoFloats.count {
            NSLog("ScreenCaptureKit: Ring buffer full, dropped \(monoFloats.count - written) frames from \(isMicrophone ? "mic" : "system")")
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
