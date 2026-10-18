import Foundation
import Darwin

/// A first-run / setup diagnostic (ADR 0014): reports where this machine's
/// Tandem state lives, whether the store and the bikeshed are readable,
/// whether a writer currently holds the store lock, and whether the app and the
/// agent skill are installed. It powers `tandem-cli doctor`.
///
/// The *gathering* is impure — it stats the filesystem and probes the lock —
/// but it produces a `Facts` value, and `report(_:)` is a pure function of that
/// value. So the report's shape, wording, and pass/fail rule are covered by
/// `TandemCoreTests` with injected facts, and the executable only wires the
/// real world to it (the same "tested core, thin adapter" split as the CLI).
public enum SetupDiagnostics {

    /// One row of the report. `ok` is false only for a check that is a genuine
    /// setup failure (see `Report.ok`); the informational checks always pass and
    /// carry context.
    public struct Check: Sendable, Equatable, Codable {
        public let name: String
        public let ok: Bool
        public let detail: String

        public init(name: String, ok: Bool, detail: String) {
            self.name = name
            self.ok = ok
            self.detail = detail
        }
    }

    /// The whole report. `ok` is the AND of the failing-capable checks, so a
    /// held lock, an unreadable store, or a bikeshed path that is a file make the
    /// CLI exit `1`; every informational check is `ok: true`.
    public struct Report: Sendable, Equatable, Codable {
        public let ok: Bool
        public let checks: [Check]

        public init(ok: Bool, checks: [Check]) {
            self.ok = ok
            self.checks = checks
        }
    }

    /// An agent host whose skills directory was checked. Informational: a host
    /// that is not installed is not a failure.
    public struct SkillHost: Sendable, Equatable {
        public let name: String
        public let path: String
        public let found: Bool

        public init(name: String, path: String, found: Bool) {
            self.name = name
            self.path = path
            self.found = found
        }
    }

    /// Raw observations from the machine. Injected as a value so `report(_:)`
    /// stays pure and deterministic under test.
    public struct Facts: Sendable, Equatable {
        public var homePath: String
        public var homeSource: TandemHome.Source
        public var storePath: String
        public var storeHealth: Store.Health
        public var storeLockHeld: Bool
        public var bikeshedPath: String
        public var bikeshedNoteCount: Int
        /// The bikeshed path exists but is a regular file, not a folder.
        public var bikeshedIsFile: Bool
        public var appPresent: Bool
        public var appVersion: String?
        public var appQuarantined: Bool
        public var skillHosts: [SkillHost]
        /// `~/.config/opencode/opencode.json` lists this repo's `skills/` dir.
        public var opencodeListsSkills: Bool

        public init(homePath: String, homeSource: TandemHome.Source,
                    storePath: String, storeHealth: Store.Health, storeLockHeld: Bool,
                    bikeshedPath: String, bikeshedNoteCount: Int, bikeshedIsFile: Bool,
                    appPresent: Bool, appVersion: String?, appQuarantined: Bool,
                    skillHosts: [SkillHost], opencodeListsSkills: Bool) {
            self.homePath = homePath
            self.homeSource = homeSource
            self.storePath = storePath
            self.storeHealth = storeHealth
            self.storeLockHeld = storeLockHeld
            self.bikeshedPath = bikeshedPath
            self.bikeshedNoteCount = bikeshedNoteCount
            self.bikeshedIsFile = bikeshedIsFile
            self.appPresent = appPresent
            self.appVersion = appVersion
            self.appQuarantined = appQuarantined
            self.skillHosts = skillHosts
            self.opencodeListsSkills = opencodeListsSkills
        }
    }

    /// The check order, pinned: the CLI documents and tests it. The first three
    /// can fail; the last three are informational.
    static let checkNames = ["home", "store", "bikeshed", "lock", "app", "skills"]

    /// Assemble the report from observed `facts`. Pure.
    public static func report(_ facts: Facts) -> Report {
        var checks: [Check] = []

        let why: String
        switch facts.homeSource {
        case .environmentOverride: why = "TANDEM_HOME override"
        case .shippedApp: why = "shipped app"
        case .devBuild: why = "dev build"
        }
        checks.append(Check(name: "home", ok: true,
                            detail: "\(facts.homePath) (\(why))"))

        switch facts.storeHealth {
        case .absent:
            checks.append(Check(name: "store", ok: true,
                                detail: "\(facts.storePath) — absent (a valid empty store)"))
        case .counts(let open, let done):
            checks.append(Check(name: "store", ok: true,
                                detail: "\(facts.storePath) — \(open) open, \(done) done"))
        case .unreadable:
            checks.append(Check(name: "store", ok: false,
                                detail: "\(facts.storePath) — present but unreadable or corrupt; "
                                      + "fix the file, or let the app back it up and start empty"))
        }

        if facts.bikeshedIsFile {
            checks.append(Check(name: "bikeshed", ok: false,
                                detail: "\(facts.bikeshedPath) — a file, not a folder; pick a folder"))
        } else {
            checks.append(Check(name: "bikeshed", ok: true,
                                detail: "\(facts.bikeshedPath) — \(facts.bikeshedNoteCount) note(s)"))
        }

        checks.append(Check(name: "lock", ok: !facts.storeLockHeld,
                            detail: facts.storeLockHeld
                                ? "held by another process — retry once it finishes"
                                : "free"))

        if facts.appPresent {
            let version = facts.appVersion.map { "version \($0)" } ?? "version unknown"
            let quarantine = facts.appQuarantined
                ? "; quarantined — run: xattr -dr com.apple.quarantine /Applications/Tandem.app"
                : ""
            checks.append(Check(name: "app", ok: true,
                                detail: "installed at \(appPath) (\(version)\(quarantine))"))
        } else {
            checks.append(Check(name: "app", ok: true,
                                detail: "not installed at \(appPath)"))
        }

        checks.append(Check(name: "skills", ok: true,
                            detail: skillDetail(facts)))

        return Report(ok: checks.allSatisfy { $0.ok }, checks: checks)
    }

    /// Gather the real machine's facts, then assemble. The only impure entry
    /// point; the CLI calls this.
    public static func run() -> Report {
        report(gather())
    }

    // MARK: - Gathering (impure)

    static let appPath = "/Applications/Tandem.app"

    static func gather() -> Facts {
        let store = Store()
        let bikeshed = Bikeshed()

        let health = store.health()
        let notes = (try? bikeshed.list())?.value ?? []
        let bikeshedIsFile = isFile(bikeshed.directory.path)

        let appURL = URL(fileURLWithPath: appPath)
        let appPresent = FileManager.default.fileExists(atPath: appPath)
        let appVersion = appPresent
            ? (Bundle(url: appURL)?.infoDictionary?["CFBundleShortVersionString"] as? String)
            : nil

        return Facts(
            homePath: TandemHome.directory.path,
            homeSource: TandemHome.source(
                environment: ProcessInfo.processInfo.environment,
                executablePath: TandemHome.executablePath,
                isDebugBuild: TandemHome.isDebugBuild),
            storePath: store.directory.appendingPathComponent("todos.json").path,
            storeHealth: health,
            storeLockHeld: store.lockIsHeld(),
            bikeshedPath: bikeshed.directory.path,
            bikeshedNoteCount: noteCount(notes),
            bikeshedIsFile: bikeshedIsFile,
            appPresent: appPresent,
            appVersion: appVersion,
            appQuarantined: appPresent && hasQuarantine(appPath),
            skillHosts: skillHosts(),
            opencodeListsSkills: opencodeListsSkills())
    }

    /// Recursive count of `nodes`' notes.
    private static func noteCount(_ nodes: [BikeshedNode]) -> Int {
        nodes.reduce(0) { total, node in
            total + (node.kind == .note ? 1 : 0) + noteCount(node.children)
        }
    }

    private static func skillDetail(_ facts: Facts) -> String {
        let hosts = facts.skillHosts.map { host in
            "\(host.name) \(host.found ? "found" : "not found")"
        }.joined(separator: ", ")
        let opencode = facts.opencodeListsSkills
            ? "opencode configured"
            : "opencode not configured"
        return "\(hosts); \(opencode) — install: ln -s \"$PWD/skills/tandem\" <host>/tandem"
    }

    private static func skillHosts() -> [SkillHost] {
        let home = NSHomeDirectory()
        let hosts = [
            SkillHost(name: "agents",
                      path: "\(home)/.agents/skills/tandem",
                      found: false),
            SkillHost(name: "claude",
                      path: "\(home)/.claude/skills/tandem",
                      found: false),
        ]
        return hosts.map { host in
            SkillHost(name: host.name, path: host.path,
                      found: FileManager.default.fileExists(atPath: host.path))
        }
    }

    /// Whether `~/.config/opencode/opencode.json`'s `skills` array lists a path
    /// containing `tandem/skills` (this repo's skills directory).
    private static func opencodeListsSkills() -> Bool {
        let config = URL(fileURLWithPath: "\(NSHomeDirectory())/.config/opencode/opencode.json")
        guard
            let data = try? Data(contentsOf: config),
            let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let skills = object["skills"] as? [String]
        else { return false }
        return skills.contains { $0.contains("tandem/skills") }
    }

    // MARK: - Filesystem helpers

    /// True when `path` exists and is a regular file (not a directory). A
    /// missing path is `false` (an empty bikeshed is a valid setup).
    private static func isFile(_ path: String) -> Bool {
        var isDir: ObjCBool = false
        guard FileManager.default.fileExists(atPath: path, isDirectory: &isDir) else { return false }
        return !isDir.boolValue
    }

    /// Whether `path` carries the `com.apple.quarantine` extended attribute — the
    /// flag Homebrew leaves on a cask download until it is stripped.
    private static func hasQuarantine(_ path: String) -> Bool {
        getxattr(path, "com.apple.quarantine", nil, 0, 0, 0) >= 0
    }
}
