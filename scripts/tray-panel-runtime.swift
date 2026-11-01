// Owned WKWebView regression fixture. Loads only the local built tray page with synthetic IPC.
import AppKit
import WebKit

final class Fixture: NSObject, WKScriptMessageHandler, WKNavigationDelegate {
    let window: NSWindow
    let web: WKWebView
    let output: URL
    var finished = false
    init(url: URL, data: String, checks: String, output: URL) {
        self.output = output
        let config = WKWebViewConfiguration()
        config.websiteDataStore = .nonPersistent()
        config.userContentController.addUserScript(WKUserScript(source: "window.panelData = \(data); window.__TAURI_INTERNALS__ = {invoke: async (cmd) => { if (cmd !== 'tray_panel') return; window.seenFocusSeq = window.panelData.focusSeq; return structuredClone(window.panelData); }};", injectionTime: .atDocumentStart, forMainFrameOnly: true))
        config.userContentController.addUserScript(WKUserScript(source: checks, injectionTime: .atDocumentEnd, forMainFrameOnly: true))
        web = WKWebView(frame: NSRect(x: 0, y: 0, width: 360, height: 600), configuration: config)
        window = NSWindow(contentRect: NSRect(x: 100, y: 100, width: 360, height: 600), styleMask: [.titled, .fullSizeContentView], backing: .buffered, defer: false)
        super.init()
        config.userContentController.add(self, name: "fixture")
        window.title = "holdmap tray layout fixture"
        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = true
        for button: NSWindow.ButtonType in [.closeButton, .miniaturizeButton, .zoomButton] { window.standardWindowButton(button)?.isHidden = true }
        window.contentView = web
        window.isReleasedWhenClosed = false
        web.navigationDelegate = self
        window.makeKeyAndOrderFront(nil)
        web.load(URLRequest(url: url))
        DispatchQueue.main.asyncAfter(deadline: .now() + 45) { if !self.finished { self.finish(["error": "Native fixture timed out"]) } }
    }
    func userContentController(_ userContentController: WKUserContentController, didReceive message: WKScriptMessage) {
        guard !finished else { return }
        if let report = message.body as? [String: Any] { finish(report) }
    }
    func finish(_ report: [String: Any]) {
        guard !finished else { return }
        finished = true
        web.takeSnapshot(with: nil) { image, error in
            if let tiff = image?.tiffRepresentation, let bitmap = NSBitmapImageRep(data: tiff), let png = bitmap.representation(using: .png, properties: [:]) {
                try? png.write(to: self.output.appendingPathExtension("png"))
            }
            if let bytes = try? JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys]) { try? bytes.write(to: self.output) }
            self.window.close()
            NSApplication.shared.terminate(nil)
        }
    }
    func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) { finish(["error": error.localizedDescription]) }
}
let args = CommandLine.arguments
let app = NSApplication.shared
app.setActivationPolicy(.accessory)
let fixture = Fixture(url: URL(string: args[1])!, data: try String(contentsOfFile: args[2], encoding: .utf8), checks: try String(contentsOfFile: args[3], encoding: .utf8), output: URL(fileURLWithPath: args[4]))
app.run()
