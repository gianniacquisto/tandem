import Foundation
import Testing
@testable import TandemCore

/// The agent face's install, uninstall and state (ADR 0034): one bundled copy in
/// the Tandem home, host links into it, a manifest that records every path, and
/// the warning model that `setup status` and `doctor` both read.
///
/// Every test drives a fixture bundle directory instead of a real `.app`, so no
/// build and no `/Applications` is involved: `TANDEM_AGENT_BUNDLE` is the seam
/// the product already has (`TANDEM_HOME`'s shape), and the environment is
/// injected so `~/.claude` and `~/.agents` are throwaway.
@Suite("Setup install")
struct SetupInstallTests {

    // MARK: - Fixture

    struct Fixture {
        let root: URL
        let home: URL          // $HOME
        let tandemHome: URL    // <TandemHome>
        let bundle: URL
        let skills: [String]
        let prompts: [String]
        let environment: [String: String]

        var face: URL { tandemHome.appendingPathComponent("agent") }
        var manifest: URL { tandemHome.appendingPathComponent("agent-install.json") }
        var opencodeConfig: URL {
            home.appendingPathComponent(".config/opencode/opencode.json")
        }
    }

    private func fixture(
        skills: [String] = ["alpha", "beta"],
        prompts: [String] = ["builder"],
        version: String = "1.0"
    ) throws -> Fixture {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-setup-tests-\(UUID().uuidString)")
        let home = root.appendingPathComponent("home")
        let tandemHome = home.appendingPathComponent(".tandem")
        let bundle = root.appendingPathComponent("bundle")
        try FileManager.default.createDirectory(at: home, withIntermediateDirectories: true)
        try writeBundle(at: bundle, skills: skills, prompts: prompts, version: version)
        return Fixture(
            root: root, home: home, tandemHome: tandemHome, bundle: bundle,
            skills: skills, prompts: prompts,
            environment: [
                "HOME": home.path,
                "XDG_CONFIG_HOME": home.appendingPathComponent(".config").path,
                "TANDEM_HOME": tandemHome.path,
            ])
    }

    private func writeBundle(
        at bundle: URL, skills: [String], prompts: [String], version: String
    ) throws {
        let fm = FileManager.default
        try fm.createDirectory(at: bundle.appendingPathComponent("skills"),
                               withIntermediateDirectories: true)
        try fm.createDirectory(at: bundle.appendingPathComponent("agents"),
                               withIntermediateDirectories: true)
        for skill in skills {
            let dir = bundle.appendingPathComponent("skills/\(skill)")
            try fm.createDirectory(at: dir, withIntermediateDirectories: true)
            try "\(skill)\n".write(to: dir.appendingPathComponent("SKILL.md"),
                                   atomically: true, encoding: .utf8)
        }
        for prompt in prompts {
            try "\(prompt)\n".write(to: bundle.appendingPathComponent("agents/\(prompt).md"),
                                    atomically: true, encoding: .utf8)
        }
        try "the agent face\n".write(to: bundle.appendingPathComponent("AGENTS.md"),
                                     atomically: true, encoding: .utf8)
        try version.write(to: bundle.appendingPathComponent("VERSION"),
                          atomically: true, encoding: .utf8)
    }

    private func install(
        _ f: Fixture,
        hosts: [SetupInstall.Host] = [.opencode],
        force: Bool = false,
        bundle: URL? = nil
    ) throws -> SetupInstall.Report {
        try SetupInstall.install(
            home: f.tandemHome, environment: f.environment,
            bundle: bundle ?? f.bundle, force: force, hosts: hosts)
    }

    private func state(
        _ f: Fixture, bundle: URL? = nil,
        executablePath: String = "/tmp/tandem-cli", isDebugBuild: Bool = true
    ) -> SetupInstall.State {
        SetupInstall.state(
            home: f.tandemHome, environment: f.environment,
            bundle: bundle ?? f.bundle,
            executablePath: executablePath, isDebugBuild: isDebugBuild)
    }

    private func action(_ report: SetupInstall.Report, _ path: String) -> SetupInstall.PathAction? {
        report.paths.first { $0.path == path }
    }

    private func link(_ path: String, _ target: String) throws {
        try FileManager.default.createDirectory(
            at: URL(fileURLWithPath: path).deletingLastPathComponent(),
            withIntermediateDirectories: true)
        try FileManager.default.createSymbolicLink(atPath: path, withDestinationPath: target)
    }

    private func target(_ path: String) -> String? {
        try? FileManager.default.destinationOfSymbolicLink(atPath: path)
    }

    private func isSymlink(_ path: String) -> Bool {
        (try? FileManager.default.attributesOfItem(atPath: path))?[.type] as? FileAttributeType
            == .typeSymbolicLink
    }

    /// Every non-directory entry under `root`, as a sorted list of
    /// `path → contents-or-target`. Empty directories are not state.
    private func tree(_ root: URL) throws -> [String: String] {
        let fm = FileManager.default
        guard let walker = fm.enumerator(atPath: root.path) else { return [:] }
        var entries: [String: String] = [:]
        for case let relative as String in walker {
            if relative.split(separator: "/").contains(where: { $0.hasPrefix("agent.new-") }) {
                continue
            }
            let path = root.appendingPathComponent(relative).path
            var isDir: ObjCBool = false
            guard fm.fileExists(atPath: path, isDirectory: &isDir) else { continue }
            if isDir.boolValue { continue }
            if let target = target(path) {
                entries[relative] = "-> \(target)"
            } else {
                entries[relative] = (try? String(contentsOfFile: path, encoding: .utf8)) ?? "<data>"
            }
        }
        return entries
    }

    // MARK: - Install

    @Test("install writes the bundled face into the Tandem home and links every skill and prompt into the host")
    func installWiresTheHost() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }

        let report = try install(f)

        #expect(FileManager.default.fileExists(
            atPath: f.face.appendingPathComponent("AGENTS.md").path))
        #expect(try String(contentsOf: f.face.appendingPathComponent("VERSION"), encoding: .utf8)
            == "1.0")
        for skill in f.skills {
            let path = f.home.appendingPathComponent(
                ".config/opencode/skills/\(skill)").path
            #expect(isSymlink(path), "\(skill) is linked")
            #expect(target(path) == f.face.appendingPathComponent("skills/\(skill)").path)
        }
        for prompt in f.prompts {
            let path = f.home.appendingPathComponent(
                ".config/opencode/agents/\(prompt).md").path
            #expect(isSymlink(path), "\(prompt) is linked")
            #expect(target(path) == f.face.appendingPathComponent("agents/\(prompt).md").path)
        }
        let config = try String(contentsOf: f.opencodeConfig, encoding: .utf8)
        #expect(config.contains(f.face.appendingPathComponent("AGENTS.md").path))
        #expect(report.verb == "install")
        #expect(report.manifest == "used")
        #expect(!FileManager.default.fileExists(
            atPath: f.home.appendingPathComponent(".config/opencode/AGENTS.md").path),
            "install never writes the global AGENTS.md")
        #expect(!FileManager.default.fileExists(
            atPath: f.home.appendingPathComponent(".claude/CLAUDE.md").path),
            "opencode is the default and claude is not touched")
    }

    @Test("install is idempotent: a second run changes nothing and reports the same state")
    func installIsIdempotent() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }

        let first = try install(f)
        let before = try tree(f.root)
        let manifestBefore = try Data(contentsOf: f.manifest)

        let second = try install(f)
        #expect(try tree(f.root) == before)
        #expect(try Data(contentsOf: f.manifest) == manifestBefore)
        #expect(second.state == first.state)
        #expect(!second.paths.contains { $0.action == "linked" || $0.action == "replaced" },
                "a second install writes nothing")
    }

    @Test("install never overwrites a real file at a host path and reports it as a skip")
    func installSkipsARealFile() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }

        let path = f.home.appendingPathComponent(".config/opencode/skills/alpha").path
        try FileManager.default.createDirectory(
            at: URL(fileURLWithPath: path).deletingLastPathComponent(),
            withIntermediateDirectories: true)
        try "the human's own\n".write(toFile: path, atomically: true, encoding: .utf8)

        let report = try install(f, force: true)
        #expect(try String(contentsOfFile: path, encoding: .utf8) == "the human's own\n",
                "even --force leaves a real file alone")
        #expect(action(report, path)?.action == "skip")
        #expect(action(report, path)?.reason?.contains("a real file is there") == true)
    }

    @Test("install leaves a link that resolves into a checkout alone, and says so; --force replaces it")
    func installLeavesACheckoutLink() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }
        let checkout = f.root.appendingPathComponent("checkout/skills/alpha")
        try FileManager.default.createDirectory(at: checkout, withIntermediateDirectories: true)
        let path = f.home.appendingPathComponent(".config/opencode/skills/alpha").path
        try link(path, checkout.path)

        let report = try install(f)
        #expect(target(path) == checkout.path, "the checkout route survives")
        #expect(action(report, path)?.action == "skip")
        #expect(action(report, path)?.reason?.contains("points elsewhere") == true)

        let forced = try install(f, force: true)
        #expect(target(path) == f.face.appendingPathComponent("skills/alpha").path)
        #expect(action(forced, path)?.action == "replaced")
    }

    @Test("install replaces a dead link")
    func installReplacesADeadLink() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }
        let path = f.home.appendingPathComponent(".config/opencode/skills/alpha").path
        try link(path, f.root.appendingPathComponent("gone/skills/alpha").path)

        let report = try install(f)
        #expect(target(path) == f.face.appendingPathComponent("skills/alpha").path)
        #expect(action(report, path)?.action == "replaced")
    }

    @Test("install honours TANDEM_HOME and XDG_CONFIG_HOME")
    func installHonoursTheEnvironment() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }
        let elsewhere = f.root.appendingPathComponent("xdg")
        var environment = f.environment
        environment["XDG_CONFIG_HOME"] = elsewhere.path

        _ = try SetupInstall.install(
            home: f.tandemHome, environment: environment, bundle: f.bundle,
            force: false, hosts: [.opencode])

        #expect(isSymlink(elsewhere.appendingPathComponent("opencode/skills/alpha").path))
        #expect(!FileManager.default.fileExists(
            atPath: f.home.appendingPathComponent(".config/opencode/skills/alpha").path))
    }

    @Test("a new bundled version replaces the installed copy and the host links still resolve")
    func installReplacesAVersion() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }
        _ = try install(f)

        try writeBundle(at: f.bundle, skills: f.skills, prompts: f.prompts, version: "2.0")
        let report = try install(f)

        #expect(try String(contentsOf: f.face.appendingPathComponent("VERSION"), encoding: .utf8)
            == "2.0")
        #expect(report.state.installedVersion == "2.0")
        for skill in f.skills {
            let path = f.home.appendingPathComponent(
                ".config/opencode/skills/\(skill)").path
            #expect(FileManager.default.fileExists(atPath: path), "\(skill) still resolves")
        }
        #expect(try SetupInstall.manifest(home: f.tandemHome)?.version == "2.0")
    }

    @Test("the replace is atomic: a failed materialise leaves the previous copy and every link intact")
    func installFailsCleanly() throws {
        let f = try fixture()
        defer {
            try? FileManager.default.setAttributes(
                [.posixPermissions: 0o644],
                ofItemAtPath: f.bundle.appendingPathComponent("skills/alpha/SKILL.md").path)
            try? FileManager.default.removeItem(at: f.root)
        }
        _ = try install(f)
        let before = try tree(f.tandemHome)

        // A new version whose bundle cannot be read: the copy fails mid-way.
        try writeBundle(at: f.bundle, skills: f.skills, prompts: f.prompts, version: "2.0")
        let unreadable = f.bundle.appendingPathComponent("skills/alpha/SKILL.md").path
        try FileManager.default.setAttributes([.posixPermissions: 0o000], ofItemAtPath: unreadable)

        #expect(throws: (any Error).self) { try install(f) }
        #expect(try tree(f.tandemHome) == before, "nothing half-written, every link intact")
        let names = try FileManager.default.contentsOfDirectory(atPath: f.tandemHome.path)
        #expect(!names.contains { $0.hasPrefix("agent.new-") }, "the temporary tree is removed")
    }

    @Test("install fails cleanly, leaving nothing half-written, when the host directory is not writable")
    func installRefusesAnUnwritableHome() throws {
        let f = try fixture()
        defer {
            try? FileManager.default.setAttributes(
                [.posixPermissions: 0o755], ofItemAtPath: f.tandemHome.path)
            try? FileManager.default.removeItem(at: f.root)
        }
        try FileManager.default.createDirectory(at: f.tandemHome, withIntermediateDirectories: true)
        try FileManager.default.setAttributes([.posixPermissions: 0o555],
                                             ofItemAtPath: f.tandemHome.path)

        let report = try install(f)
        #expect(!FileManager.default.fileExists(atPath: f.face.path))
        #expect(!FileManager.default.fileExists(atPath: f.manifest.path))
        #expect(report.state.warnings.contains { $0.code == .unwritable && $0.severity == .error })
    }

    @Test("claude and agents are written only when named; opencode is the default")
    func installHonoursTheHostList() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }

        _ = try install(f)
        #expect(!FileManager.default.fileExists(atPath: f.home.appendingPathComponent(".claude").path))
        #expect(!FileManager.default.fileExists(atPath: f.home.appendingPathComponent(".agents").path))

        let all = try install(f, hosts: [.opencode, .claude, .agents])
        #expect(isSymlink(f.home.appendingPathComponent(".claude/skills/alpha").path))
        #expect(isSymlink(f.home.appendingPathComponent(".agents/skills/alpha").path))
        #expect(FileManager.default.fileExists(
            atPath: f.home.appendingPathComponent(".claude/CLAUDE.md").path))
        #expect(all.manifest == "used")
        #expect(Set(try SetupInstall.manifest(home: f.tandemHome)?.hosts ?? [])
            == ["opencode", "claude", "agents"])
    }

    // MARK: - The resolver

    @Test("the bundle resolves from TANDEM_AGENT_BUNDLE, from an .app path, and to none for a dev build")
    func bundleResolution() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }

        #expect(SetupInstall.bundle(
            environment: ["TANDEM_AGENT_BUNDLE": f.bundle.path],
            executablePath: "/tmp/tandem-cli", isDebugBuild: true)?
            .resolvingSymlinksInPath().path == f.bundle.resolvingSymlinksInPath().path)

        let app = f.root.appendingPathComponent("Tandem.app")
        let face = app.appendingPathComponent("Contents/Resources/agent")
        try FileManager.default.createDirectory(at: face, withIntermediateDirectories: true)
        let executable = app.appendingPathComponent("Contents/MacOS/tandem-cli").path
        #expect(SetupInstall.bundle(
            environment: [:], executablePath: executable, isDebugBuild: false)?
            .resolvingSymlinksInPath().path == face.resolvingSymlinksInPath().path)
        #expect(SetupInstall.bundle(
            environment: [:], executablePath: executable, isDebugBuild: true) == nil,
            "a dev build cannot invent a bundle")
        #expect(SetupInstall.bundle(
            environment: [:], executablePath: "/tmp/build/tandem-cli", isDebugBuild: false) == nil)
    }

    // MARK: - Status and its states

    @Test("status reports wired-to-the-bundle, wired-to-a-checkout, a real file and absent")
    func statusReportsEachState() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }

        let empty = state(f)
        #expect(empty.hosts.allSatisfy { $0.state == "absent" })

        _ = try install(f, hosts: [.opencode, .claude, .agents])
        let checkout = f.root.appendingPathComponent("checkout/skills/alpha")
        try FileManager.default.createDirectory(at: checkout, withIntermediateDirectories: true)
        let claude = f.home.appendingPathComponent(".claude/skills/alpha").path
        try FileManager.default.removeItem(atPath: claude)
        try link(claude, checkout.path)
        let agents = f.home.appendingPathComponent(".agents/skills/beta").path
        try FileManager.default.removeItem(atPath: agents)
        try "the human's own\n".write(toFile: agents, atomically: true, encoding: .utf8)

        let wired = state(f)
        #expect(wired.hosts.first { $0.host == "opencode" }?.state == "installed")
        #expect(wired.hosts.first { $0.host == "claude" }?.state == "checkout")
        #expect(wired.hosts.first { $0.host == "agents" }?.state == "real-file")
        #expect(wired.installedVersion == "1.0")
        #expect(wired.bundledVersion == "1.0")
    }

    // MARK: - The manifest

    @Test("install records every link and edit in the manifest, and the manifest survives the wholesale replace")
    func manifestRecordsEverything() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }
        _ = try install(f)

        let manifest = try #require(try SetupInstall.manifest(home: f.tandemHome))
        #expect(manifest.version == "1.0")
        #expect(manifest.hosts == ["opencode"])
        #expect(manifest.links.count == f.skills.count + f.prompts.count)
        #expect(manifest.links.allSatisfy { $0.kind == "symlink" })
        #expect(manifest.links.contains {
            $0.path == f.home.appendingPathComponent(".config/opencode/skills/alpha").path
        })
        #expect(manifest.edits.contains { $0.kind == "instructions-entry" })

        try writeBundle(at: f.bundle, skills: f.skills, prompts: f.prompts, version: "2.0")
        _ = try install(f)
        let after = try #require(try SetupInstall.manifest(home: f.tandemHome))
        #expect(after.version == "2.0")
        #expect(after.links == manifest.links, "a replace does not touch the links")
        #expect(after.edits == manifest.edits)
    }

    @Test("install merges the manifest across two runs: --host opencode then --host claude keeps both records")
    func manifestMerges() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }
        _ = try install(f, hosts: [.opencode])
        _ = try install(f, hosts: [.claude])

        let manifest = try #require(try SetupInstall.manifest(home: f.tandemHome))
        #expect(manifest.hosts == ["opencode", "claude"])
        #expect(manifest.links.contains {
            $0.path == f.home.appendingPathComponent(".config/opencode/skills/alpha").path
        })
        #expect(manifest.links.contains {
            $0.path == f.home.appendingPathComponent(".claude/skills/alpha").path
        })
        #expect(manifest.edits.contains { $0.kind == "instructions-entry" })
        #expect(manifest.edits.contains { $0.kind == "claude-block" })
    }

    // MARK: - Uninstall

    @Test("uninstall removes exactly what install wrote and nothing else")
    func uninstallRemovesWhatInstallWrote() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }
        let before = try tree(f.root)
        _ = try install(f, hosts: [.opencode, .claude, .agents])

        let report = try SetupInstall.uninstall(
            home: f.tandemHome, environment: f.environment, hosts: nil)

        #expect(report.verb == "uninstall")
        #expect(report.manifest == "used")
        #expect(try tree(f.root) == before, "the home returns to its pre-install state")
        #expect(!FileManager.default.fileExists(atPath: f.face.path))
        #expect(!FileManager.default.fileExists(atPath: f.manifest.path))
    }

    @Test("uninstall leaves a host link repointed by hand, and says so")
    func uninstallLeavesARepointedLink() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }
        _ = try install(f)
        let path = f.home.appendingPathComponent(".config/opencode/skills/alpha").path
        let checkout = f.root.appendingPathComponent("checkout/skills/alpha")
        try FileManager.default.createDirectory(at: checkout, withIntermediateDirectories: true)
        try FileManager.default.removeItem(atPath: path)
        try link(path, checkout.path)

        let report = try SetupInstall.uninstall(
            home: f.tandemHome, environment: f.environment, hosts: nil)

        #expect(FileManager.default.fileExists(atPath: path))
        #expect(action(report, path)?.action == "left")
        #expect(action(report, path)?.reason?.contains("points elsewhere") == true)
    }

    @Test("uninstall with no manifest removes the installed face and reports the links as unattributed")
    func uninstallWithoutAManifest() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }
        _ = try install(f)
        let linkPath = f.home.appendingPathComponent(".config/opencode/skills/alpha").path
        try FileManager.default.removeItem(at: f.manifest)

        let report = try SetupInstall.uninstall(
            home: f.tandemHome, environment: f.environment, hosts: nil)

        #expect(report.manifest == "absent")
        #expect(report.unattributed == true)
        #expect(!FileManager.default.fileExists(atPath: f.face.path))
        #expect(isSymlink(linkPath), "an unattributed link is left, and the report says so")
    }

    @Test("install, uninstall, install returns to the same state")
    func installUninstallInstall() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }
        _ = try install(f, hosts: [.opencode, .claude])
        let installed = try tree(f.root)
        let manifest = try Data(contentsOf: f.manifest)

        _ = try SetupInstall.uninstall(home: f.tandemHome, environment: f.environment, hosts: nil)
        _ = try install(f, hosts: [.opencode, .claude])

        #expect(try tree(f.root) == installed)
        #expect(try Data(contentsOf: f.manifest) == manifest)
    }

    @Test("a second uninstall is a clean no-op and exits 0")
    func secondUninstallIsANoOp() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }
        _ = try install(f)
        _ = try SetupInstall.uninstall(home: f.tandemHome, environment: f.environment, hosts: nil)

        let again = try SetupInstall.uninstall(
            home: f.tandemHome, environment: f.environment, hosts: nil)
        #expect(again.manifest == "absent")
        #expect(again.paths.allSatisfy { $0.action == "absent" }, "nothing left to remove")
    }

    // MARK: - The OpenCode config edit

    @Test("the opencode config merge is idempotent and preserves a pre-existing instructions array and unrelated keys")
    func configMergePreservesTheHumanConfig() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }
        try FileManager.default.createDirectory(
            at: f.opencodeConfig.deletingLastPathComponent(), withIntermediateDirectories: true)
        let original = """
        {
          "$schema": "https://opencode.ai/config.json",
          "instructions": ["/somewhere/AGENTS.md"],
          "mcp": {}
        }

        """
        try original.write(to: f.opencodeConfig, atomically: true, encoding: .utf8)

        _ = try install(f)
        let merged = try String(contentsOf: f.opencodeConfig, encoding: .utf8)
        #expect(merged.contains("/somewhere/AGENTS.md"), "the human's entry survives")
        #expect(merged.contains(f.face.appendingPathComponent("AGENTS.md").path))
        #expect(merged.contains("$schema"))
        #expect(merged.contains("\"mcp\""))

        _ = try install(f)
        #expect(try String(contentsOf: f.opencodeConfig, encoding: .utf8) == merged,
                "a second install does not append a second entry")
    }

    @Test("an unparseable opencode.json is a skip with the reason, and the skills are still linked")
    func unparseableConfigIsASkip() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }
        try FileManager.default.createDirectory(
            at: f.opencodeConfig.deletingLastPathComponent(), withIntermediateDirectories: true)
        try "not json at all {".write(to: f.opencodeConfig, atomically: true, encoding: .utf8)

        let report = try install(f)
        #expect(action(report, f.opencodeConfig.path)?.action == "skip")
        #expect(action(report, f.opencodeConfig.path)?.reason?.isEmpty == false)
        #expect(isSymlink(f.home.appendingPathComponent(
            ".config/opencode/skills/alpha").path), "the skills are still linked")
    }

    @Test("a config with trailing commas is read, and the instructions entry is inserted textually and removed byte-exactly")
    func jsoncConfigIsRead() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }
        try FileManager.default.createDirectory(
            at: f.opencodeConfig.deletingLastPathComponent(), withIntermediateDirectories: true)
        let original = """
        {
          // the provider list
          "providers": {
            "gianni-desktop": { "models": ["medium",], },
          },
        }

        """
        try original.write(to: f.opencodeConfig, atomically: true, encoding: .utf8)

        _ = try install(f)
        let merged = try String(contentsOf: f.opencodeConfig, encoding: .utf8)
        #expect(merged.contains(f.face.appendingPathComponent("AGENTS.md").path))
        #expect(merged.contains("// the provider list"), "the comment survives a textual insert")

        _ = try SetupInstall.uninstall(home: f.tandemHome, environment: f.environment, hosts: nil)
        #expect(try String(contentsOf: f.opencodeConfig, encoding: .utf8) == original,
                "removal is byte-exact")
    }

    @Test("an instructions array that already holds the entry is ok and the file is not rewritten")
    func existingEntryIsOK() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }
        let entry = f.face.appendingPathComponent("AGENTS.md").path
        try FileManager.default.createDirectory(
            at: f.opencodeConfig.deletingLastPathComponent(), withIntermediateDirectories: true)
        let original = "{\n  \"instructions\": [\"\(entry)\"]\n}\n"
        try original.write(to: f.opencodeConfig, atomically: true, encoding: .utf8)

        let report = try install(f)
        #expect(action(report, f.opencodeConfig.path)?.action == "ok")
        #expect(try String(contentsOf: f.opencodeConfig, encoding: .utf8) == original)
    }

    // MARK: - The CLAUDE.md block

    @Test("the CLAUDE.md block is created when the file is absent, appended when it has no markers, and replaced in place when it has them")
    func claudeBlock() throws {
        let absent = try fixture()
        defer { try? FileManager.default.removeItem(at: absent.root) }
        _ = try install(absent, hosts: [.claude])
        let claudeMD = absent.home.appendingPathComponent(".claude/CLAUDE.md")
        let created = try String(contentsOf: claudeMD, encoding: .utf8)
        #expect(created.contains("<!-- tandem:begin -->"))
        #expect(created.contains(absent.face.appendingPathComponent("AGENTS.md").path))
        #expect(created.contains("<!-- tandem:end -->"))

        let plain = try fixture()
        defer { try? FileManager.default.removeItem(at: plain.root) }
        let plainMD = plain.home.appendingPathComponent(".claude/CLAUDE.md")
        try FileManager.default.createDirectory(
            at: plainMD.deletingLastPathComponent(), withIntermediateDirectories: true)
        try "the human's own rules\n".write(to: plainMD, atomically: true, encoding: .utf8)
        _ = try install(plain, hosts: [.claude])
        let appended = try String(contentsOf: plainMD, encoding: .utf8)
        #expect(appended.hasPrefix("the human's own rules\n"))
        #expect(appended.contains("<!-- tandem:begin -->"))

        // A second install replaces the block in place: no second block, same bytes.
        _ = try install(plain, hosts: [.claude])
        #expect(try String(contentsOf: plainMD, encoding: .utf8) == appended)
        #expect(appended.components(separatedBy: "<!-- tandem:begin -->").count == 2)

        _ = try SetupInstall.uninstall(home: plain.tandemHome, environment: plain.environment,
                                       hosts: nil)
        #expect(try String(contentsOf: plainMD, encoding: .utf8) == "the human's own rules\n",
                "uninstall leaves the rest of the file byte-identical")
    }

    // MARK: - The warning model

    private func warnings(_ f: Fixture, bundle: URL?) -> [SetupInstall.Warning] {
        state(f, bundle: bundle).warnings
    }

    private func has(_ warnings: [SetupInstall.Warning], _ code: SetupInstall.Code) -> Bool {
        warnings.contains { $0.code == code }
    }

    @Test("no-bundle is info on a dev build and warn inside an .app whose Resources lack the face")
    func noBundleSeverity() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }

        let dev = SetupInstall.state(
            home: f.tandemHome, environment: f.environment, bundle: nil,
            executablePath: "/tmp/build/tandem-cli", isDebugBuild: true)
        #expect(dev.warnings.contains { $0.code == .noBundle && $0.severity == .info })

        let app = SetupInstall.state(
            home: f.tandemHome, environment: f.environment, bundle: nil,
            executablePath: "/Applications/Tandem.app/Contents/MacOS/tandem-cli",
            isDebugBuild: false)
        #expect(app.warnings.contains { $0.code == .noBundle && $0.severity == .warn })
    }

    @Test("an agent directory with no manifest warns no-manifest")
    func noManifest() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }
        try FileManager.default.createDirectory(at: f.face, withIntermediateDirectories: true)
        try "1.0".write(to: f.face.appendingPathComponent("VERSION"),
                        atomically: true, encoding: .utf8)
        #expect(has(warnings(f, bundle: nil), .noManifest))
    }

    @Test("an installed version that differs from the bundle warns stale-face")
    func staleFace() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }
        _ = try install(f)
        try writeBundle(at: f.bundle, skills: f.skills, prompts: f.prompts, version: "2.0")
        let stale = warnings(f, bundle: f.bundle).first { $0.code == .staleFace }
        #expect(stale != nil)
        #expect(stale?.detail.contains("1.0") == true)
        #expect(stale?.detail.contains("2.0") == true)
    }

    @Test("an agent directory without a VERSION marker warns missing-version")
    func missingVersion() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }
        try FileManager.default.createDirectory(
            at: f.face.appendingPathComponent("skills"), withIntermediateDirectories: true)
        let list = warnings(f, bundle: f.bundle)
        #expect(has(list, .missingVersion))
        #expect(!has(list, .staleFace), "nothing to compare without a marker")
        #expect(!has(list, .partialFace))
    }

    @Test("a face missing a skill the bundle carries, or carrying one it dropped, warns partial-face")
    func partialFace() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }
        _ = try install(f)
        try FileManager.default.removeItem(at: f.face.appendingPathComponent("skills/beta"))
        try FileManager.default.createDirectory(
            at: f.face.appendingPathComponent("skills/retired"), withIntermediateDirectories: true)
        let partial = warnings(f, bundle: f.bundle).first { $0.code == .partialFace }
        #expect(partial?.detail.contains("missing: skills/beta") == true)
        #expect(partial?.detail.contains("extra: skills/retired") == true)
    }

    @Test("a host symlink whose target is gone warns dead-link")
    func deadLink() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }
        _ = try install(f)
        let path = f.home.appendingPathComponent(".config/opencode/skills/alpha").path
        try FileManager.default.removeItem(atPath: path)
        try link(path, f.face.appendingPathComponent("skills/ghost").path)
        let dead = warnings(f, bundle: f.bundle).first { $0.code == .deadLink }
        #expect(dead?.path == path)
        #expect(dead?.detail.contains("(gone)") == true)
    }

    @Test("a link into the agent directory that the manifest does not record warns orphan-link")
    func orphanLink() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }
        _ = try install(f)
        try FileManager.default.removeItem(at: f.manifest)
        #expect(has(warnings(f, bundle: f.bundle), .orphanLink))
    }

    @Test("a checkout link is info, and warns as checkout-shadows-install beside a current install")
    func checkoutLinkWarnings() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }
        let checkout = f.root.appendingPathComponent("checkout/skills/alpha")
        try FileManager.default.createDirectory(at: checkout, withIntermediateDirectories: true)
        let path = f.home.appendingPathComponent(".config/opencode/skills/alpha").path
        try link(path, checkout.path)

        let bare = warnings(f, bundle: f.bundle)
        #expect(bare.contains { $0.code == .foreignLink && $0.severity == .info })

        _ = try install(f, force: true)
        try FileManager.default.removeItem(atPath: path)
        try link(path, checkout.path)
        let shadowed = warnings(f, bundle: f.bundle).first { $0.code == .checkoutShadowsInstall }
        #expect(shadowed?.severity == .warn)
        #expect(shadowed?.detail.contains("beside a current install") == true)
    }

    @Test("a real file at a host path warns blocked-path and names the path")
    func blockedPath() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }
        _ = try install(f)
        let path = f.home.appendingPathComponent(".config/opencode/agents/builder.md").path
        try FileManager.default.removeItem(atPath: path)
        try "mine\n".write(toFile: path, atomically: true, encoding: .utf8)
        let blocked = warnings(f, bundle: f.bundle).first { $0.code == .blockedPath }
        #expect(blocked?.path == path)
        #expect(blocked?.detail.contains("holds a real file") == true)
    }

    @Test("a global AGENTS.md beside the instructions entry warns double-load")
    func doubleLoad() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }
        _ = try install(f)
        let global = f.home.appendingPathComponent(".config/opencode/AGENTS.md")
        try "the old route\n".write(to: global, atomically: true, encoding: .utf8)
        let warning = warnings(f, bundle: f.bundle).first { $0.code == .doubleLoad }
        #expect(warning != nil)
        #expect(warning?.detail.contains("both load") == true)
    }

    @Test("a claude install with OPENCODE_DISABLE_CLAUDE_CODE set warns host-disabled, and an agents install with OPENCODE_DISABLE_EXTERNAL_SKILLS set does too")
    func hostDisabled() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }
        _ = try install(f, hosts: [.claude, .agents])

        #expect(has(warnings(f, bundle: f.bundle), .hostDisabled) == false,
                "no variable set, no warning")

        var environment = f.environment
        environment["OPENCODE_DISABLE_CLAUDE_CODE"] = "true"
        environment["OPENCODE_DISABLE_EXTERNAL_SKILLS"] = "1"
        let disabled = SetupInstall.state(
            home: f.tandemHome, environment: environment, bundle: f.bundle,
            executablePath: "/tmp/tandem-cli", isDebugBuild: true).warnings
        #expect(disabled.contains { $0.code == .hostDisabled && $0.detail.contains("claude") })
        #expect(disabled.contains { $0.code == .hostDisabled && $0.detail.contains("agents") })
    }

    @Test("a host with its links but no instruction file warns half-installed")
    func halfInstalled() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }
        _ = try install(f)
        try FileManager.default.removeItem(at: f.opencodeConfig)

        let warning = warnings(f, bundle: f.bundle).first { $0.code == .halfInstalled }
        #expect(warning != nil)
        #expect(warning?.detail.contains("no instruction file") == true)
        #expect(warning?.hint.contains("setup install --host opencode") == true)
    }

    @Test("a Tandem home that cannot be written is an error")
    func unwritableHome() throws {
        let f = try fixture()
        defer {
            try? FileManager.default.setAttributes(
                [.posixPermissions: 0o755], ofItemAtPath: f.tandemHome.path)
            try? FileManager.default.removeItem(at: f.root)
        }
        try FileManager.default.createDirectory(at: f.tandemHome, withIntermediateDirectories: true)
        try FileManager.default.setAttributes([.posixPermissions: 0o555],
                                             ofItemAtPath: f.tandemHome.path)
        let warning = warnings(f, bundle: f.bundle).first { $0.code == .unwritable }
        #expect(warning?.severity == .error)
        #expect(warning?.detail.contains("is not writable") == true)
    }

    @Test("the warning codes and their severities are pinned in one list")
    func codesArePinned() {
        #expect(SetupInstall.Code.allCases.map(\.rawValue) == [
            "no-bundle", "no-manifest", "stale-face", "missing-version", "partial-face",
            "dead-link", "orphan-link", "foreign-link", "checkout-shadows-install",
            "blocked-path", "double-load", "host-disabled", "half-installed", "unwritable",
        ])
        #expect(Set(SetupInstall.Severity.allCases.map(\.rawValue)) == ["error", "warn", "info"])
    }

    @Test("warnings sort by severity, then code, then path, and one path carries one code")
    func warningsAreOrdered() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }
        _ = try install(f)
        let alpha = f.home.appendingPathComponent(".config/opencode/skills/alpha").path
        let beta = f.home.appendingPathComponent(".config/opencode/skills/beta").path
        try FileManager.default.removeItem(atPath: alpha)
        try link(alpha, f.face.appendingPathComponent("skills/ghost").path)
        try FileManager.default.removeItem(atPath: beta)
        try FileManager.default.removeItem(at: f.opencodeConfig)

        let list = warnings(f, bundle: f.bundle)
        let order = list.map { "\($0.severity.rawValue)|\($0.code.rawValue)|\($0.path ?? "")" }
        #expect(order == order.sorted())
        #expect(Set(list.compactMap(\.path)).count == list.compactMap(\.path).count,
                "one path carries one code")
    }

    @Test("a dead link to a retired skill is one dead-link warning, not also an orphan")
    func deadLinkBeatsOrphan() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }
        _ = try install(f)
        let path = f.home.appendingPathComponent(".config/opencode/skills/alpha").path
        try FileManager.default.removeItem(atPath: path)
        try link(path, f.face.appendingPathComponent("skills/retired").path)
        try FileManager.default.removeItem(at: f.manifest)

        let list = warnings(f, bundle: f.bundle)
        #expect(list.filter { $0.path == path }.map(\.code) == [.deadLink])
    }

    @Test("status and install print the same warning list for the same fixture home")
    func statusAndInstallAgree() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }
        _ = try install(f)
        let path = f.home.appendingPathComponent(".config/opencode/skills/alpha").path
        try FileManager.default.removeItem(atPath: path)
        try link(path, f.root.appendingPathComponent("checkout/alpha").path)

        let installed = try install(f)
        #expect(installed.state.warnings == state(f).warnings)
    }
}
