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

    /// The comments directory's state (ADR 0017, amendment 2026-10-20). Absent is
    /// valid — the first comment creates it — so the check fails only when the
    /// path is present but unreadable: a file in its place, or a directory that
    /// cannot be listed.
    public enum CommentsState: Sendable, Equatable {
        case absent
        case readable
        case unreadable
    }

    /// Raw observations from the machine. Injected as a value so `report(_:)`
    /// stays pure and deterministic under test.
    public struct Facts: Sendable, Equatable {
        public var homePath: String
        public var homeSource: TandemHome.Source
        public var storePath: String
        public var storeHealth: Store.Health
        /// The rolling backup beside the store (ADR 0018): absent, or its age.
        public var storeBackup: Store.BackupHealth
        public var storeLockHeld: Bool
        public var bikeshedPath: String
        public var bikeshedNoteCount: Int
        /// The bikeshed path exists but is a regular file, not a folder.
        public var bikeshedIsFile: Bool
        public var commentsPath: String
        public var commentsState: CommentsState
        public var appPresent: Bool
        public var appVersion: String?
        public var appQuarantined: Bool
        public var skillHosts: [SkillHost]
        /// Whether the agent host has Tandem's skills: `~/.config/opencode/opencode.json`'s
        /// `skills` array lists a path containing `tandem/skills` (this repo's skills
        /// directory), or the host's own skills directory holds the linked `tandem` skill
        /// — the route `Scripts/install-agent-host.sh` takes.
        public var opencodeHasSkills: Bool

        public init(homePath: String, homeSource: TandemHome.Source,
                    storePath: String, storeHealth: Store.Health,
                    storeBackup: Store.BackupHealth, storeLockHeld: Bool,
                    bikeshedPath: String, bikeshedNoteCount: Int, bikeshedIsFile: Bool,
                    commentsPath: String, commentsState: CommentsState,
                    appPresent: Bool, appVersion: String?, appQuarantined: Bool,
                    skillHosts: [SkillHost], opencodeHasSkills: Bool) {
            self.homePath = homePath
            self.homeSource = homeSource
            self.storePath = storePath
            self.storeHealth = storeHealth
            self.storeBackup = storeBackup
            self.storeLockHeld = storeLockHeld
            self.bikeshedPath = bikeshedPath
            self.bikeshedNoteCount = bikeshedNoteCount
            self.bikeshedIsFile = bikeshedIsFile
            self.commentsPath = commentsPath
            self.commentsState = commentsState
            self.appPresent = appPresent
            self.appVersion = appVersion
            self.appQuarantined = appQuarantined
            self.skillHosts = skillHosts
            self.opencodeHasSkills = opencodeHasSkills
        }
    }

    /// The check order, pinned: the CLI documents and tests it. The first four
    /// can fail; the last three are informational.
    static let checkNames = ["home", "store", "bikeshed", "comments", "lock", "app", "skills"]

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

        let backup = backupDetail(facts.storeBackup)
        switch facts.storeHealth {
        case .absent:
            checks.append(Check(name: "store", ok: true,
                                detail: "\(facts.storePath) — absent (a valid empty store); \(backup)"))
        case .counts(let open, let done):
            checks.append(Check(name: "store", ok: true,
                                detail: "\(facts.storePath) — \(open) open, \(done) done; \(backup)"))
        case .unreadable:
            checks.append(Check(name: "store", ok: false,
                                detail: "\(facts.storePath) — present but unreadable or corrupt; "
                                      + "fix the file, or let the app back it up and start empty; "
                                      + backup))
        }

        if facts.bikeshedIsFile {
            checks.append(Check(name: "bikeshed", ok: false,
                                detail: "\(facts.bikeshedPath) — a file, not a folder; pick a folder"))
        } else {
            checks.append(Check(name: "bikeshed", ok: true,
                                detail: "\(facts.bikeshedPath) — \(facts.bikeshedNoteCount) note(s)"))
        }

        switch facts.commentsState {
        case .absent:
            checks.append(Check(name: "comments", ok: true,
                                detail: "\(facts.commentsPath) — absent (created on first comment)"))
        case .readable:
            checks.append(Check(name: "comments", ok: true,
                                detail: facts.commentsPath))
        case .unreadable:
            checks.append(Check(name: "comments", ok: false,
                                detail: "\(facts.commentsPath) — present but unreadable; "
                                       + "a directory of comment logs is expected here"))
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

    /// The rolling backup's clause for the store check: presence and age, the recovery
    /// copy's one line of context (ADR 0018). Informational — a store written once, or
    /// one nobody may write, is not a failure.
    static func backupDetail(_ health: Store.BackupHealth) -> String {
        switch health {
        case .absent: return "no backup yet"
        case .present(let age): return "backup \(ageText(age)) old"
        }
    }

    /// An age in the shortest unit that still reads as one: seconds, minutes, hours,
    /// days. Coarse on purpose — the question is "is there a recent copy?", not "when?".
    static func ageText(_ age: TimeInterval) -> String {
        let seconds = max(0, Int(age.rounded()))
        if seconds < 60 { return "\(seconds)s" }
        if seconds < 3_600 { return "\(seconds / 60)m" }
        if seconds < 86_400 { return "\(seconds / 3_600)h" }
        return "\(seconds / 86_400)d"
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
        let commentsPath = store.commentsDirectory.path
        let commentsState = commentsState(at: commentsPath)

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
            storeBackup: store.backupHealth(),
            storeLockHeld: store.lockIsHeld(),
            bikeshedPath: bikeshed.directory.path,
            bikeshedNoteCount: noteCount(notes),
            bikeshedIsFile: bikeshedIsFile,
            commentsPath: commentsPath,
            commentsState: commentsState,
            appPresent: appPresent,
            appVersion: appVersion,
            appQuarantined: appPresent && hasQuarantine(appPath),
            skillHosts: skillHosts(),
            opencodeHasSkills: opencodeHasSkills())
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
        let opencode = facts.opencodeHasSkills
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

    /// Whether the agent host has Tandem's skills, by either route: the
    /// `skills` array in `~/.config/opencode/opencode.json` listing a path
    /// containing `tandem/skills` (this repo's skills directory), or the host's
    /// own skills directory holding the linked `tandem` skill — what
    /// `Scripts/install-agent-host.sh` makes.
    private static func opencodeHasSkills() -> Bool {
        let home = NSHomeDirectory()
        let config = URL(fileURLWithPath: "\(home)/.config/opencode/opencode.json")
        if
            let data = try? Data(contentsOf: config),
            let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let skills = object["skills"] as? [String],
            skills.contains(where: { $0.contains("tandem/skills") })
        { return true }
        let linked = URL(fileURLWithPath: "\(home)/.config/opencode/skills/tandem")
        return FileManager.default.fileExists(atPath: linked.path)
    }

    // MARK: - Filesystem helpers

    /// The comments directory's state (see `CommentsState`). Absent is the
    /// normal early state — the first comment creates it — so only
    /// present-but-unusable counts: a file in its place, or a directory whose
    /// contents cannot be read.
    static func commentsState(at path: String) -> CommentsState {
        var isDir: ObjCBool = false
        guard FileManager.default.fileExists(atPath: path, isDirectory: &isDir) else {
            return .absent
        }
        if !isDir.boolValue { return .unreadable }
        return (try? FileManager.default.contentsOfDirectory(atPath: path)) != nil
            ? .readable
            : .unreadable
    }

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
