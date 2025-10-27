// Swift ScreenCaptureKit bridge for capturing system audio on macOS 12.3+
// This module provides FFI functions callable from Rust

import Foundation
import ScreenCaptureKit
import CoreAudio

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
    private var callback: (@convention(c) (UnsafePointer<Int16>?, Int32, UInt32, UInt16) -> Void)?
    private let sampleRate: UInt32
    private let channels: UInt16

    init(sampleRate: UInt32, channels: UInt16) {
        self.sampleRate = sampleRate
        self.channels = channels
        super.init()
    }

    func start(callback: @escaping @convention(c) (UnsafePointer<Int16>?, Int32, UInt32, UInt16) -> Void) async throws {
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
        config.sampleRate = Int(sampleRate)
        config.channelCount = Int(channels)

        // Create and start stream
        stream = SCStream(filter: filter, configuration: config, delegate: self)

        // Add audio output
        try stream?.addStreamOutput(self, type: .audio, sampleHandlerQueue: DispatchQueue.global(qos: .userInitiated))

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
        guard outputType == .audio else { return }
        guard let callback = self.callback else { return }

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

        // Cast to Int16 samples (assuming 16-bit PCM)
        let samples = data.withMemoryRebound(to: Int16.self, capacity: length / 2) { $0 }
        let sampleCount = Int32(length / 2)

        // Call Rust callback
        callback(samples, sampleCount, sampleRate, channels)
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
    callback: @escaping @convention(c) (UnsafePointer<Int16>?, Int32, UInt32, UInt16) -> Void
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
