import SwiftUI
import WebKit
import AppKit
import UniformTypeIdentifiers
import TandemCore

/// Locates the editor bundle (one HTML page, one JS bundle, one CSS file +
/// the KaTeX fonts — ticket 07). Two paths, the `BrandAsset` pattern:
/// 1. the SwiftPM resource bundle — `swift build` emits
///    `tandem_Tandem.bundle` NEXT TO the binary (dev) or, in the assembled
///    app, under `Contents/Resources/` (the copy is in `make-app-bundle.sh`);
///    `Bundle.module` resolves both;
/// 2. a source-checkout fallback (walking up from this file) so a bare
///    `swift run` from an odd layout never renders an empty editor.
enum EditorAsset {
    /// The directory holding `index.html`, `editor.js`, `style.css`, `fonts/`.
    static let directory: URL = {
        if let url = Bundle.module.url(forResource: "Editor", withExtension: nil),
           FileManager.default.fileExists(
               atPath: url.appendingPathComponent("index.html").path)
        {
            return url
        }
        var dir = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
        for _ in 0..<6 {
            let candidate = dir.appendingPathComponent("Sources/Tandem/Resources/Editor")
            if FileManager.default.fileExists(
                atPath: candidate.appendingPathComponent("index.html").path)
            {
                return candidate
            }
            dir = dir.deletingLastPathComponent()
        }
        return URL(fileURLWithPath: "/dev/null")
    }()
}

/// The Swift side of the bridge (notes/bridge-contract.md) and the WKWebView
/// host for the bundled CM6 editor page (spec: "Editor — … Bridge").
///
/// The webview instance is owned by the model, so it lives for the app's
/// lifetime: switching tabs re-embeds the same NSView instead of reloading
/// the page (one window, one editor state). JS → Swift arrives as
/// `WKScriptMessageHandler` messages on the `"tandem"` handler; Swift → JS
/// leaves via `callAsyncJavaScript` against the page globals. The JS side
/// has no file or network access — everything crosses this bridge.
final class EditorHost: NSObject, WKScriptMessageHandler, WKURLSchemeHandler,
                         WKNavigationDelegate {
    let webView: WKWebView
    /// The owning model. The model builds this host during its own init —
    /// where Swift forbids handing `self` to a nested initializer — so it
    /// attaches itself via `attach(to:)` right after. Every callback below
    /// arrives asynchronously (page load, messages, scheme requests), so the
    /// attach is always complete by the time any of them can run.
    private weak var modelRef: BikeshedModel?
    var model: BikeshedModel { modelRef! }
    /// The page has posted `loaded` (after DOM + bundle init). Per the
    /// contract, the Swift side only sends `set*` after `loaded`; anything
    /// earlier is dropped (`editorLoaded` re-sends the current state).
    private(set) var pageLoaded = false
    /// The pending `data-theme` on `<html>` ("day"/"night"); applied on
    /// `loaded` if the appearance was set before the page finished.
    private var theme = "day"

    override init() {
        // `self` may not be handed out before `super.init()`, so the handlers
        // are registered on the live configuration right after it — still
        // before the first load, when messages or scheme requests can
        // arrive. (`WKWebView.configuration` is a copy, so register on the
        // webview's own configuration, not the local one.)
        webView = WKWebView(frame: .zero, configuration: WKWebViewConfiguration())
        super.init()
        // JS → Swift: the one handler the contract names.
        webView.configuration.userContentController.add(
            self, contentWorld: .page, name: "tandem")
        // Local images/attachments: `tandem:<vault-relative-path>`.
        webView.configuration.setURLSchemeHandler(self, forURLScheme: "tandem")
        webView.navigationDelegate = self
        // The webview never leaves the bundled page (the contract's
        // invariant 4): navigation is intercepted in the delegate.
        loadPage()
    }

    private func loadPage() {
        // `loadFileURL` with read access scoped to the Editor directory only
        // (style.css, editor.js, fonts/ are siblings of index.html).
        let dir = EditorAsset.directory
        webView.loadFileURL(
            dir.appendingPathComponent("index.html"),
            allowingReadAccessTo: dir)
    }

    /// The model links itself in here once it exists (its own init can't
    /// pass `self` to a nested initializer; see `modelRef`).
    func attach(to model: BikeshedModel) {
        modelRef = model
    }

    // MARK: JS → Swift (the contract's messages, one JSON string each)

    func userContentController(_ controller: WKUserContentController,
                               didReceive message: WKScriptMessage) {
        // The JS side posts `JSON.stringify(payload)` — a single JSON string
        // per message (see the bundle's bridge glue).
        guard message.name == "tandem",
              let body = message.body as? String,
              let data = body.data(using: .utf8),
              let obj = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let type = obj["type"] as? String
        else { return }
        switch type {
        case "loaded":
            pageLoaded = true
            applyTheme()
            model.editorLoaded()
        case "dirty":
            model.editorDirty(text: obj["text"] as? String ?? "")
        case "link-opened":
            model.linkOpened(target: obj["target"] as? String ?? "")
        case "checkbox-toggled":
            // Informational (the contract): the document change itself
            // arrives via `dirty`, which autosaves — nothing to do here.
            break
        default:
            break
        }
    }

    // MARK: Swift → JS (the contract's calls, via callAsyncJavaScript)

    /// `setDocument(markdown, notePath)` — replace the document contents
    /// (open a note, adopt an external change, "Take theirs"). Resets the
    /// dirty state; emits nothing.
    func sendDocument(_ markdown: String, notePath: String) {
        send("setDocument(arguments.markdown, arguments.notePath)",
             arguments: ["markdown": markdown, "notePath": notePath])
    }

    /// `insertText(text)` — insert at the caret.
    func insertText(_ text: String) {
        send("insertText(arguments.text)", arguments: ["text": text])
    }

    /// `revealLine(line)` — scroll so the 1-based line is visible and place
    /// the caret at its start.
    func revealLine(_ line: Int) {
        send("revealLine(arguments.line)", arguments: ["line": line])
    }

    /// `setVaultIndex(notes)` — the current file set: vault-root-relative
    /// paths of every note (case as on disk), sent on load and after every
    /// vault change.
    ///
    /// Contract note: the bridge-contract note writes the payload as
    /// `[{path: string}]`, but the committed bundle (ticket 07) consumes a
    /// flat `string[]` — `resolveWikilink(target, notes)` iterates raw path
    /// strings and the bundle's 136/136 tests exercise that shape. The Swift
    /// side sends what the bundle consumes (the set's meaning is identical);
    /// reported as a deviation, not silent.
    func sendVaultIndex(paths: [String]) {
        send("setVaultIndex(arguments.notes)", arguments: ["notes": paths])
    }

    /// The Ligne day/night theme on the page's `<html>` (the bundle switches
    /// its CSS custom properties on `[data-theme]`; day is the default).
    /// Tracked against the window's effective appearance by the face.
    func setTheme(_ name: String) {
        theme = name
        applyTheme()
    }

    private func applyTheme() {
        guard pageLoaded else { return }
        webView.evaluateJavaScript(
            "document.documentElement.setAttribute('data-theme', '\(theme)')")
    }

    /// All `set*` calls land here: the page is only ready after `loaded`,
    /// and the contract's loading rule re-sends the state at that point.
    private func send(_ functionBody: String, arguments: [String: Any]) {
        guard pageLoaded else { return }
        // `callAsyncJavaScript` is @MainActor in this SDK (WK_SWIFT_UI_ACTOR);
        // hop to the main actor — a bare `Task {}` would run the call on the
        // global executor. The result is fire-and-forget (the JS side reports
        // state back through messages, never via the call's return value).
        Task { @MainActor in
            _ = try? await webView.callAsyncJavaScript(
                functionBody, arguments: arguments, in: nil, contentWorld: .page)
        }
    }

    // MARK: WKNavigationDelegate — the webview never leaves the bundled page

    func webView(_ webView: WKWebView,
                 decidePolicyFor navigationAction: WKNavigationAction)
        async -> WKNavigationActionPolicy {
        // The bundled page loads over file://; anything else (a link click
        // that escaped the JS-side interception) is cancelled — every link
        // click arrives as `link-opened` instead (the contract's invariant).
        if let url = navigationAction.request.url, url.isFileURL {
            return .allow
        }
        return .cancel
    }

    // MARK: WKURLSchemeHandler — `tandem:<vault-relative-path>`

    /// Serves local images/attachments, confined to the vault root (the
    /// no-network invariant: remote `http(s)` targets are rewritten by the
    /// JS before render and never reach here). A missing file → 404, which
    /// makes the bundle's `<img>` error handler swap in the broken-image
    /// placeholder with alt text.
    func webView(_ webView: WKWebView, start urlSchemeTask: WKURLSchemeTask) {
        let request = urlSchemeTask.request
        guard let url = request.url, url.scheme == "tandem" else {
            fail404(urlSchemeTask)
            return
        }
        // The bundle renders local references as `tandem:<vault-relative-path>`;
        // absoluteString minus the scheme prefix is that path, percent-encoded.
        let prefix = "tandem:"
        var rel = url.absoluteString
        guard rel.hasPrefix(prefix) else { fail404(urlSchemeTask); return }
        rel.removeFirst(prefix.count)

        // Path hygiene (the contract: refuse `..` escapes) — the shared GUI
        // rule (VaultPaths), the same one the model's link routing serves.
        // Decode each component first, then judge: a percent-encoded escape
        // (`%2e%2e`) is judged as `..` and refused here, not only by the
        // containment check below (which stays as defence in depth). Empty
        // components (`//`) are refused too — the core's rule; the JS only
        // ever emits well-formed vault-relative paths, so this only changes
        // malformed-input handling (404 where the double slash was a no-op).
        let components = rel.split(separator: "/", omittingEmptySubsequences: false)
            .map(String.init)
            .map { $0.removingPercentEncoding ?? $0 }
        guard let fileURL = VaultPaths.url(
            under: model.vault.directory,
            for: components.joined(separator: "/")
        ) else {
            fail404(urlSchemeTask)
            return
        }
        // Containment: the resolved path must stay under the vault root.
        let root = model.vault.directory.standardizedFileURL.path
        guard fileURL.standardizedFileURL.path.hasPrefix(root + "/") else {
            fail404(urlSchemeTask)
            return
        }

        var isDir: ObjCBool = false
        guard FileManager.default.fileExists(atPath: fileURL.path, isDirectory: &isDir),
              !isDir.boolValue,
              let data = try? Data(contentsOf: fileURL)
        else {
            fail404(urlSchemeTask)
            return
        }

        // No-store: the vault is live (agents write while the app runs), so
        // a stale cached image must not survive a file change.
        guard let response = HTTPURLResponse(
            url: url,
            statusCode: 200,
            httpVersion: nil,
            headerFields: [
                "Cache-Control": "no-store",
                "Content-Type": Self.mimeType(for: fileURL),
            ])
        else {
            fail404(urlSchemeTask)
            return
        }
        urlSchemeTask.didReceive(response)
        urlSchemeTask.didReceive(data)
        urlSchemeTask.didFinish()
    }

    func webView(_ webView: WKWebView, stop urlSchemeTask: WKURLSchemeTask) {
        // WebKit's contract: no callbacks on a stopped task. In-flight reads
        // are simply abandoned (the task is discarded with the request).
    }

    private func fail404(_ task: WKURLSchemeTask) {
        let url = task.request.url ?? URL(fileURLWithPath: "/dev/null")
        guard let response = HTTPURLResponse(
            url: url, statusCode: 404, httpVersion: nil,
            headerFields: ["Cache-Control": "no-store"])
        else {
            task.didFailWithError(
                NSError(domain: "TandemEditorHost", code: 404))
            return
        }
        task.didReceive(response)
        task.didFinish()
    }

    private static func mimeType(for url: URL) -> String {
        UTType(filenameExtension: url.pathExtension)?.preferredMIMEType
            ?? "application/octet-stream"
    }
}

/// The editor column's live-preview pane: embeds the model's PERSISTENT
/// WKWebView (the same NSView across tab switches — no page reload, the
/// document and its decorations survive). The Ligne theme follows the window
/// appearance via `data-theme` on the page's `<html>`.
struct BikeEditorPane: NSViewRepresentable {
    let model: BikeshedModel
    @Environment(\.colorScheme) private var colorScheme

    func makeNSView(context: Context) -> WKWebView {
        model.editor.setTheme(colorScheme == .dark ? "night" : "day")
        return model.editor.webView
    }

    func updateNSView(_ nsView: WKWebView, context: Context) {
        model.editor.setTheme(colorScheme == .dark ? "night" : "day")
    }
}
