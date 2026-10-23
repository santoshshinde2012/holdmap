import Foundation
import AppKit
import ScreenCaptureKit
import AVFoundation
import CoreMedia
import CoreVideo
import ImageIO
import UniformTypeIdentifiers
import Darwin

/// Defaults when Rust sends no options: the original two minutes and 128 MiB bounds.
private let durationLimit = 120.0
private let byteLimit: Int64 = 128 * 1024 * 1024
/// Absolute ceilings; Rust clamps every request to them and the recorder re-checks.
private let hardDurationLimit = 3600.0
private let mp4ByteLimit: Int64 = 2 * 1024 * 1024 * 1024
private let gifByteCeiling: Int64 = 128 * 1024 * 1024
/// ImageIO keeps every GIF frame in memory until it finalizes, so frames are bounded by pixels.
private let gifMemoryBudget = 256 * 1024 * 1024

/// What the user chose before the picker opened (already clamped by Rust, clamped again here).
private struct RecordingOptions {
    var durationLimit = 120.0
    var mp4 = true
    var gif = true
    var bitrate = 5_000_000
    var gifFPS = 8
    var gifEdge = 640
    var gifBytes: Int64 = 25 * 1024 * 1024
    init() {}
    init?(_ raw: Any?) {
        guard let raw else { return }
        guard let values = raw as? [String: Any] else { return nil }
        func number(_ key: String, _ low: Double, _ high: Double, _ fallback: Double) -> Double {
            guard let value = (values[key] as? NSNumber)?.doubleValue, value.isFinite else { return fallback }
            return min(high, max(low, value))
        }
        durationLimit = number("max_duration_ms", 1_000, hardDurationLimit * 1000, durationLimit * 1000) / 1000
        mp4 = (values["mp4"] as? Bool) ?? true
        gif = (values["gif"] as? Bool) ?? true
        guard mp4 || gif else { return nil }
        bitrate = Int(number("mp4_bitrate", 500_000, 8_000_000, Double(bitrate)))
        gifFPS = Int(number("gif_fps", 1, 15, Double(gifFPS)))
        gifEdge = Int(number("gif_max_edge", 160, 960, Double(gifEdge)))
        gifBytes = Int64(number("gif_max_bytes", 1024 * 1024, Double(gifByteCeiling), Double(gifBytes)))
    }
}
private let captureQueue = DispatchQueue(label: "holdmap.capture.samples")
private let exportQueue = DispatchQueue(label: "holdmap.capture.export", qos: .userInitiated)
private let snapshotQueue = DispatchQueue(label: "holdmap.capture.snapshot", qos: .userInitiated)
private let snapshotByteLimit = 32 * 1024 * 1024
private let snapshotWidthLimit = 3840
private let snapshotHeightLimit = 2160
private func json(_ object: [String: Any]) -> String {
    guard let data = try? JSONSerialization.data(withJSONObject: object, options: [.sortedKeys]),
          let value = String(data: data, encoding: .utf8) else { return "{\"phase\":\"error\",\"error\":\"encoding_failed\"}" }
    return value
}
private func fileSize(_ url: URL) -> Int64 {
    // URL resource values may be cached while the writer is still growing a file.
    ((try? FileManager.default.attributesOfItem(atPath: url.path)[.size]) as? NSNumber)?.int64Value ?? 0
}
private func validID(_ value: String) -> Bool {
    UUID(uuidString: value) != nil && value.utf8.count == 36
}
@available(macOS 14.0, *)
private var session: CaptureSession?
@available(macOS 14.0, *)
private var snapshotJob: SnapshotCapture?

private func privateDirectory(_ request: [String: Any]) -> URL? {
    guard let path = request["directory"] as? String, path.utf8.count <= 4096,
          path.hasPrefix("/"), !path.contains("\0") else { return nil }
    let root = URL(fileURLWithPath: path, isDirectory: true)
    guard let values = try? root.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey]),
          values.isDirectory == true, values.isSymbolicLink != true,
          let attributes = try? FileManager.default.attributesOfItem(atPath: root.path),
          (attributes[.ownerAccountID] as? NSNumber)?.uint32Value == geteuid(),
          (attributes[.posixPermissions] as? NSNumber)?.intValue == 0o700,
          (try? FileManager.default.contentsOfDirectory(atPath: root.path).isEmpty) == true else { return nil }
    return root
}

// Rust calls only on the AppKit main thread. The input must be trusted native
// JSON, never a frontend path/source ID. Returned string is owned; free with API.
@_cdecl("hm_capture_command")
public func command(_ raw: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? {
    guard Thread.isMainThread else { return strdup(json(["phase": "error", "error": "main_thread_required"])) }
    guard let raw, let text = String(validatingUTF8: raw), text.utf8.count <= 8192,
          let data = text.data(using: .utf8),
          let request = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
          let op = request["op"] as? String else { return strdup(json(["phase": "error", "error": "invalid_request"])) }
    if op == "capabilities" {
        var available = false
        if #available(macOS 14.0, *) { available = true }
        // Availability only: no enumeration, permission preflight, picker or IO.
        return strdup(json(["available": available, "minimum_macos": "14.0",
            "formats": ["mp4", "gif"], "max_duration_ms": Int(hardDurationLimit * 1000),
            "max_bytes": mp4ByteLimit, "max_width": 1920, "max_height": 1080, "audio": false]))
    }
    guard #available(macOS 14.0, *) else { return strdup(json(["phase": "error", "error": "unsupported_os"])) }
    guard let id = request["session_id"] as? String, validID(id) else {
        return strdup(json(["phase": "error", "error": "invalid_session"]))
    }
    if op == "start_snapshot" || op == "snapshot_status" || op == "discard_snapshot" {
        return strdup(json(snapshotCommand(op, request: request, id: id)))
    }
    if op == "start" {
        guard snapshotJob?.ownsSelection != true else {
            return strdup(json(["phase": "error", "error": "snapshot_busy"]))
        }
        if session?.isDisposed == true { session = nil }
        guard session == nil else { return strdup(json(["phase": "error", "error": "session_exists"])) }
        // Rust owns this freshly created 0700 directory for the whole session.
        guard let root = privateDirectory(request) else {
            return strdup(json(["phase": "error", "error": "invalid_directory"]))
        }
        guard let options = RecordingOptions(request["options"]) else {
            return strdup(json(["phase": "error", "error": "invalid_options"]))
        }
        let next = CaptureSession(id: id, root: root, options: options)
        session = next
        next.choose()
        return strdup(json(next.snapshot()))
    }
    guard let current = session, current.id == id else {
        return strdup(json(["phase": "error", "error": "unknown_session"]))
    }
    switch op {
    case "status": return strdup(json(current.snapshot()))
    case "stop": current.stop(); return strdup(json(current.snapshot()))
    case "discard":
        current.discard()
        return strdup(json(current.snapshot()))
    default: return strdup(json(["phase": "error", "error": "invalid_operation"]))
    }
}
@_cdecl("hm_capture_free_json")
public func freeJSON(_ value: UnsafeMutablePointer<CChar>?) { free(value) }

@available(macOS 14.0, *)
private final class CaptureSession: NSObject, SCContentSharingPickerObserver, SCStreamDelegate {
    let id: String
    let root: URL
    let options: RecordingOptions
    var phase = "choosing"
    var error: String?
    var width = 0, height = 0
    var startedAt: TimeInterval?
    var elapsed = 0.0
    var stream: SCStream?
    var selectedFilter: SCContentFilter?
    var sink: WriterSink?
    var mp4Bytes: Int64 = 0, gifBytes: Int64 = 0
    var warnings: [String] = []
    var timer: DispatchSourceTimer?
    var deadline: DispatchWorkItem?
    let cancellation = Cancellation()
    var pickerRegistered = false
    var isDisposed = false
    var startPending = false
    var stopRequested = false
    var discardRequested = false
    var streamStopPending = false
    var writerPending = false
    var exportPending = false
    var cleanupPending = false

    init(id: String, root: URL, options: RecordingOptions = RecordingOptions()) { self.id = id; self.root = root; self.options = options }
    func snapshot() -> [String: Any] {
        var result: [String: Any] = ["session_id": id, "phase": phase,
            "elapsed_ms": Int(min(options.durationLimit, startedAt.map { ProcessInfo.processInfo.systemUptime - $0 } ?? elapsed) * 1000),
            "width": width, "height": height, "mp4_bytes": mp4Bytes, "gif_bytes": gifBytes, "warnings": warnings]
        if let error { result["error"] = error }
        return result
    }
    func setDeadline(_ seconds: TimeInterval, _ action: @escaping () -> Void) {
        deadline?.cancel()
        let work = DispatchWorkItem(block: action)
        deadline = work
        DispatchQueue.main.asyncAfter(deadline: .now() + seconds, execute: work)
    }
    func choose() {
        let picker = SCContentSharingPicker.shared
        var config = SCContentSharingPickerConfiguration()
        config.allowedPickerModes = [.singleWindow, .singleDisplay]
        config.allowsChangingSelectedContent = false
        picker.defaultConfiguration = config
        picker.maximumStreamCount = 1
        picker.add(self)
        pickerRegistered = true
        picker.isActive = true
        setDeadline(60) { [weak self] in
            guard let self, self.phase == "choosing", !self.startPending else { return }
            self.error = "picker_timed_out"
            self.releasePicker()
            self.phase = "error"
        }
        picker.present()
    }
    func releasePicker() {
        guard pickerRegistered else { return }
        SCContentSharingPicker.shared.remove(self)
        SCContentSharingPicker.shared.isActive = false
        pickerRegistered = false
    }
    func contentSharingPicker(_ picker: SCContentSharingPicker, didCancelFor stream: SCStream?) {
        DispatchQueue.main.async { [weak self] in
            guard let self, self.phase == "choosing", !self.startPending else { return }
            self.deadline?.cancel(); self.releasePicker(); self.phase = "cancelled"
        }
    }
    func contentSharingPickerStartDidFailWithError(_ error: Error) {
        DispatchQueue.main.async { [weak self] in
            guard let self, self.phase == "choosing", !self.startPending else { return }
            self.fail("picker_failed")
        }
    }
    func contentSharingPicker(_ picker: SCContentSharingPicker, didUpdateWith filter: SCContentFilter, for stream: SCStream?) {
        DispatchQueue.main.async { [weak self] in
            guard let self, self.phase == "choosing", !self.startPending, !self.discardRequested else { return }
            self.begin(filter)
        }
    }
    func begin(_ filter: SCContentFilter) {
        do {
            selectedFilter = filter
            let rect = filter.contentRect
            let scale = Double(filter.pointPixelScale)
            let sourceWidth = rect.width * scale, sourceHeight = rect.height * scale
            guard sourceWidth.isFinite, sourceHeight.isFinite, sourceWidth > 0, sourceHeight > 0 else {
                fail("invalid_dimensions"); return
            }
            let ratio = min(1, min(1920 / sourceWidth, 1080 / sourceHeight))
            width = max(2, Int(sourceWidth * ratio) / 2 * 2)
            height = max(2, Int(sourceHeight * ratio) / 2 * 2)
            let writerSink = try WriterSink(url: root.appendingPathComponent("capture.mp4"), width: width, height: height,
                bitrate: options.bitrate, durationLimit: options.durationLimit)
            sink = writerSink
            let config = SCStreamConfiguration()
            config.width = width; config.height = height
            config.minimumFrameInterval = CMTime(value: 1, timescale: 30)
            config.pixelFormat = kCVPixelFormatType_32BGRA
            config.queueDepth = 3
            config.capturesAudio = false
            if #available(macOS 15.0, *) { config.captureMicrophone = false }
            config.showsCursor = true
            config.scalesToFit = true
            let next = SCStream(filter: filter, configuration: config, delegate: self)
            try next.addStreamOutput(writerSink, type: .screen, sampleHandlerQueue: captureQueue)
            stream = next
            startPending = true
            startedAt = ProcessInfo.processInfo.systemUptime
            armTimer()
            setDeadline(15) { [weak self] in
                guard let self, self.startPending else { return }
                // A timeout does not acknowledge the outstanding OS callback.
                // Keep the stream/session and Rust directory owned until it returns.
                self.fail("capture_start_timed_out")
            }
            next.startCapture { [weak self, weak next] error in
                DispatchQueue.main.async {
                    guard let self, self.startPending else { next?.stopCapture { _ in }; return }
                    self.startPending = false
                    if !self.streamStopPending { self.deadline?.cancel() }
                    guard error == nil else { self.fail("capture_start_failed"); return }
                    if self.discardRequested || self.stopRequested || self.error != nil {
                        self.requestStreamStop()
                    } else {
                        self.phase = "capturing"
                        self.armTimer()
                    }
                }
            }
        } catch { fail("capture_setup_failed") }
    }
    func armTimer() {
        timer?.cancel(); timer = nil
        let clock = DispatchSource.makeTimerSource(queue: .main)
        clock.schedule(deadline: .now() + .milliseconds(100), repeating: .milliseconds(100))
        clock.setEventHandler { [weak self] in
            guard let self, self.phase == "capturing" || self.startPending else { return }
            let elapsed = self.startedAt.map { ProcessInfo.processInfo.systemUptime - $0 } ?? 0
            if elapsed >= self.options.durationLimit || self.sink.map({ fileSize($0.url) >= mp4ByteLimit - 1024 * 1024 }) == true {
                self.stop()
            } else if self.sink?.hasFailed == true { self.fail("mp4_failed") }
        }
        timer = clock; clock.resume()
    }
    func stream(_ stream: SCStream, didStopWithError error: Error) {
        DispatchQueue.main.async { [weak self] in
            guard let self, !self.streamStopPending,
                  self.phase == "capturing" || self.startPending else { return }
            // The system menu's Stop action follows the same finalization path.
            self.stop()
        }
    }
    func stop() {
        guard phase == "choosing" || phase == "capturing" else { return }
        stopRequested = true
        if phase == "choosing" && !startPending {
            deadline?.cancel(); releasePicker(); phase = "cancelled"; return
        }
        phase = "finalizing"
        releasePicker()
        requestStreamStop()
    }
    func requestStreamStop() {
        guard !streamStopPending else { return }
        elapsed = min(options.durationLimit, startedAt.map { ProcessInfo.processInfo.systemUptime - $0 } ?? elapsed)
        startedAt = nil
        phase = "finalizing"
        timer?.cancel(); timer = nil
        releasePicker()
        guard let active = stream else {
            if !startPending { afterStreamStopped() }
            return
        }
        streamStopPending = true
        setDeadline(10) { [weak self] in
            guard let self, self.streamStopPending else { return }
            self.error = self.error ?? "capture_stop_timed_out"
            // Retain ownership until the OS callback quiesces. Rust must keep its
            // TempDir when its own bounded polling timeout expires, too.
            self.cancellation.cancel()
        }
        active.stopCapture { [weak self] _ in
            DispatchQueue.main.async {
                guard let self, self.streamStopPending else { return }
                self.deadline?.cancel(); self.streamStopPending = false
                // A successful start may arrive after an earlier stop callback.
                // Keep its stream and ownership; start completion stops it again.
                if self.startPending { return }
                self.stream = nil
                self.afterStreamStopped()
            }
        }
    }
    func afterStreamStopped() {
        guard !startPending, !writerPending, !exportPending, !cleanupPending else { return }
        selectedFilter = nil
        if discardRequested || error != nil { cleanup(); return }
        guard let sink else { fail("missing_writer"); return }
        writerPending = true
        sink.finish(duration: elapsed) { [weak self] ok in
            DispatchQueue.main.async {
                guard let self else { return }
                self.writerPending = false
                if self.discardRequested || self.error != nil { self.cleanup(); return }
                let size = fileSize(sink.url)
                guard ok, size > 0, size <= mp4ByteLimit else { self.fail("mp4_failed"); return }
                try? FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: sink.url.path)
                if self.options.mp4 { self.mp4Bytes = size }
                // An MP4-only recording is finished; a GIF-only one keeps the MP4 just long enough to convert it.
                if self.options.gif { self.makeGIF(sink.url) } else { self.phase = "ready" }
            }
        }
    }
    func makeGIF(_ mp4: URL) {
        let gif = root.appendingPathComponent("capture.gif")
        let token = cancellation
        exportPending = true
        let options = self.options
        exportQueue.async { [weak self] in
            let ok = autoreleasepool { Self.encodeGIF(mp4: mp4, gif: gif, cancellation: token, options: options) }
            DispatchQueue.main.async {
                guard let self else { return }
                self.exportPending = false
                if self.discardRequested || self.error != nil { self.cleanup(); return }
                if ok {
                    self.gifBytes = fileSize(gif)
                    try? FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: gif.path)
                } else {
                    self.warnings.append("gif_failed")
                    try? FileManager.default.removeItem(at: gif)
                }
                if !self.options.mp4 {
                    try? FileManager.default.removeItem(at: mp4)
                    // A GIF-only recording with no GIF has nothing to offer.
                    if self.gifBytes == 0 { self.fail("gif_failed"); return }
                }
                self.phase = "ready"
            }
        }
    }
    /// Down-samples the MP4 into a GIF within the user's size budget. A GIF that comes out too large is
    /// redone with half the frames at a smaller size rather than failing the recording.
    static func encodeGIF(mp4: URL, gif: URL, cancellation: Cancellation, options: RecordingOptions = RecordingOptions()) -> Bool {
        var frameShare = 1.0, edge = Double(options.gifEdge)
        for _ in 0..<3 {
            if cancellation.isCancelled { return false }
            let outcome = autoreleasepool {
                encodeGIFOnce(mp4: mp4, gif: gif, cancellation: cancellation, options: options, frameShare: frameShare, edge: Int(edge))
            }
            if outcome == .done { return true }
            try? FileManager.default.removeItem(at: gif)
            // Only a size overrun is worth retrying; a failed frame or a cancel is final.
            guard outcome == .tooLarge else { return false }
            frameShare /= 2; edge = max(160, edge * 0.8)
        }
        return false
    }
    private enum GIFOutcome { case done, tooLarge, failed }
    private static func encodeGIFOnce(mp4: URL, gif: URL, cancellation: Cancellation, options: RecordingOptions, frameShare: Double, edge: Int) -> GIFOutcome {
        let asset = AVURLAsset(url: mp4)
        let duration = min(options.durationLimit, CMTimeGetSeconds(asset.duration))
        guard duration.isFinite, duration > 0, !cancellation.isCancelled else { return .failed }
        // Every frame stays in memory until the GIF is finalized, so cap by pixels as well as by rate.
        let frameBytes = max(1, Double(edge * edge) * 0.65 * 4)
        let memoryFrames = max(1, min(300, Int(Double(gifMemoryBudget) / frameBytes)))
        let wanted = Int(ceil(duration * Double(options.gifFPS) * frameShare))
        let frames = max(1, min(memoryFrames, wanted))
        // GIF delays use centiseconds. Distribute rounding across frames rather
        // than adding a rounding error for every frame of a long recording.
        let totalDelay = max(frames * 2, Int((duration * 100).rounded()))
        let generator = AVAssetImageGenerator(asset: asset)
        generator.appliesPreferredTrackTransform = true
        generator.maximumSize = CGSize(width: edge, height: edge)
        generator.requestedTimeToleranceBefore = .zero
        generator.requestedTimeToleranceAfter = .zero
        guard let destination = CGImageDestinationCreateWithURL(gif as CFURL, UTType.gif.identifier as CFString, frames, nil) else { return .failed }
        try? FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: gif.path)
        CGImageDestinationSetProperties(destination, [kCGImagePropertyGIFDictionary: [kCGImagePropertyGIFLoopCount: 0]] as CFDictionary)
        let began = ProcessInfo.processInfo.systemUptime
        for index in 0..<frames {
            if cancellation.isCancelled || ProcessInfo.processInfo.systemUptime - began > 180 {
                generator.cancelAllCGImageGeneration(); return .failed
            }
            let ok = autoreleasepool { () -> Bool in
                let seconds = min(max(0, duration - 1.0 / 30.0), Double(index) * duration / Double(max(1, frames - 1)))
                guard let image = try? generator.copyCGImage(at: CMTime(seconds: seconds, preferredTimescale: 600), actualTime: nil) else { return false }
                let previous = Int((Double(index) * Double(totalDelay) / Double(frames)).rounded())
                let next = Int((Double(index + 1) * Double(totalDelay) / Double(frames)).rounded())
                let delay = Double(next - previous) / 100
                CGImageDestinationAddImage(destination, image, [kCGImagePropertyGIFDictionary:
                    [kCGImagePropertyGIFDelayTime: delay, kCGImagePropertyGIFUnclampedDelayTime: delay]] as CFDictionary)
                return true
            }
            if !ok { return .failed }
        }
        guard !cancellation.isCancelled, CGImageDestinationFinalize(destination) else { return .failed }
        try? FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: gif.path)
        let size = fileSize(gif)
        if size > options.gifBytes { return .tooLarge }
        return size > 0 ? .done : .failed
    }
    func fail(_ reason: String) {
        guard !isDisposed else { return }
        error = reason
        cancellation.cancel()
        releasePicker(); timer?.cancel(); timer = nil
        phase = "finalizing"
        if writerPending || exportPending { return }
        requestStreamStop()
    }
    func discard() {
        guard !isDisposed else { return }
        discardRequested = true
        cancellation.cancel()
        releasePicker(); timer?.cancel(); timer = nil
        phase = "finalizing"
        if writerPending || exportPending { return }
        requestStreamStop()
    }
    func cleanup() {
        guard !startPending, !streamStopPending, !writerPending, !exportPending, !cleanupPending else { return }
        cleanupPending = true
        let done = { [weak self] in
            DispatchQueue.main.async {
                guard let self else { return }
                // Only fixed private filenames; the Rust-owned directory remains.
                try? FileManager.default.removeItem(at: self.root.appendingPathComponent("capture.mp4"))
                try? FileManager.default.removeItem(at: self.root.appendingPathComponent("capture.gif"))
                self.mp4Bytes = 0; self.gifBytes = 0
                self.cleanupPending = false
                self.sink = nil
                self.selectedFilter = nil
                self.isDisposed = self.discardRequested
                self.phase = self.discardRequested ? "cancelled" : "error"
            }
        }
        if let sink { sink.cancel(done) } else { done() }
    }
}

@available(macOS 14.0, *)
private func snapshotCommand(_ op: String, request: [String: Any], id: String) -> [String: Any] {
    func failure(_ reason: String) -> [String: Any] { ["session_id": id, "phase": "error", "error": reason] }
    if op == "start_snapshot" {
        if snapshotJob?.isDisposed == true { snapshotJob = nil }
        guard snapshotJob == nil else { return failure("snapshot_busy") }
        guard let root = privateDirectory(request) else { return failure("invalid_directory") }
        var filter: SCContentFilter?
        if let recording = request["recording_id"] as? String {
            guard validID(recording), let current = session, current.id == recording,
                  current.phase == "capturing", !current.discardRequested, !current.stopRequested,
                  let selected = current.selectedFilter else { return failure("recording_unavailable") }
            filter = selected
        } else {
            guard request["recording_id"] == nil || request["recording_id"] is NSNull else { return failure("invalid_request") }
            // The shared system picker is never reconfigured while a video owns its source.
            if let current = session, current.phase == "choosing" || current.phase == "capturing"
                || current.startPending || current.streamStopPending || current.stream != nil {
                return failure("recording_unavailable")
            }
        }
        let next = SnapshotCapture(id: id, root: root)
        snapshotJob = next
        if let filter { next.capture(filter) } else { next.choose() }
        return next.status()
    }
    guard let current = snapshotJob, current.id == id else { return failure("unknown_snapshot") }
    if op == "discard_snapshot" { current.discard() }
    return current.status()
}

// The image workflow owns no AVAssetWriter and never changes CaptureSession's phases.
@available(macOS 14.0, *)
private final class SnapshotCapture: NSObject, SCContentSharingPickerObserver {
    let id: String
    let root: URL
    let options: RecordingOptions
    var phase = "choosing"
    var error: String?
    var width = 0, height = 0
    var pngBytes: Int64 = 0
    var pickerRegistered = false
    var capturePending = false
    var encodePending = false
    var discardRequested = false
    var isDisposed = false
    var deadline: DispatchWorkItem?
    let cancellation = Cancellation()
    var ownsSelection: Bool { pickerRegistered || capturePending }

    init(id: String, root: URL, options: RecordingOptions = RecordingOptions()) { self.id = id; self.root = root; self.options = options }
    func status() -> [String: Any] {
        var result: [String: Any] = ["session_id": id, "phase": phase, "width": width,
            "height": height, "png_bytes": pngBytes]
        if let error { result["error"] = error }
        return result
    }
    func timeout(_ seconds: Double, _ reason: String) {
        deadline?.cancel()
        let work = DispatchWorkItem { [weak self] in self?.fail(reason) }
        deadline = work
        DispatchQueue.main.asyncAfter(deadline: .now() + seconds, execute: work)
    }
    func choose() {
        let picker = SCContentSharingPicker.shared
        var config = SCContentSharingPickerConfiguration()
        config.allowedPickerModes = [.singleWindow, .singleDisplay]
        config.allowsChangingSelectedContent = false
        picker.defaultConfiguration = config
        picker.maximumStreamCount = 1
        picker.add(self); pickerRegistered = true; picker.isActive = true
        timeout(60, "picker_timed_out")
        picker.present()
    }
    func releasePicker() {
        guard pickerRegistered else { return }
        SCContentSharingPicker.shared.remove(self)
        SCContentSharingPicker.shared.isActive = false
        pickerRegistered = false
    }
    func contentSharingPicker(_ picker: SCContentSharingPicker, didCancelFor stream: SCStream?) {
        DispatchQueue.main.async { [weak self] in
            guard let self, self.phase == "choosing" else { return }
            self.discard()
        }
    }
    func contentSharingPickerStartDidFailWithError(_ error: Error) {
        DispatchQueue.main.async { [weak self] in
            guard let self, self.phase == "choosing" else { return }
            self.fail("picker_failed")
        }
    }
    func contentSharingPicker(_ picker: SCContentSharingPicker, didUpdateWith filter: SCContentFilter, for stream: SCStream?) {
        DispatchQueue.main.async { [weak self] in
            guard let self, self.phase == "choosing", !self.discardRequested else { return }
            self.capture(filter)
        }
    }
    func capture(_ filter: SCContentFilter) {
        let sourceWidth = filter.contentRect.width * Double(filter.pointPixelScale)
        let sourceHeight = filter.contentRect.height * Double(filter.pointPixelScale)
        guard sourceWidth.isFinite, sourceHeight.isFinite, sourceWidth > 0, sourceHeight > 0 else {
            fail("invalid_dimensions"); return
        }
        let scale = min(1, min(Double(snapshotWidthLimit) / sourceWidth, Double(snapshotHeightLimit) / sourceHeight))
        width = max(1, Int(sourceWidth * scale)); height = max(1, Int(sourceHeight * scale))
        let config = SCStreamConfiguration()
        config.width = width; config.height = height
        config.pixelFormat = kCVPixelFormatType_32BGRA
        config.capturesAudio = false
        if #available(macOS 15.0, *) { config.captureMicrophone = false }
        config.showsCursor = true; config.scalesToFit = true
        phase = "capturing"; capturePending = true
        timeout(15, "snapshot_timed_out")
        SCScreenshotManager.captureImage(contentFilter: filter, configuration: config) { [self, filter] image, failure in
            // Retain the selected filter and job until this uninterruptible OS callback returns.
            _ = filter
            DispatchQueue.main.async {
                self.capturePending = false
                self.releasePicker()
                if self.discardRequested || self.error != nil { self.cleanup(); return }
                guard failure == nil, let image else { self.fail("snapshot_capture_failed"); return }
                self.encode(image)
            }
        }
    }
    func encode(_ image: CGImage) {
        guard image.width > 0, image.height > 0,
              image.width <= snapshotWidthLimit, image.height <= snapshotHeightLimit else {
            fail("invalid_dimensions"); return
        }
        width = image.width; height = image.height
        encodePending = true
        let output = root.appendingPathComponent("capture.png")
        let token = cancellation
        snapshotQueue.async { [self] in
            let size = autoreleasepool { Self.writePNG(image, to: output, cancellation: token) }
            DispatchQueue.main.async {
                self.encodePending = false
                self.deadline?.cancel()
                if self.discardRequested || self.error != nil { self.cleanup(); return }
                guard let size else { self.fail("snapshot_encode_failed"); return }
                self.pngBytes = size
                self.phase = "ready"
            }
        }
    }
    static func writePNG(_ image: CGImage, to output: URL, cancellation: Cancellation) -> Int64? {
        guard !cancellation.isCancelled, image.width > 0, image.height > 0,
              image.width <= snapshotWidthLimit, image.height <= snapshotHeightLimit else { return nil }
        let data = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(data, UTType.png.identifier as CFString, 1, nil) else { return nil }
        CGImageDestinationAddImage(destination, image, nil)
        guard CGImageDestinationFinalize(destination), !cancellation.isCancelled,
              data.length > 0, data.length <= snapshotByteLimit else { return nil }
        let descriptor = output.path.withCString { open($0, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC, 0o600) }
        guard descriptor >= 0 else { return nil }
        let file = FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
        do {
            try file.write(contentsOf: data as Data)
            try file.synchronize()
            try file.close()
            guard !cancellation.isCancelled else { return nil }
            return Int64(data.length)
        } catch {
            try? file.close()
            return nil
        }
    }
    func fail(_ reason: String) {
        guard !isDisposed else { return }
        error = reason; phase = "error"
        cancellation.cancel(); deadline?.cancel(); releasePicker()
        cleanup()
    }
    func discard() {
        guard !isDisposed else { return }
        discardRequested = true
        cancellation.cancel(); deadline?.cancel(); releasePicker()
        cleanup()
    }
    func cleanup() {
        // A timeout requests cancellation; it never acknowledges an outstanding OS/PNG callback.
        guard !capturePending, !encodePending else { return }
        try? FileManager.default.removeItem(at: root.appendingPathComponent("capture.png"))
        pngBytes = 0
        if discardRequested { isDisposed = true; phase = "cancelled" }
    }
}

private final class Cancellation {
    private let lock = NSLock()
    private var value = false
    var isCancelled: Bool { lock.lock(); defer { lock.unlock() }; return value }
    func cancel() { lock.lock(); value = true; lock.unlock() }
}

@available(macOS 14.0, *)
private final class WriterSink: NSObject, SCStreamOutput {
    let url: URL
    let writer: AVAssetWriter
    let input: AVAssetWriterInput
    var firstPTS: CMTime?
    var lastSample: CMSampleBuffer?
    var lastPTS: CMTime?
    var pendingSample: CMSampleBuffer?
    var pendingSince: TimeInterval?
    var retryScheduled = false
    var finishing = false
    var finishRequested = false
    private let failureLock = NSLock()
    private var failed = false
    var hasFailed: Bool { failureLock.lock(); defer { failureLock.unlock() }; return failed }
    private func recordFailure() { failureLock.lock(); failed = true; failureLock.unlock() }
    let limit: Double
    init(url: URL, width: Int, height: Int, bitrate: Int = 5_000_000, durationLimit limit: Double = durationLimit) throws {
        self.url = url
        self.limit = limit
        writer = try AVAssetWriter(outputURL: url, fileType: .mp4)
        let settings: [String: Any] = [AVVideoCodecKey: AVVideoCodecType.h264,
            AVVideoWidthKey: width, AVVideoHeightKey: height,
            AVVideoCompressionPropertiesKey: [AVVideoAverageBitRateKey: bitrate,
                AVVideoMaxKeyFrameIntervalKey: 60, AVVideoExpectedSourceFrameRateKey: 30]]
        input = AVAssetWriterInput(mediaType: .video, outputSettings: settings)
        input.expectsMediaDataInRealTime = true
        super.init()
        guard writer.canAdd(input) else { throw NSError(domain: "holdmap", code: 1) }
        writer.add(input)
    }
    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard !finishing, type == .screen, CMSampleBufferIsValid(sampleBuffer), CMSampleBufferDataIsReady(sampleBuffer),
              CMSampleBufferGetImageBuffer(sampleBuffer) != nil,
              let attachments = CMSampleBufferGetSampleAttachmentsArray(sampleBuffer, createIfNecessary: false) as? [[SCStreamFrameInfo: Any]],
              let raw = attachments.first?[.status] as? Int, SCFrameStatus(rawValue: raw) == .complete else { return }
        appendScreenSample(sampleBuffer)
    }
    // Same writer path is exercised with owned pixel buffers by the fixture.
    func appendScreenSample(_ sampleBuffer: CMSampleBuffer) {
        guard !finishing, !finishRequested, !hasFailed, CMSampleBufferIsValid(sampleBuffer), CMSampleBufferDataIsReady(sampleBuffer),
              CMSampleBufferGetImageBuffer(sampleBuffer) != nil else { return }
        let pts = CMSampleBufferGetPresentationTimeStamp(sampleBuffer)
        guard pts.isNumeric, lastPTS.map({ pts > $0 }) ?? true else { return }
        if firstPTS == nil {
            guard writer.startWriting() else { recordFailure(); return }
            try? FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: url.path)
            writer.startSession(atSourceTime: pts); firstPTS = pts
        }
        guard let firstPTS, CMTimeGetSeconds(pts - firstPTS) < limit, writer.status == .writing else { return }
        // Retain at most the latest pending frame. In particular, a first frame
        // cannot disappear just because the hardware encoder was not ready yet:
        // an unchanged desktop may provide no further complete frames.
        if lastSample == nil && pendingSample != nil {
            flushPendingSample()
            if pendingSample != nil { return }
        }
        pendingSample = sampleBuffer
        pendingSince = pendingSince ?? ProcessInfo.processInfo.systemUptime
        flushPendingSample()
    }
    func flushPendingSample() {
        guard !finishing, !hasFailed, let pendingSample else { return }
        if writer.status != .writing { recordFailure(); return }
        if input.isReadyForMoreMediaData {
            self.pendingSample = nil; pendingSince = nil
            if input.append(pendingSample) {
                lastSample = pendingSample
                lastPTS = CMSampleBufferGetPresentationTimeStamp(pendingSample)
            } else { recordFailure() }
        } else if ProcessInfo.processInfo.systemUptime - (pendingSince ?? 0) > 2 {
            self.pendingSample = nil; pendingSince = nil; recordFailure()
        } else if !retryScheduled {
            retryScheduled = true
            captureQueue.asyncAfter(deadline: .now() + .milliseconds(10)) { [weak self] in
                guard let self else { return }
                self.retryScheduled = false
                self.flushPendingSample()
            }
        }
    }
    func finish(duration: Double, _ done: @escaping (Bool) -> Void) {
        captureQueue.async { [self] in
            guard !finishing, !finishRequested else { done(false); return }
            finishRequested = true
            finishWhenReady(duration: duration, done)
        }
    }
    func finishWhenReady(duration: Double, _ done: @escaping (Bool) -> Void) {
        guard !finishing else { done(false); return }
        flushPendingSample()
        if pendingSample != nil && !hasFailed {
            captureQueue.asyncAfter(deadline: .now() + .milliseconds(10)) { [self] in
                finishWhenReady(duration: duration, done)
            }
            return
        }
        finishing = true
        pendingSample = nil; pendingSince = nil
        guard writer.status == .writing, let firstPTS, lastSample != nil, !hasFailed else {
            writer.cancelWriting(); done(false); return
        }
        let end = firstPTS + CMTime(seconds: min(limit, max(1.0 / 30.0, duration)), preferredTimescale: 600)
        // Extend static screen content to the user's stop time rather than
        // truncating the MP4 at the last changed desktop frame.
        if let lastSample, let lastPTS, input.isReadyForMoreMediaData {
            let finalPTS = end - CMTime(value: 1, timescale: 30)
            if finalPTS > lastPTS {
                var timing = CMSampleTimingInfo(duration: CMTime(value: 1, timescale: 30),
                    presentationTimeStamp: finalPTS, decodeTimeStamp: .invalid)
                var copy: CMSampleBuffer?
                if CMSampleBufferCreateCopyWithNewTiming(allocator: kCFAllocatorDefault, sampleBuffer: lastSample,
                    sampleTimingEntryCount: 1, sampleTimingArray: &timing, sampleBufferOut: &copy) == noErr, let copy {
                    _ = input.append(copy)
                }
            }
        }
        lastSample = nil
        writer.endSession(atSourceTime: end)
        input.markAsFinished()
        writer.finishWriting { done(self.writer.status == .completed) }
    }
    func cancel(_ done: (() -> Void)? = nil) {
        captureQueue.async { [self] in
            finishing = true; lastSample = nil; pendingSample = nil; pendingSince = nil
            if writer.status == .writing || writer.status == .unknown { writer.cancelWriting() }
            done?()
        }
    }
}
