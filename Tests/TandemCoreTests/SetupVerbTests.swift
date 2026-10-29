import Foundation
import Testing
@testable import TandemCore

/// The `setup` group through the built binary: one JSON report on stdout, the
/// exit codes the install report earns, and the failure that is a message on
/// stderr with no JSON at all (ADR 0034).
@Suite("Setup verbs")
struct SetupVerbTests {

    private let cli = CLIHarness()

    private struct Fixture {
        let root: URL
        let home: URL
        let bundle: URL
        let environment: [String: String?]
    }

    private func fixture(version: String = "1.0") throws -> Fixture {
        let fm = FileManager.default
        let root = fm.temporaryDirectory
            .appendingPathComponent("tandem-setup-verb-tests-\(UUID().uuidString)")
        let home = root.appendingPathComponent("home")
        let bundle = root.appendingPathComponent("bundle")
        try fm.createDirectory(at: bundle.appendingPathComponent("skills/alpha"),
                               withIntermediateDirectories: true)
        try fm.createDirectory(at: bundle.appendingPathComponent("agents"),
                               withIntermediateDirectories: true)
        try "alpha\n".write(to: bundle.appendingPathComponent("skills/alpha/SKILL.md"),
                            atomically: true, encoding: .utf8)
        try "builder\n".write(to: bundle.appendingPathComponent("agents/builder.md"),
                              atomically: true, encoding: .utf8)
        try "the agent face\n".write(to: bundle.appendingPathComponent("AGENTS.md"),
                                     atomically: true, encoding: .utf8)
        try version.write(to: bundle.appendingPathComponent("VERSION"),
                          atomically: true, encoding: .utf8)
        try fm.createDirectory(at: home, withIntermediateDirectories: true)
        return Fixture(root: root, home: home, bundle: bundle, environment: [
            "HOME": home.path,
            "XDG_CONFIG_HOME": home.appendingPathComponent(".config").path,
            "TANDEM_AGENT_BUNDLE": bundle.path,
        ])
    }

    @Test("setup install on a throwaway home prints one JSON report and exits 0")
    func installPrintsAReport() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }

        let result = try cli.run(home: f.home, ["setup", "install"], environment: f.environment)
        #expect(result.code == 0)
        #expect(result.error == "")
        let object = try cli.object(from: result.output)
        #expect(object["verb"] as? String == "install")
        #expect(object["manifest"] as? String == "used")
        let state = try #require(object["state"] as? [String: Any])
        #expect(state["installedVersion"] as? String == "1.0")
        #expect(state["bundledVersion"] as? String == "1.0")
        let paths = try #require(object["paths"] as? [[String: Any]])
        #expect(paths.contains { $0["action"] as? String == "linked" })
    }

    @Test("setup status exits 0 and names each host")
    func statusNamesEveryHost() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }

        let result = try cli.run(home: f.home, ["setup", "status"], environment: f.environment)
        #expect(result.code == 0)
        #expect(result.error == "")
        let object = try cli.object(from: result.output)
        #expect(object["verb"] as? String == "status")
        #expect(object["bundled"] as? String == nil || object["bundled"] is NSNull,
                "status carries no bundled path key")
        let state = try #require(object["state"] as? [String: Any])
        let hosts = try #require(state["hosts"] as? [[String: Any]])
        #expect(Set(hosts.compactMap { $0["host"] as? String }) == ["opencode", "claude", "agents"])
    }

    @Test("setup uninstall prints one JSON report and exits 0")
    func uninstallPrintsAReport() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }
        _ = try cli.run(home: f.home, ["setup", "install"], environment: f.environment)

        let result = try cli.run(home: f.home, ["setup", "uninstall"], environment: f.environment)
        #expect(result.code == 0)
        let object = try cli.object(from: result.output)
        #expect(object["verb"] as? String == "uninstall")
        #expect(object["manifest"] as? String == "used")
    }

    @Test("setup uninstall on a home with nothing installed exits 0 and reports absent")
    func uninstallWithNothingInstalled() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }

        let result = try cli.run(home: f.home, ["setup", "uninstall"], environment: f.environment)
        #expect(result.code == 0)
        let object = try cli.object(from: result.output)
        #expect(object["manifest"] as? String == "absent")
    }

    @Test("setup install with no bundle exits 1 with the message on stderr and no JSON on stdout")
    func installWithoutABundleFails() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }
        var environment = f.environment
        environment["TANDEM_AGENT_BUNDLE"] = nil

        let result = try cli.run(home: f.home, ["setup", "install"], environment: environment)
        #expect(result.code == 1)
        #expect(result.output == "", "no JSON on stdout")
        #expect(result.error.contains("no bundled agent face"))
    }

    @Test("an unknown --host is a domain failure, exit 1")
    func unknownHostFails() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }

        let result = try cli.run(home: f.home, ["setup", "install", "--host", "banana"],
                                 environment: f.environment)
        #expect(result.code == 1)
        #expect(result.output == "")
        #expect(result.error.contains("unknown host"))
    }

    @Test("setup status prints the warnings and exits 0")
    func statusPrintsWarnings() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }
        _ = try cli.run(home: f.home, ["setup", "install"], environment: f.environment)
        // A dead link: the path is one the bundle ships and the symptom is named.
        let path = f.home.appendingPathComponent(".config/opencode/skills/alpha").path
        try FileManager.default.removeItem(atPath: path)
        try FileManager.default.createSymbolicLink(
            atPath: path,
            withDestinationPath: f.home.appendingPathComponent(".tandem/agent/skills/ghost").path)

        let result = try cli.run(home: f.home, ["setup", "status"], environment: f.environment)
        #expect(result.code == 0, "status exits 0 even with warnings")
        let object = try cli.object(from: result.output)
        let state = try #require(object["state"] as? [String: Any])
        let warnings = try #require(state["warnings"] as? [[String: Any]])
        #expect(warnings.contains { $0["code"] as? String == "dead-link" })
        #expect(warnings.allSatisfy { $0["hint"] as? String != nil })
    }

    @Test("setup install prints the same warnings as status and exits 0 when they are warnings")
    func installPrintsTheSameWarnings() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }
        let installed = try cli.run(home: f.home, ["setup", "install"], environment: f.environment)
        let path = f.home.appendingPathComponent(".config/opencode/skills/alpha").path
        try FileManager.default.removeItem(atPath: path)
        try FileManager.default.createSymbolicLink(
            atPath: path,
            withDestinationPath: f.home.appendingPathComponent(".tandem/agent/skills/ghost").path)

        let again = try cli.run(home: f.home, ["setup", "install"], environment: f.environment)
        let status = try cli.run(home: f.home, ["setup", "status"], environment: f.environment)
        #expect(installed.code == 0)
        #expect(again.code == 0, "a warning is a skip, not a failure")
        let installedState = try #require(
            try cli.object(from: again.output)["state"] as? [String: Any])
        let statusState = try #require(
            try cli.object(from: status.output)["state"] as? [String: Any])
        #expect(installedState["warnings"] as? [[String: Any]] != nil)
        let a = try JSONSerialization.data(withJSONObject: installedState["warnings"] ?? [],
                                           options: [.sortedKeys])
        let b = try JSONSerialization.data(withJSONObject: statusState["warnings"] ?? [],
                                           options: [.sortedKeys])
        #expect(a == b, "one producer, the same list")
    }

    @Test("setup install exits 1 and still prints the report when a warning is an error")
    func installPrintsItsErrorReport() throws {
        let f = try fixture()
        defer {
            try? FileManager.default.setAttributes(
                [.posixPermissions: 0o755],
                ofItemAtPath: f.home.appendingPathComponent(".tandem").path)
            try? FileManager.default.removeItem(at: f.root)
        }
        let home = f.home.appendingPathComponent(".tandem")
        try FileManager.default.createDirectory(at: home, withIntermediateDirectories: true)
        try FileManager.default.setAttributes([.posixPermissions: 0o555], ofItemAtPath: home.path)

        let result = try cli.run(home: home, ["setup", "install"], environment: f.environment)
        #expect(result.code == 1)
        let object = try cli.object(from: result.output)
        let state = try #require(object["state"] as? [String: Any])
        let warnings = try #require(state["warnings"] as? [[String: Any]])
        #expect(warnings.contains {
            $0["code"] as? String == "unwritable" && $0["severity"] as? String == "error"
        })
    }

    @Test("setup --help and each verb's --help print their own grammar and exit 0")
    func setupHelp() throws {
        let f = try fixture()
        defer { try? FileManager.default.removeItem(at: f.root) }

        let group = try cli.run(home: f.home, ["setup", "--help"], environment: f.environment)
        #expect(group.code == 0)
        #expect(group.output.contains("tandem-cli setup install"))
        #expect(group.output.contains("tandem-cli setup uninstall"))
        #expect(group.output.contains("tandem-cli setup status"))

        for verb in ["install", "uninstall", "status"] {
            let result = try cli.run(home: f.home, ["setup", verb, "--help"],
                                     environment: f.environment)
            #expect(result.code == 0, "setup \(verb) --help")
            #expect(result.error == "", "setup \(verb) --help")
            #expect(result.output.contains("tandem-cli setup \(verb)"), "setup \(verb)")
        }
    }
}
