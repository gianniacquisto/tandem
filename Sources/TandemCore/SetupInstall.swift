import Foundation

/// The agent face's one writer: the bundled skills, agent prompts and `AGENTS.md`
/// installed into `<TandemHome>/agent/`, the host links that point at it, the
/// manifest that records every path it made, and the warning model `tandem-cli
/// setup status` and `tandem-cli doctor` both read (ADR 0034).
///
/// The rules, in one place because the three verbs must not disagree:
///
/// - **One installed copy.** `<TandemHome>/agent/` is a real directory, replaced
///   wholesale when the bundled `VERSION` differs; the host paths are symlinks
///   into it, so an upgrade is one directory replace and no re-linking.
/// - **A link is somebody's decision.** A path that resolves into the face is the
///   install's and stays; a dead link is replaced (a dead link is not a
///   decision); a link elsewhere, a checkout included, is left and reported
///   unless `--force`; a real file is never touched, `--force` included.
/// - **The manifest outlives the replace.** `<TandemHome>/agent-install.json`
///   sits beside `agent/`, never inside it, and records the links, the host
///   instruction edits and the hosts a run targeted.
/// - **One producer of warnings.** `state(home:environment:bundle:)` is the only
///   function that builds a `Warning`; `setup status`, `setup install` (after it
///   writes) and `SetupDiagnostics.observe()` all call it.
public enum SetupInstall {

    // MARK: - Hosts

    /// A program that reads the face. `opencode` is the default; the others are
    /// written only when named, because each is a directory the human may not use.
    public enum Host: String, CaseIterable, Sendable, Codable {
        case opencode
        case claude
        case agents
    }

    /// `opencode` is the default; a comma-separated list names more than one.
    public static func hosts(from raw: String?) throws -> [Host] {
        guard let raw, !raw.isEmpty else { return [.opencode] }
        var result: [Host] = []
        for name in raw.split(separator: ",") {
            let trimmed = name.trimmingCharacters(in: .whitespaces)
            guard let host = Host(rawValue: trimmed) else {
                throw SetupInstallError.unknownHost(trimmed)
            }
            if !result.contains(host) { result.append(host) }
        }
        return result.isEmpty ? [.opencode] : result
    }

    /// `setup uninstall` without `--host` is a full run; with one it narrows.
    public static func uninstallHosts(from raw: String?) throws -> [Host]? {
        guard let raw else { return nil }
        return try hosts(from: raw)
    }

    // MARK: - The warning model

    public enum Severity: String, Sendable, Codable, CaseIterable, Comparable {
        case error
        case warn
        case info

        private var rank: Int {
            switch self {
            case .error: 0
            case .warn: 1
            case .info: 2
            }
        }

        public static func < (lhs: Severity, rhs: Severity) -> Bool { lhs.rank < rhs.rank }
    }

    /// The fourteen codes. The raw strings are the stable contract.
    public enum Code: String, Sendable, Codable, CaseIterable {
        case noBundle = "no-bundle"
        case noManifest = "no-manifest"
        case staleFace = "stale-face"
        case missingVersion = "missing-version"
        case partialFace = "partial-face"
        case deadLink = "dead-link"
        case orphanLink = "orphan-link"
        case foreignLink = "foreign-link"
        case checkoutShadowsInstall = "checkout-shadows-install"
        case blockedPath = "blocked-path"
        case doubleLoad = "double-load"
        case hostDisabled = "host-disabled"
        case halfInstalled = "half-installed"
        case unwritable = "unwritable"
    }

    public struct Warning: Sendable, Equatable, Codable {
        public let code: Code
        public let severity: Severity
        /// The host path or face path the warning is about; `nil` for a
        /// face-level code. The ordering rule and the one-per-path rule need it.
        public let path: String?
        public let detail: String
        public let hint: String

        public init(code: Code, severity: Severity, path: String?,
                    detail: String, hint: String) {
            self.code = code
            self.severity = severity
            self.path = path
            self.detail = detail
            self.hint = hint
        }
    }

    // MARK: - State and reports

    public struct HostState: Sendable, Equatable, Codable {
        public let host: String
        /// `installed`, `checkout`, `real-file` or `absent`.
        public let state: String
        public let links: Int
        public let expectedLinks: Int

        public init(host: String, state: String, links: Int, expectedLinks: Int) {
            self.host = host
            self.state = state
            self.links = links
            self.expectedLinks = expectedLinks
        }
    }

    public struct State: Sendable, Equatable, Codable {
        public let bundledVersion: String?
        public let installedVersion: String?
        public let hosts: [HostState]
        public let warnings: [Warning]

        public init(bundledVersion: String?, installedVersion: String?,
                    hosts: [HostState], warnings: [Warning]) {
            self.bundledVersion = bundledVersion
            self.installedVersion = installedVersion
            self.hosts = hosts
            self.warnings = warnings
        }

        /// Whether a warning of `warn` or `error` stands: `doctor`'s row and the
        /// three verbs' exit codes all read this one rule.
        public var hasTrouble: Bool {
            warnings.contains { $0.severity != .info }
        }

        public var hasError: Bool {
            warnings.contains { $0.severity == .error }
        }
    }

    public struct PathAction: Sendable, Equatable, Codable {
        public let path: String
        /// `linked`, `ok`, `replaced`, `skip` on install; `removed`, `left`,
        /// `absent` on uninstall.
        public let action: String
        public let reason: String?

        public init(path: String, action: String, reason: String? = nil) {
            self.path = path
            self.action = action
            self.reason = reason
        }
    }

    public struct Report: Sendable, Equatable, Codable {
        public let verb: String
        /// `used` or `absent`.
        public let manifest: String
        public let paths: [PathAction]
        public let state: State
        /// Present only on an uninstall that found no manifest: the host links
        /// could not be attributed, and the report says so rather than guessing.
        public let unattributed: Bool?

        public init(verb: String, manifest: String, paths: [PathAction],
                    state: State, unattributed: Bool? = nil) {
            self.verb = verb
            self.manifest = manifest
            self.paths = paths
            self.state = state
            self.unattributed = unattributed
        }
    }

    // MARK: - The manifest

    public struct Manifest: Sendable, Equatable, Codable {
        public struct Link: Sendable, Equatable, Codable {
            public var path: String
            public var kind: String
            public var target: String
            public var host: String

            public init(path: String, kind: String = "symlink",
                        target: String, host: String) {
                self.path = path
                self.kind = kind
                self.target = target
                self.host = host
            }
        }

        public struct Edit: Sendable, Equatable, Codable {
            public var file: String
            /// `instructions-entry` or `claude-block`.
            public var kind: String
            /// The exact text written, so removal never re-derives it.
            public var text: String
            public var host: String

            public init(file: String, kind: String, text: String, host: String) {
                self.file = file
                self.kind = kind
                self.text = text
                self.host = host
            }
        }

        public var version: String
        /// The hosts a run targeted, independent of what it managed to write.
        public var hosts: [String]
        public var links: [Link]
        public var edits: [Edit]

        public init(version: String, hosts: [String], links: [Link], edits: [Edit]) {
            self.version = version
            self.hosts = hosts
            self.links = links
            self.edits = edits
        }

        public init(from decoder: any Decoder) throws {
            let container = try decoder.container(keyedBy: CodingKeys.self)
            version = try container.decodeIfPresent(String.self, forKey: .version) ?? ""
            hosts = try container.decodeIfPresent([String].self, forKey: .hosts) ?? []
            links = try container.decodeIfPresent([Link].self, forKey: .links) ?? []
            edits = try container.decodeIfPresent([Edit].self, forKey: .edits) ?? []
        }
    }

    /// The manifest at `<home>/agent-install.json`, or `nil` when it is absent or
    /// unreadable.
    public static func manifest(home: URL) -> Manifest? {
        let path = home.appendingPathComponent("agent-install.json").path
        guard let data = try? Data(contentsOf: URL(fileURLWithPath: path)) else { return nil }
        return try? JSONDecoder().decode(Manifest.self, from: data)
    }

    static func write(_ manifest: Manifest, to url: URL) throws {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .prettyPrinted]
        let data = try encoder.encode(manifest)
        let parent = url.deletingLastPathComponent()
        try FileManager.default.createDirectory(at: parent, withIntermediateDirectories: true)
        let temporary = parent.appendingPathComponent(".agent-install-\(UUID().uuidString).json")
        try data.write(to: temporary)
        if FileManager.default.fileExists(atPath: url.path) {
            _ = try FileManager.default.replaceItemAt(url, withItemAt: temporary)
        } else {
            try FileManager.default.moveItem(at: temporary, to: url)
        }
    }

    // MARK: - Where the bundle is

    /// The agent face inside a `.app` this executable sits in: the candidate
    /// path, whether or not it exists. `nil` outside a release build in a bundle.
    static func appCandidate(executablePath: String, isDebugBuild: Bool) -> URL? {
        guard !isDebugBuild else { return nil }
        let resolved = TandemHome.resolvedPath(executablePath)
        guard TandemHome.isInsideAppBundle(resolved) else { return nil }
        var components = URL(fileURLWithPath: resolved).pathComponents
        guard let index = components.firstIndex(where: { $0.hasSuffix(".app") }) else {
            return nil
        }
        components = Array(components[...index])
        let app = components.dropFirst().joined(separator: "/")
        return URL(fileURLWithPath: "/" + app)
            .appendingPathComponent("Contents/Resources/agent")
    }

    /// The bundled face, resolved in one place: `TANDEM_AGENT_BUNDLE` (the test
    /// seam, the shape `TANDEM_HOME` already has), else the `.app` this
    /// executable runs from, else none. A dev build cannot invent a bundle.
    public static func bundle(environment: [String: String],
                              executablePath: String,
                              isDebugBuild: Bool) -> URL? {
        if let override = environment["TANDEM_AGENT_BUNDLE"], !override.isEmpty {
            let url = URL(fileURLWithPath: (override as NSString).expandingTildeInPath)
                .standardizedFileURL
            return isDirectory(url.path) ? url : nil
        }
        guard let candidate = appCandidate(executablePath: executablePath,
                                           isDebugBuild: isDebugBuild) else { return nil }
        return isDirectory(candidate.path) ? candidate : nil
    }

    /// This process's bundle: the resolver with this executable and build mode,
    /// so the CLI and `doctor` need not read the two internal values themselves.
    public static func bundledFace(environment: [String: String]) -> URL? {
        bundle(environment: environment,
               executablePath: TandemHome.executablePath,
               isDebugBuild: TandemHome.isDebugBuild)
    }

    // MARK: - Install

    public static func install(home: URL, environment: [String: String], bundle: URL?,
                               force: Bool, hosts: [Host]) throws -> Report {
        guard let bundle, isDirectory(bundle.path) else { throw SetupInstallError.noBundle }
        guard let version = versionText(of: bundle) else {
            throw SetupInstallError.invalidBundle("\(bundle.path) has no VERSION marker")
        }
        let context = Context(home: home, environment: environment)

        // Probe before writing: an unwritable home or host directory leaves
        // nothing half-written, and the report carries the diagnosis.
        let preexisting = currentState(home: home, environment: environment, bundle: bundle)
        if preexisting.hasError {
            return Report(verb: "install",
                          manifest: manifest(home: home) == nil ? "absent" : "used",
                          paths: [], state: preexisting)
        }

        try installFace(bundle: bundle, into: context.face, version: version)

        var manifest = manifest(home: home)
            ?? Manifest(version: version, hosts: [], links: [], edits: [])
        manifest.version = version
        for host in hosts where !manifest.hosts.contains(host.rawValue) {
            manifest.hosts.append(host.rawValue)
        }

        var actions: [PathAction] = []
        for host in hosts {
            for expected in expectedLinks(host: host, bundle: bundle, context: context) {
                let action = try syncLink(path: expected.path, target: expected.target,
                                          force: force, context: context)
                actions.append(action)
                if ["linked", "ok", "replaced"].contains(action.action) {
                    record(Manifest.Link(path: expected.path, target: expected.target,
                                         host: host.rawValue), in: &manifest)
                }
            }
            if let edit = instructionEdit(host: host, context: context) {
                actions.append(try apply(edit, in: &manifest))
            }
        }

        try write(manifest, to: context.manifestURL)
        let after = currentState(home: home, environment: environment, bundle: bundle)
        return Report(verb: "install", manifest: "used", paths: actions, state: after)
    }

    /// `setup status` for this process's own bundle.
    public static func status(home: URL, environment: [String: String]) -> Report {
        status(home: home, environment: environment,
               bundle: bundledFace(environment: environment),
               executablePath: TandemHome.executablePath,
               isDebugBuild: TandemHome.isDebugBuild)
    }

    /// `setup status`: the state, framed as the same report the other two verbs
    /// print, so one encoder serves all three.
    public static func status(home: URL, environment: [String: String], bundle: URL?,
                              executablePath: String, isDebugBuild: Bool) -> Report {
        Report(verb: "status",
               manifest: manifest(home: home) == nil ? "absent" : "used",
               paths: [],
               state: state(home: home, environment: environment, bundle: bundle,
                            executablePath: executablePath, isDebugBuild: isDebugBuild))
    }

    /// Materialise the face beside the old one, then rename it into place. A
    /// failure removes the temporary tree and leaves the old copy intact.
    static func installFace(bundle: URL, into face: URL, version: String) throws {
        let fm = FileManager.default
        let home = face.deletingLastPathComponent()
        try fm.createDirectory(at: home, withIntermediateDirectories: true)

        let skillNames = skills(in: bundle)
        let promptNames = prompts(in: bundle)
        let current = versionText(of: face)
        if current == version && current != nil,
           Set(skillNames) == Set(skills(in: face)),
           Set(promptNames) == Set(prompts(in: face)),
           fm.fileExists(atPath: face.appendingPathComponent("AGENTS.md").path) {
            return
        }

        let temporary = home.appendingPathComponent("agent.new-\(UUID().uuidString)")
        do {
            try fm.createDirectory(at: temporary, withIntermediateDirectories: true)
            for name in ["skills", "agents", "AGENTS.md"] {
                try fm.copyItem(at: bundle.appendingPathComponent(name),
                                to: temporary.appendingPathComponent(name))
            }
            try version.write(to: temporary.appendingPathComponent("VERSION"),
                              atomically: true, encoding: .utf8)
            let old = home.appendingPathComponent("agent.old-\(UUID().uuidString)")
            let hadOld = fm.fileExists(atPath: face.path)
            if hadOld { try fm.moveItem(at: face, to: old) }
            do {
                try fm.moveItem(at: temporary, to: face)
            } catch {
                if hadOld { try? fm.moveItem(at: old, to: face) }
                throw error
            }
            if hadOld { try? fm.removeItem(at: old) }
        } catch {
            try? fm.removeItem(at: temporary)
            throw error
        }
    }

    /// The link rule, in one function: absent is created, ours is `ok`, a dead
    /// link is replaced, a foreign link is left unless forced, and a real file is
    /// never touched.
    static func syncLink(path: String, target: String, force: Bool,
                         context: Context) throws -> PathAction {
        let fm = FileManager.default
        try fm.createDirectory(at: URL(fileURLWithPath: path).deletingLastPathComponent(),
                               withIntermediateDirectories: true)
        guard let attributes = try? fm.attributesOfItem(atPath: path) else {
            try fm.createSymbolicLink(atPath: path, withDestinationPath: target)
            return PathAction(path: path, action: "linked")
        }
        guard attributes[.type] as? FileAttributeType == .typeSymbolicLink else {
            return PathAction(path: path, action: "skip", reason: "a real file is there")
        }
        let raw = (try? fm.destinationOfSymbolicLink(atPath: path)) ?? ""
        let absolute = absoluteTarget(raw, at: path)
        let resolved = URL(fileURLWithPath: absolute).resolvingSymlinksInPath().path
        if isInside(resolved, face: context.face.path) && fm.fileExists(atPath: resolved) {
            return PathAction(path: path, action: "ok")
        }
        if !fm.fileExists(atPath: resolved) {
            try fm.removeItem(atPath: path)
            try fm.createSymbolicLink(atPath: path, withDestinationPath: target)
            return PathAction(path: path, action: "replaced")
        }
        guard force else {
            return PathAction(path: path, action: "skip",
                              reason: "points elsewhere: \(resolved)")
        }
        try fm.removeItem(atPath: path)
        try fm.createSymbolicLink(atPath: path, withDestinationPath: target)
        return PathAction(path: path, action: "replaced")
    }

    static func record(_ link: Manifest.Link, in manifest: inout Manifest) {
        manifest.links.removeAll { $0.path == link.path }
        manifest.links.append(link)
        manifest.links.sort { $0.path < $1.path }
    }

    static func record(_ edit: Manifest.Edit, in manifest: inout Manifest) {
        manifest.edits.removeAll { $0.file == edit.file && $0.kind == edit.kind }
        manifest.edits.append(edit)
        manifest.edits.sort { ($0.file, $0.kind) < ($1.file, $1.kind) }
    }

    // MARK: - The host instruction files

    struct InstructionEdit {
        let host: Host
        let file: String
        let kind: String
        /// The absolute `AGENTS.md` path (OpenCode's entry) or the block (Claude's).
        let entry: String
    }

    static func instructionEdit(host: Host, context: Context) -> InstructionEdit? {
        let agentsMD = context.face.appendingPathComponent("AGENTS.md").path
        switch host {
        case .opencode:
            return InstructionEdit(host: .opencode, file: context.opencodeConfig,
                                   kind: "instructions-entry", entry: agentsMD)
        case .claude:
            let block = """
            <!-- tandem:begin -->
            @\(agentsMD)
            <!-- tandem:end -->
            """
            return InstructionEdit(host: .claude, file: context.claudeMD,
                                   kind: "claude-block", entry: block)
        case .agents:
            return nil
        }
    }

    /// Apply one host's instruction edit: OpenCode's `instructions` entry in its
    /// config, Claude's marked block in `CLAUDE.md`. Both are textual writes so
    /// the human's formatting survives and removal is byte-exact.
    static func apply(_ edit: InstructionEdit, in manifest: inout Manifest) throws -> PathAction {
        let fm = FileManager.default
        let path = edit.file
        let existing = try? String(contentsOfFile: path, encoding: .utf8)

        switch edit.kind {
        case "instructions-entry":
            let entry = edit.entry
            if let existing, let object = tolerantJSONObject(existing) {
                if let array = object["instructions"] as? [String], array.contains(entry) {
                    return PathAction(path: path, action: "ok")
                }
            } else if existing != nil {
                return PathAction(path: path, action: "skip",
                                  reason: "opencode.json is not JSON or JSONC; edit it by hand")
            }
            let text: String
            let inserted: String
            do {
                (text, inserted) = try insertInstruction(entry, into: existing)
            } catch {
                return PathAction(path: path, action: "skip",
                                  reason: "the instructions key could not be read")
            }
            try fm.createDirectory(at: URL(fileURLWithPath: path).deletingLastPathComponent(),
                                   withIntermediateDirectories: true)
            try text.write(toFile: path, atomically: true, encoding: .utf8)
            record(Manifest.Edit(file: path, kind: edit.kind, text: inserted,
                                 host: edit.host.rawValue), in: &manifest)
            return PathAction(path: path, action: "linked")

        default:
            let block = edit.entry
            let text: String
            let recorded: String
            let action: String
            if existing == nil || existing == "" {
                recorded = block + "\n"
                text = recorded
                action = "linked"
            } else if let range = existing!.range(of: "<!-- tandem:begin -->"),
                      let end = existing!.range(of: "<!-- tandem:end -->",
                                                range: range.upperBound..<existing!.endIndex) {
                let current = String(existing![range.lowerBound..<end.upperBound])
                if current == block { return PathAction(path: path, action: "ok") }
                text = existing!.replacingCharacters(in: range.lowerBound..<end.upperBound,
                                                     with: block)
                recorded = block
                action = "replaced"
            } else {
                recorded = "\n" + block
                text = existing! + recorded
                action = "linked"
            }
            try fm.createDirectory(at: URL(fileURLWithPath: path).deletingLastPathComponent(),
                                   withIntermediateDirectories: true)
            try text.write(toFile: path, atomically: true, encoding: .utf8)
            record(Manifest.Edit(file: path, kind: edit.kind, text: recorded,
                                 host: edit.host.rawValue), in: &manifest)
            return PathAction(path: path, action: action)
        }
    }

    /// The exact text inserted, and the file's new text.
    static func insertInstruction(_ entry: String,
                                  into existing: String?) throws -> (String, String) {
        guard let existing else {
            let inserted = "{\n  \"instructions\": [\"\(entry)\"]\n}\n"
            return (inserted, inserted)
        }
        guard let open = existing.firstIndex(of: "{") else {
            throw SetupInstallError.invalidBundle("\(existing.prefix(20)) has no object")
        }
        if let keyRange = existing.range(of: "\"instructions\"") {
            guard let bracket = existing.range(of: "[", range: keyRange.upperBound..<existing.endIndex),
                  let close = closingBracket(in: existing, from: bracket.lowerBound) else {
                throw SetupInstallError.invalidBundle("the instructions array is unreadable")
            }
            let inside = existing[bracket.upperBound..<close]
            let trimmed = inside.trimmingCharacters(in: .whitespacesAndNewlines)
            let inserted = trimmed.isEmpty ? "\"\(entry)\"" : ", \"\(entry)\""
            var text = existing
            text.insert(contentsOf: inserted, at: close)
            return (text, inserted)
        }
        let inserted = "\"instructions\": [\"\(entry)\"],\n"
        var text = existing
        text.insert(contentsOf: inserted, at: existing.index(after: open))
        return (text, inserted)
    }

    /// The `]` matching the `[` at `start`, scanning a string array.
    static func closingBracket(in text: String, from start: String.Index) -> String.Index? {
        var depth = 0
        var index = start
        var inString = false
        var escaped = false
        while index < text.endIndex {
            let character = text[index]
            if inString {
                if escaped {
                    escaped = false
                } else if character == "\\" {
                    escaped = true
                } else if character == "\"" {
                    inString = false
                }
            } else {
                switch character {
                case "\"": inString = true
                case "[": depth += 1
                case "]":
                    depth -= 1
                    if depth == 0 { return index }
                default: break
                }
            }
            index = text.index(after: index)
        }
        return nil
    }

    /// Parse strictly, then tolerantly: the live config is JSONC (comments and
    /// trailing commas), and the host reads it, so this reader must too.
    static func tolerantJSONObject(_ text: String) -> [String: Any]? {
        if let object = try? JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any] {
            return object
        }
        let stripped = stripJSONC(text)
        return try? JSONSerialization.jsonObject(with: Data(stripped.utf8)) as? [String: Any]
    }

    /// Remove `//` and `/* */` comments and trailing commas from a copy. The
    /// answer it serves is "is the entry there", and the write is textual.
    static func stripJSONC(_ text: String) -> String {
        var output = ""
        var index = text.startIndex
        var inString = false
        var escaped = false
        while index < text.endIndex {
            let character = text[index]
            if inString {
                output.append(character)
                if escaped {
                    escaped = false
                } else if character == "\\" {
                    escaped = true
                } else if character == "\"" {
                    inString = false
                }
                index = text.index(after: index)
                continue
            }
            if character == "\"" {
                inString = true
                output.append(character)
                index = text.index(after: index)
                continue
            }
            if character == "/", text.index(after: index) < text.endIndex {
                let next = text[text.index(after: index)]
                if next == "/" {
                    while index < text.endIndex, text[index] != "\n" {
                        index = text.index(after: index)
                    }
                    continue
                }
                if next == "*" {
                    index = text.index(index, offsetBy: 2)
                    while index < text.endIndex {
                        if text[index] == "*", text.index(after: index) < text.endIndex,
                           text[text.index(after: index)] == "/" {
                            index = text.index(index, offsetBy: 2)
                            break
                        }
                        index = text.index(after: index)
                    }
                    continue
                }
            }
            output.append(character)
            index = text.index(after: index)
        }
        return removeTrailingCommas(output)
    }

    static func removeTrailingCommas(_ text: String) -> String {
        var output = ""
        var inString = false
        var escaped = false
        let characters = Array(text)
        for (index, character) in characters.enumerated() {
            if inString {
                output.append(character)
                if escaped {
                    escaped = false
                } else if character == "\\" {
                    escaped = true
                } else if character == "\"" {
                    inString = false
                }
                continue
            }
            if character == "\"" {
                inString = true
                output.append(character)
                continue
            }
            if character == "," {
                var next = index + 1
                while next < characters.count,
                      characters[next].isWhitespace {
                    next += 1
                }
                if next < characters.count,
                   characters[next] == "]" || characters[next] == "}" {
                    continue
                }
            }
            output.append(character)
        }
        return output
    }

    // MARK: - Uninstall

    public static func uninstall(home: URL, environment: [String: String],
                                 hosts: [Host]?) throws -> Report {
        let fm = FileManager.default
        let context = Context(home: home, environment: environment)
        guard let manifest = manifest(home: home) else {
            var actions: [PathAction] = []
            if hosts == nil {
                if fm.fileExists(atPath: context.face.path) {
                    try fm.removeItem(at: context.face)
                    actions.append(PathAction(path: context.face.path, action: "removed"))
                } else {
                    actions.append(PathAction(path: context.face.path, action: "absent"))
                }
            }
            let after = currentState(home: home, environment: environment, bundle: nil)
            return Report(verb: "uninstall", manifest: "absent", paths: actions,
                          state: after, unattributed: hosts == nil ? true : nil)
        }

        let narrowed = hosts.map { Set($0.map(\.rawValue)) }
        var remaining = manifest
        var actions: [PathAction] = []

        for link in manifest.links where narrowed?.contains(link.host) ?? true {
            actions.append(try removeRecordedLink(link, context: context))
        }
        for edit in manifest.edits where narrowed?.contains(edit.host) ?? true {
            actions.append(try removeRecordedEdit(edit))
        }
        remaining.links.removeAll { narrowed?.contains($0.host) ?? true }
        remaining.edits.removeAll { narrowed?.contains($0.host) ?? true }
        remaining.hosts.removeAll { narrowed?.contains($0) ?? true }

        if hosts == nil {
            if fm.fileExists(atPath: context.face.path) {
                try fm.removeItem(at: context.face)
                actions.append(PathAction(path: context.face.path, action: "removed"))
            } else {
                actions.append(PathAction(path: context.face.path, action: "absent"))
            }
            try? fm.removeItem(at: context.manifestURL)
        } else if remaining.links.isEmpty && remaining.edits.isEmpty && remaining.hosts.isEmpty {
            try? fm.removeItem(at: context.manifestURL)
        } else {
            try write(remaining, to: context.manifestURL)
        }

        let after = currentState(home: home, environment: environment, bundle: nil)
        return Report(verb: "uninstall", manifest: "used", paths: actions, state: after)
    }

    static func removeRecordedLink(_ link: Manifest.Link, context: Context) throws -> PathAction {
        let fm = FileManager.default
        let path = link.path
        guard let attributes = try? fm.attributesOfItem(atPath: path) else {
            return PathAction(path: path, action: "absent")
        }
        guard attributes[.type] as? FileAttributeType == .typeSymbolicLink else {
            return PathAction(path: path, action: "left", reason: "a real file is there")
        }
        let raw = (try? fm.destinationOfSymbolicLink(atPath: path)) ?? ""
        let absolute = absoluteTarget(raw, at: path)
        let resolved = URL(fileURLWithPath: absolute).resolvingSymlinksInPath().path
        guard isInside(absolute, face: context.face.path)
            || isInside(resolved, face: context.face.path) else {
            return PathAction(path: path, action: "left",
                              reason: "points elsewhere: \(resolved)")
        }
        try fm.removeItem(atPath: path)
        return PathAction(path: path, action: "removed")
    }

    static func removeRecordedEdit(_ edit: Manifest.Edit) throws -> PathAction {
        let fm = FileManager.default
        let path = edit.file
        guard let text = try? String(contentsOfFile: path, encoding: .utf8) else {
            return PathAction(path: path, action: "absent")
        }
        guard let range = text.range(of: edit.text) else {
            return PathAction(path: path, action: "left", reason: "the recorded text is gone")
        }
        let remaining = text.replacingCharacters(in: range, with: "")
        if remaining.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            try fm.removeItem(atPath: path)
        } else {
            try remaining.write(toFile: path, atomically: true, encoding: .utf8)
        }
        return PathAction(path: path, action: "removed")
    }

    // MARK: - State: the one producer of warnings

    public static func state(home: URL, environment: [String: String], bundle: URL?,
                             executablePath: String,
                             isDebugBuild: Bool) -> State {
        let fm = FileManager.default
        let context = Context(home: home, environment: environment)
        let bundle = bundle.flatMap { isDirectory($0.path) ? $0 : nil }
        let bundledVersion = bundle.flatMap { versionText(of: $0) }
        let faceExists = isDirectory(context.face.path)
        let installedVersion = faceExists ? versionText(of: context.face) : nil
        let manifest = manifest(home: home)
        let recorded = Set(manifest?.links.map(\.path) ?? [])
        var warnings: [Warning] = []

        if bundle == nil {
            let candidate = appCandidate(executablePath: executablePath,
                                         isDebugBuild: isDebugBuild)
            warnings.append(Warning(
                code: .noBundle, severity: candidate != nil ? .warn : .info, path: nil,
                detail: "no bundled agent face",
                hint: "run inside Tandem.app, or set TANDEM_AGENT_BUNDLE"))
        }

        if faceExists && manifest == nil {
            warnings.append(Warning(
                code: .noManifest, severity: .warn, path: context.face.path,
                detail: "\(context.face.path) exists with no agent-install.json",
                hint: "tandem-cli setup install"))
        }

        if faceExists {
            if installedVersion == nil {
                warnings.append(Warning(
                    code: .missingVersion, severity: .warn,
                    path: context.face.appendingPathComponent("VERSION").path,
                    detail: "\(context.face.path)/VERSION is absent",
                    hint: "tandem-cli setup install"))
            } else if let bundledVersion, let installedVersion, bundledVersion != installedVersion {
                warnings.append(Warning(
                    code: .staleFace, severity: .warn, path: context.face.path,
                    detail: "installed \(installedVersion), bundle \(bundledVersion)",
                    hint: "tandem-cli setup install"))
            } else if let bundle, installedVersion != nil,
                      let detail = partialDetail(bundle: bundle, face: context.face) {
                warnings.append(Warning(
                    code: .partialFace, severity: .warn, path: context.face.path,
                    detail: detail, hint: "tandem-cli setup install"))
            }
        }

        let targeted = Set(manifest?.hosts ?? [])
        var expected: [Host: [ExpectedLink]] = [:]
        for host in Host.allCases {
            expected[host] = bundle.map {
                expectedLinks(host: host, bundle: $0, context: context)
            } ?? []
        }
        let expectedPaths = Set(expected.values.flatMap { $0.map(\.path) })

        // The scan set: the four host directories, the two instruction files and
        // the OpenCode config. A path is Tandem's only when its name is one the
        // bundle ships, the manifest records it, or it resolves into the face.
        var candidates = expectedPaths
        for directory in [context.opencodeSkills, context.opencodeAgents,
                          context.claudeSkills, context.agentsSkills] {
            let entries = (try? fm.contentsOfDirectory(atPath: directory)) ?? []
            for entry in entries { candidates.insert(directory + "/" + entry) }
        }

        for path in candidates.sorted() {
            guard isTandems(path, context: context, bundle: bundle, recorded: recorded) else {
                continue
            }
            if let warning = classify(path, context: context, recorded: recorded,
                                      targeted: targeted,
                                      current: installedVersion != nil
                                        && installedVersion == bundledVersion
                                        ? installedVersion : nil) {
                warnings.append(warning)
            }
        }

        // double-load: the old global AGENTS.md beside the instructions entry.
        let configText = (try? String(contentsOfFile: context.opencodeConfig, encoding: .utf8)) ?? ""
        let agentsMD = context.face.appendingPathComponent("AGENTS.md").path
        if fm.fileExists(atPath: context.opencodeAgentsMD), configText.contains(agentsMD) {
            warnings.append(Warning(
                code: .doubleLoad, severity: .warn, path: context.opencodeAgentsMD,
                detail: "\(context.opencodeAgentsMD) and the instructions entry both load \(agentsMD)",
                hint: "remove \(context.opencodeAgentsMD); the instructions entry is the installed route"))
        }

        // host-disabled: a host is installed and OpenCode will not read it.
        let installedHosts = installedHostSet(manifest: manifest, expected: expected,
                                              context: context)
        for host in [Host.claude, Host.agents] where installedHosts.contains(host) {
            let names = host == .claude
                ? ["OPENCODE_DISABLE_CLAUDE_CODE", "OPENCODE_DISABLE_CLAUDE_CODE_SKILLS"]
                : ["OPENCODE_DISABLE_EXTERNAL_SKILLS", "OPENCODE_DISABLE_CLAUDE_CODE_SKILLS"]
            if let name = names.first(where: { truthy(environment[$0]) }) {
                warnings.append(Warning(
                    code: .hostDisabled, severity: .warn, path: nil,
                    detail: "\(host.rawValue) is installed, \(name) is set",
                    hint: "unset it, or tandem-cli setup uninstall --host \(host.rawValue)"))
            }
        }

        // half-installed: the links without the instruction file, or the reverse.
        for host in [Host.opencode, Host.claude] where targeted.contains(host.rawValue) {
            let links = expected[host] ?? []
            let present = links.filter {
                resolvesIntoFace($0.path, context: context)
            }.count
            let edit = instructionEdit(host: host, context: context)
            let instruction: Bool
            if let edit {
                let text = (try? String(contentsOfFile: edit.file, encoding: .utf8)) ?? ""
                instruction = edit.kind == "instructions-entry"
                    ? text.contains(edit.entry)
                    : text.contains("<!-- tandem:begin -->")
            } else {
                instruction = true
            }
            if links.isEmpty { continue }
            if instruction && present == 0 {
                warnings.append(Warning(
                    code: .halfInstalled, severity: .warn, path: nil,
                    detail: "\(host.rawValue) has its instruction file but no links",
                    hint: "tandem-cli setup install --host \(host.rawValue)"))
            } else if !instruction && present == links.count {
                warnings.append(Warning(
                    code: .halfInstalled, severity: .warn, path: nil,
                    detail: "\(host.rawValue) has its links but no instruction file",
                    hint: "tandem-cli setup install --host \(host.rawValue)"))
            }
        }

        // unwritable: probed on the nearest existing ancestor, so an absent home
        // is not an error.
        for path in [home.path, context.opencodeSkills, context.opencodeAgents,
                     context.claudeSkills, context.agentsSkills] {
            let probe = nearestExisting(path)
            if !fm.isWritableFile(atPath: probe) {
                warnings.append(Warning(
                    code: .unwritable, severity: .error, path: probe,
                    detail: "\(probe) is not writable",
                    hint: "fix the permissions, or set TANDEM_HOME"))
            }
        }

        warnings = ordered(warnings)

        var states: [HostState] = []
        for host in Host.allCases {
            let links = expected[host] ?? []
            let present = links.filter { resolvesIntoFace($0.path, context: context) }.count
            let state: String
            if links.isEmpty {
                let recordedLinks = manifest?.links.filter { $0.host == host.rawValue } ?? []
                let live = recordedLinks.contains {
                    isInside(absoluteTarget(
                        (try? fm.destinationOfSymbolicLink(atPath: $0.path)) ?? "", at: $0.path),
                             face: context.face.path)
                }
                state = live ? "installed" : "absent"
            } else if present == links.count {
                state = "installed"
            } else if links.contains(where: { isForeignLink($0.path, context: context) }) {
                state = "checkout"
            } else if links.contains(where: { isRealFile($0.path) }) {
                state = "real-file"
            } else {
                state = "absent"
            }
            states.append(HostState(host: host.rawValue, state: state,
                                    links: present, expectedLinks: links.count))
        }

        return State(bundledVersion: bundledVersion,
                     installedVersion: installedVersion,
                     hosts: states,
                     warnings: warnings)
    }

    /// The process's own view of the state: this build's executable and mode.
    static func currentState(home: URL, environment: [String: String],
                             bundle: URL?) -> State {
        state(home: home, environment: environment, bundle: bundle,
              executablePath: TandemHome.executablePath,
              isDebugBuild: TandemHome.isDebugBuild)
    }

    static func ordered(_ warnings: [Warning]) -> [Warning] {
        var seen = Set<String>()
        var unique: [Warning] = []
        for warning in warnings {
            let key = warning.code.rawValue + "|" + (warning.path ?? warning.detail)
            if seen.contains(key) { continue }
            seen.insert(key)
            unique.append(warning)
        }
        return unique.sorted { lhs, rhs in
            if lhs.severity != rhs.severity { return lhs.severity < rhs.severity }
            if lhs.code.rawValue != rhs.code.rawValue { return lhs.code.rawValue < rhs.code.rawValue }
            return (lhs.path ?? "") < (rhs.path ?? "")
        }
    }

    /// First match wins, one record for the path.
    static func classify(_ path: String, context: Context, recorded: Set<String>,
                         targeted: Set<String>, current: String?) -> Warning? {
        let fm = FileManager.default
        guard let attributes = try? fm.attributesOfItem(atPath: path) else { return nil }

        if attributes[.type] as? FileAttributeType != .typeSymbolicLink {
            guard targeted.contains(host(of: path, context: context)) else { return nil }
            return Warning(code: .blockedPath, severity: .warn, path: path,
                           detail: "\(path) holds a real file",
                           hint: "move it, then tandem-cli setup install")
        }

        let raw = (try? fm.destinationOfSymbolicLink(atPath: path)) ?? ""
        let absolute = absoluteTarget(raw, at: path)
        let resolved = URL(fileURLWithPath: absolute).resolvingSymlinksInPath().path
        if !fm.fileExists(atPath: resolved) {
            return Warning(code: .deadLink, severity: .warn, path: path,
                           detail: "\(path) -> \(absolute) (gone)",
                           hint: "tandem-cli setup install")
        }
        if isInside(resolved, face: context.face.path) {
            guard !recorded.contains(path) else { return nil }
            return Warning(code: .orphanLink, severity: .warn, path: path,
                           detail: "\(path) is not in the manifest",
                           hint: "tandem-cli setup install")
        }
        if let current {
            return Warning(
                code: .checkoutShadowsInstall, severity: .warn, path: path,
                detail: "\(path) -> \(resolved), beside a current install of \(current)",
                hint: "remove the checkout link: rm \(path)")
        }
        return Warning(code: .foreignLink, severity: .info, path: path,
                       detail: "\(path) -> \(resolved)",
                       hint: "none: a checkout route is expected on a dev machine")
    }

    static func host(of path: String, context: Context) -> String {
        if path.hasPrefix(context.opencodeSkills) || path.hasPrefix(context.opencodeAgents) {
            return Host.opencode.rawValue
        }
        if path.hasPrefix(context.claudeSkills) { return Host.claude.rawValue }
        if path.hasPrefix(context.agentsSkills) { return Host.agents.rawValue }
        return ""
    }

    static func isTandems(_ path: String, context: Context, bundle: URL?,
                          recorded: Set<String>) -> Bool {
        if recorded.contains(path) { return true }
        if resolvesIntoFace(path, context: context) { return true }
        if let raw = try? FileManager.default.destinationOfSymbolicLink(atPath: path),
           isInside(absoluteTarget(raw, at: path), face: context.face.path) {
            return true
        }
        let name = URL(fileURLWithPath: path).lastPathComponent
        if let bundle {
            if skills(in: bundle).contains(name) || prompts(in: bundle).contains(name) {
                return true
            }
        }
        return false
    }

    static func installedHostSet(manifest: Manifest?, expected: [Host: [ExpectedLink]],
                                 context: Context) -> Set<Host> {
        var hosts = Set((manifest?.hosts ?? []).compactMap(Host.init(rawValue:)))
        for (host, links) in expected where links.contains(where: {
            resolvesIntoFace($0.path, context: context)
        }) {
            hosts.insert(host)
        }
        return hosts
    }

    static func truthy(_ value: String?) -> Bool {
        guard let value else { return false }
        return value.lowercased() == "true" || value == "1"
    }

    static func partialDetail(bundle: URL, face: URL) -> String? {
        let faceSkills = Set(skills(in: face))
        let bundleSkills = Set(skills(in: bundle))
        let facePrompts = Set(prompts(in: face))
        let bundlePrompts = Set(prompts(in: bundle))
        let missing = (bundleSkills.subtracting(faceSkills).map { "skills/\($0)" }
            + bundlePrompts.subtracting(facePrompts).map { "agents/\($0)" }).sorted()
        let extra = (faceSkills.subtracting(bundleSkills).map { "skills/\($0)" }
            + facePrompts.subtracting(bundlePrompts).map { "agents/\($0)" }).sorted()
        guard !missing.isEmpty || !extra.isEmpty else { return nil }
        var parts: [String] = []
        if !missing.isEmpty { parts.append("missing: " + missing.joined(separator: ", ")) }
        if !extra.isEmpty { parts.append("extra: " + extra.joined(separator: ", ")) }
        return parts.joined(separator: "; ")
    }

    // MARK: - Paths inside the face

    struct ExpectedLink {
        let path: String
        let target: String
    }

    static func expectedLinks(host: Host, bundle: URL, context: Context) -> [ExpectedLink] {
        let face = context.face
        var links: [ExpectedLink] = []
        for skill in skills(in: bundle) {
            links.append(ExpectedLink(
                path: skillsDirectory(host: host, context: context) + "/" + skill,
                target: face.appendingPathComponent("skills/\(skill)").path))
        }
        if host == .opencode {
            for prompt in prompts(in: bundle) {
                links.append(ExpectedLink(
                    path: context.opencodeAgents + "/" + prompt,
                    target: face.appendingPathComponent("agents/\(prompt)").path))
            }
        }
        return links
    }

    static func skillsDirectory(host: Host, context: Context) -> String {
        switch host {
        case .opencode: context.opencodeSkills
        case .claude: context.claudeSkills
        case .agents: context.agentsSkills
        }
    }

    static func skills(in directory: URL) -> [String] {
        let path = directory.appendingPathComponent("skills").path
        let entries = (try? FileManager.default.contentsOfDirectory(atPath: path)) ?? []
        return entries.filter { isDirectory(path + "/" + $0) }.sorted()
    }

    static func prompts(in directory: URL) -> [String] {
        let path = directory.appendingPathComponent("agents").path
        let entries = (try? FileManager.default.contentsOfDirectory(atPath: path)) ?? []
        return entries.filter { $0.hasSuffix(".md") && !isDirectory(path + "/" + $0) }.sorted()
    }

    static func versionText(of directory: URL) -> String? {
        let path = directory.appendingPathComponent("VERSION").path
        guard let text = try? String(contentsOfFile: path, encoding: .utf8) else { return nil }
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }

    static func resolvesIntoFace(_ path: String, context: Context) -> Bool {
        guard (try? FileManager.default.attributesOfItem(atPath: path))?[.type]
            as? FileAttributeType == .typeSymbolicLink else { return false }
        let raw = (try? FileManager.default.destinationOfSymbolicLink(atPath: path)) ?? ""
        let absolute = absoluteTarget(raw, at: path)
        let resolved = URL(fileURLWithPath: absolute).resolvingSymlinksInPath().path
        return isInside(resolved, face: context.face.path)
            && FileManager.default.fileExists(atPath: resolved)
    }

    static func isForeignLink(_ path: String, context: Context) -> Bool {
        guard (try? FileManager.default.attributesOfItem(atPath: path))?[.type]
            as? FileAttributeType == .typeSymbolicLink else { return false }
        let raw = (try? FileManager.default.destinationOfSymbolicLink(atPath: path)) ?? ""
        let resolved = URL(fileURLWithPath: absoluteTarget(raw, at: path))
            .resolvingSymlinksInPath().path
        return FileManager.default.fileExists(atPath: resolved)
            && !isInside(resolved, face: context.face.path)
    }

    static func isRealFile(_ path: String) -> Bool {
        guard let attributes = try? FileManager.default.attributesOfItem(atPath: path) else {
            return false
        }
        return attributes[.type] as? FileAttributeType != .typeSymbolicLink
    }

    static func isInside(_ path: String, face: String) -> Bool {
        path == face || path.hasPrefix(face + "/")
    }

    static func absoluteTarget(_ target: String, at link: String) -> String {
        if target.hasPrefix("/") { return target }
        let parent = URL(fileURLWithPath: link).deletingLastPathComponent().path
        return URL(fileURLWithPath: parent).appendingPathComponent(target).standardized.path
    }

    static func isDirectory(_ path: String) -> Bool {
        var isDir: ObjCBool = false
        guard FileManager.default.fileExists(atPath: path, isDirectory: &isDir) else {
            return false
        }
        return isDir.boolValue
    }

    static func nearestExisting(_ path: String) -> String {
        var current = path
        let fm = FileManager.default
        while current != "/" && !fm.fileExists(atPath: current) {
            current = URL(fileURLWithPath: current).deletingLastPathComponent().path
        }
        return current
    }

    // MARK: - The environment's paths

    struct Context {
        let home: URL
        let environment: [String: String]

        var face: URL { home.appendingPathComponent("agent") }
        var manifestURL: URL { home.appendingPathComponent("agent-install.json") }

        var userHome: String {
            if let home = environment["HOME"], !home.isEmpty { return home }
            return NSHomeDirectory()
        }

        var configHome: String {
            if let xdg = environment["XDG_CONFIG_HOME"], !xdg.isEmpty { return xdg }
            return userHome + "/.config"
        }

        var opencodeSkills: String { configHome + "/opencode/skills" }
        var opencodeAgents: String { configHome + "/opencode/agents" }
        var opencodeConfig: String { configHome + "/opencode/opencode.json" }
        var opencodeAgentsMD: String { configHome + "/opencode/AGENTS.md" }
        var claudeSkills: String { userHome + "/.claude/skills" }
        var claudeMD: String { userHome + "/.claude/CLAUDE.md" }
        var agentsSkills: String { userHome + "/.agents/skills" }
    }
}

/// The setup verbs' failures: each one is a message an agent or human can act on.
public enum SetupInstallError: Error, Sendable, Equatable {
    /// No bundled face and no `TANDEM_AGENT_BUNDLE`: a dev build cannot invent one.
    case noBundle
    /// The bundle exists but is not a face (no `VERSION`, no object to insert into).
    case invalidBundle(String)
    /// `--host` named a host that does not exist.
    case unknownHost(String)
}

extension SetupInstallError: TandemError {
    public var message: String {
        switch self {
        case .noBundle:
            "no bundled agent face; run the CLI inside Tandem.app, or set TANDEM_AGENT_BUNDLE"
        case .invalidBundle(let detail):
            "the bundled agent face is not usable: \(detail)"
        case .unknownHost(let name):
            "unknown host \(name); name opencode, claude or agents"
        }
    }
}
