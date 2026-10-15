// Appended to the exact production source by check-native-screen-recording.py.
// This fixture never calls choose(), SCStream, a permission API or screen capture.
@available(macOS 14.0, *)
private enum NativeCaptureFixture {
    static func expect(_ value: @autoclosure () -> Bool, _ message: String) {
        guard value() else { fatalError(message) }
    }
    static func wait(_ message: String, seconds: Double = 20, _ ready: () -> Bool) {
        let until = Date(timeIntervalSinceNow: seconds)
        while !ready() && Date() < until { RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.01)) }
        expect(ready(), message)
    }
    static func api(_ value: [String: Any]) -> [String: Any] {
        let pointer = json(value).withCString { command($0) }
        guard let pointer else { fatalError("Fixture API allocation failed") }
        defer { freeJSON(pointer) }
        guard let data = String(cString: pointer).data(using: .utf8),
              let result = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else {
            fatalError("Fixture API returned malformed JSON")
        }
        return result
    }
    static func directory(_ root: URL, _ name: String) -> URL {
        let result = root.appendingPathComponent(name, isDirectory: true)
        try! FileManager.default.createDirectory(at: result, withIntermediateDirectories: false,
            attributes: [.posixPermissions: 0o700])
        return result
    }
    static func sample(seconds: Double, blue: UInt8, green: UInt8, red: UInt8) -> CMSampleBuffer {
        var pixel: CVPixelBuffer?
        expect(CVPixelBufferCreate(kCFAllocatorDefault, 640, 360, kCVPixelFormatType_32BGRA,
            [kCVPixelBufferIOSurfacePropertiesKey: [:]] as CFDictionary, &pixel) == kCVReturnSuccess,
            "Fixture pixel allocation failed")
        let buffer = pixel!
        CVPixelBufferLockBaseAddress(buffer, [])
        let base = CVPixelBufferGetBaseAddress(buffer)!.assumingMemoryBound(to: UInt8.self)
        let stride = CVPixelBufferGetBytesPerRow(buffer)
        for y in 0..<360 {
            for x in 0..<640 {
                let offset = y * stride + x * 4
                base[offset] = blue; base[offset + 1] = green; base[offset + 2] = red; base[offset + 3] = 255
            }
        }
        CVPixelBufferUnlockBaseAddress(buffer, [])
        var format: CMVideoFormatDescription?
        expect(CMVideoFormatDescriptionCreateForImageBuffer(allocator: kCFAllocatorDefault,
            imageBuffer: buffer, formatDescriptionOut: &format) == noErr, "Fixture format creation failed")
        var timing = CMSampleTimingInfo(duration: CMTime(value: 1, timescale: 30),
            presentationTimeStamp: CMTime(seconds: seconds, preferredTimescale: 600), decodeTimeStamp: .invalid)
        var result: CMSampleBuffer?
        expect(CMSampleBufferCreateReadyWithImageBuffer(allocator: kCFAllocatorDefault, imageBuffer: buffer,
            formatDescription: format!, sampleTiming: &timing, sampleBufferOut: &result) == noErr,
            "Fixture sample creation failed")
        return result!
    }
    static func movie(_ root: URL, name: String, duration: Double) -> URL {
        let output = root.appendingPathComponent(name + ".mp4")
        let sink = try! WriterSink(url: output, width: 640, height: 360)
        var finished = false
        var success = false
        captureQueue.async {
            let first = sample(seconds: 0, blue: 0, green: 0, red: 255)
            sink.appendScreenSample(first)
            // Long static spans prove that finalization retains the full timeline;
            // the 120-second case needs only three owned frames, not wall-clock sleep.
            let midway = sample(seconds: duration / 2, blue: 255, green: 0, red: 0)
            let end = sample(seconds: duration - 0.1, blue: 0, green: 255, red: 0)
            for frame in [midway, end] {
                let limit = Date(timeIntervalSinceNow: 5)
                while (!sink.input.isReadyForMoreMediaData || sink.pendingSample != nil) && Date() < limit {
                    sink.flushPendingSample()
                    Thread.sleep(forTimeInterval: 0.001)
                }
                sink.appendScreenSample(frame)
            }
            // This sample must be ignored by the native two-minute hard bound.
            if duration == durationLimit { sink.appendScreenSample(sample(seconds: durationLimit + 5, blue: 255, green: 255, red: 255)) }
            sink.finish(duration: duration) { ok in
                DispatchQueue.main.async { success = ok; finished = true }
            }
        }
        wait("Fixture MP4 finalization timed out") { finished }
        expect(success, "Fixture MP4 encoder failed")
        expect(fileSize(output) > 0 && fileSize(output) <= byteLimit, "Fixture MP4 exceeded export bound")
        expect((try! FileManager.default.attributesOfItem(atPath: output.path)[.posixPermissions] as! NSNumber).intValue == 0o600,
            "Fixture MP4 is not private")
        return output
    }
    static func gif(_ mp4: URL, destination: URL) {
        var finished = false
        var success = false
        exportQueue.async {
            let ok = CaptureSession.encodeGIF(mp4: mp4, gif: destination, cancellation: Cancellation())
            DispatchQueue.main.async { success = ok; finished = true }
        }
        wait("Fixture GIF conversion timed out", seconds: 180) { finished }
        expect(success, "Fixture GIF converter failed")
        expect(fileSize(destination) > 0 && fileSize(destination) <= byteLimit, "Fixture GIF exceeded export bound")
    }
    static func lifecycle(_ root: URL, mp4: URL) {
        let pending = CaptureSession(id: UUID().uuidString, root: directory(root, "pending-start"))
        pending.startPending = true
        session = pending
        expect(api(["op": "discard", "session_id": UUID().uuidString])["error"] as? String == "unknown_session",
            "A different handle controlled the native session")
        expect(api(["op": "discard", "session_id": pending.id])["phase"] as? String == "finalizing",
            "Native discard acknowledged a pending start")
        expect(pending.phase == "finalizing" && !pending.isDisposed, "Discard released a pending start")
        pending.fail("capture_start_timed_out")
        expect(pending.startPending && !pending.isDisposed, "Start timeout acknowledged a pending OS callback")
        pending.startPending = false
        pending.requestStreamStop()
        wait("Pending-start discard cleanup timed out") { pending.isDisposed }
        expect(api(["op": "status", "session_id": pending.id])["phase"] as? String == "cancelled",
            "Pending-start discard did not cancel")
        session = nil

        let stopPending = CaptureSession(id: UUID().uuidString, root: directory(root, "pending-stop"))
        stopPending.startPending = true
        stopPending.stop()
        expect(stopPending.phase == "finalizing" && !stopPending.isDisposed, "Stop cancelled pending capture incorrectly")
        stopPending.discard()
        stopPending.startPending = false
        stopPending.requestStreamStop()
        wait("Pending-stop cleanup timed out") { stopPending.isDisposed }

        let writer = CaptureSession(id: UUID().uuidString, root: directory(root, "pending-writer"))
        writer.sink = try! WriterSink(url: writer.root.appendingPathComponent("capture.mp4"), width: 640, height: 360)
        writer.writerPending = true
        writer.discard()
        expect(writer.phase == "finalizing" && !writer.isDisposed, "Discard released a pending writer")
        writer.writerPending = false
        writer.cleanup()
        wait("Pending-writer discard cleanup timed out") { writer.isDisposed }

        let exporting = CaptureSession(id: UUID().uuidString, root: directory(root, "pending-gif"))
        let copied = exporting.root.appendingPathComponent("capture.mp4")
        try! FileManager.default.copyItem(at: mp4, to: copied)
        exporting.phase = "finalizing"
        exporting.makeGIF(copied)
        exporting.discard()
        expect(!exporting.isDisposed, "Discard released an active GIF converter")
        wait("Pending-GIF discard cleanup timed out") { exporting.isDisposed }
        expect((try! FileManager.default.contentsOfDirectory(atPath: exporting.root.path)).isEmpty,
            "Discard retained native outputs")

        let capped = CaptureSession(id: UUID().uuidString, root: directory(root, "native-duration"))
        capped.phase = "capturing"
        capped.startedAt = ProcessInfo.processInfo.systemUptime - durationLimit - 1
        capped.armTimer()
        wait("Native duration timer did not stop capture", seconds: 2) { capped.stopRequested }
        capped.discard()
        wait("Duration fixture cleanup timed out") { capped.isDisposed }

        // A cancelled converter must not create an output from the source asset.
        let token = Cancellation(); token.cancel()
        let cancelled = root.appendingPathComponent("cancelled.gif")
        expect(!CaptureSession.encodeGIF(mp4: mp4, gif: cancelled, cancellation: token), "Cancelled GIF converted")
        expect(!FileManager.default.fileExists(atPath: cancelled.path), "Cancelled GIF retained a file")
    }
    static func run(_ root: URL) {
        let capabilities = api(["op": "capabilities"])
        expect(capabilities["available"] as? Bool == true, "Native capability unavailable")
        expect(api([:])["error"] as? String == "invalid_request", "Malformed native request accepted")
        expect(api(["op": "status", "session_id": "invalid"])["error"] as? String == "invalid_session",
            "Malformed session accepted")
        expect(api(["op": "start", "session_id": UUID().uuidString, "directory": root.appendingPathComponent("absent").path])["error"] as? String == "invalid_directory",
            "Absent directory accepted")
        var backgroundDone = false
        DispatchQueue.global().async {
            let response = api(["op": "capabilities"])
            expect(response["error"] as? String == "main_thread_required", "Off-main native API accepted")
            DispatchQueue.main.async { backgroundDone = true }
        }
        wait("Off-main fixture did not finish") { backgroundDone }
        let short = movie(root, name: "short", duration: 3)
        gif(short, destination: root.appendingPathComponent("short.gif"))
        let rounded = movie(root, name: "rounded", duration: 20)
        gif(rounded, destination: root.appendingPathComponent("rounded.gif"))
        let capped = movie(root, name: "capped", duration: durationLimit)
        gif(capped, destination: root.appendingPathComponent("capped.gif"))
        lifecycle(root, mp4: short)
        print(json(["ok": true, "fixture": "owned_native_pixels", "capture_invoked": false,
            "short_duration_seconds": 3, "max_duration_seconds": 120]))
    }
}

@main private struct NativeCaptureFixtureMain {
    static func main() {
        guard #available(macOS 14.0, *) else { print("SKIP native capture fixture requires macOS14"); return }
        guard CommandLine.arguments.count == 2 else { fatalError("Fixture requires private output directory") }
        NativeCaptureFixture.run(URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true))
    }
}
