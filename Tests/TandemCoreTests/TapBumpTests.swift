import Foundation
import Testing

/// The tap bump (ADR 0036): the tag run's `release` job rewrites the tap's
/// `Casks/tandem.rb` from `dist/SHA256SUMS` and pushes it, so no release waits
/// for a human commit. The suite runs `Scripts/bump-tap.sh` against throwaway
/// fixture taps (a temp clone and a local bare repo as `origin`), the way
/// `ReleaseNotesTests` runs its script, so no test touches the network or brew.
/// `--verify` is the runner's gate and needs brew; it is not exercised here.
@Suite("Tap bump")
struct TapBumpTests {

    private static let root = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent() // Tests/TandemCoreTests
        .deletingLastPathComponent() // Tests
        .deletingLastPathComponent() // repo root

    private struct FixtureError: Error { let message: String }

    private struct Run {
        let output: String
        let errors: String
        let status: Int32
    }

    /// The tap's cask at 0.34.0, the real file at the tap's head `8d9328e`
    /// when this suite was written: the worked example every bump starts from.
    private static let cask034 = """
    cask "tandem" do
      version "0.34.0"
      sha256 "b30ddae202193c06a005733848d38a2da3393c7d30aa33261c43b6d31ebfa7ec"

      url "https://github.com/gianniacquisto/tandem/releases/download/v#{version}/Tandem-#{version}.zip"
      name "Tandem"
      desc "Todo app shared by humans and agents"
      homepage "https://github.com/gianniacquisto/tandem"

      livecheck do
        url "https://github.com/gianniacquisto/tandem/releases/latest"
        strategy :github_latest
      end

      depends_on macos: :golden_gate

      # The agent-facing CLI the app bundles. The skills and the agent prompts call
      # it by name, so it has to be on PATH for the workflow to work.
      app "Tandem.app"
      binary "#{appdir}/Tandem.app/Contents/MacOS/tandem-cli"
    end
    """ + "\n"

    /// The digest `dist/SHA256SUMS` carries for a made-up 0.35.0 artifact.
    private static let digest035 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

    /// The same cask after the bump: version and sha256 changed, every other
    /// byte, the tap-prefixed `url`, `livecheck`, `depends_on`, the `binary`
    /// line, untouched.
    private static let cask035 = """
    cask "tandem" do
      version "0.35.0"
      sha256 "\(digest035)"

      url "https://github.com/gianniacquisto/tandem/releases/download/v#{version}/Tandem-#{version}.zip"
      name "Tandem"
      desc "Todo app shared by humans and agents"
      homepage "https://github.com/gianniacquisto/tandem"

      livecheck do
        url "https://github.com/gianniacquisto/tandem/releases/latest"
        strategy :github_latest
      end

      depends_on macos: :golden_gate

      # The agent-facing CLI the app bundles. The skills and the agent prompts call
      # it by name, so it has to be on PATH for the workflow to work.
      app "Tandem.app"
      binary "#{appdir}/Tandem.app/Contents/MacOS/tandem-cli"
    end
    """ + "\n"

    // MARK: fixture tap

    /// A throwaway tap: `origin` is a local bare repo, `work` a clone whose
    /// cask is `cask`, the way `brew tap` leaves a developer machine's tap.
    private static func makeTap(cask: String) throws -> (work: URL, origin: URL) {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("tap-bump-\(UUID().uuidString)")
        let origin = root.appendingPathComponent("homebrew-tap.git")
        let work = root.appendingPathComponent("homebrew-tap")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try git(root, "init", "--bare", "-b", "main", origin.path)
        try git(root, "init", "-b", "main", work.path)
        try git(work, "config", "user.name", "Test")
        try git(work, "config", "user.email", "test@example.com")
        try write(cask, to: work.appendingPathComponent("Casks/tandem.rb"))
        try git(work, "add", ".")
        try git(work, "commit", "-m", "tandem 0.34.0", "--quiet")
        try git(work, "remote", "add", "origin", origin.path)
        try git(work, "push", "--quiet", "-u", "origin", "main")
        return (work, origin)
    }

    private static func write(_ text: String, to url: URL) throws {
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try Data(text.utf8).write(to: url)
    }

    /// A `SHA256SUMS` file, `shasum -a 256`'s shape: `<digest>  <name>`.
    private static func checksums(_ entries: [(digest: String, name: String)]) throws -> URL {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("tap-bump-sums-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let url = dir.appendingPathComponent("SHA256SUMS")
        try write(entries.map { "\($0.digest)  \($0.name)\n" }.joined(), to: url)
        return url
    }

    private static func git(_ dir: URL, _ args: String...) throws {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/git")
        process.arguments = args
        process.currentDirectoryURL = dir
        let errors = Pipe()
        process.standardError = errors
        try process.run()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else {
            let message = String(
                data: errors.fileHandleForReading.readDataToEndOfFile(),
                encoding: .utf8
            ) ?? ""
            throw FixtureError(message: "git \(args.joined(separator: " ")) failed: \(message)")
        }
    }

    /// `git log`'s answer for one format string on `main`.
    private static func log(_ dir: URL, _ format: String) throws -> String {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/git")
        process.arguments = ["log", "-1", "--format=\(format)", "main"]
        process.currentDirectoryURL = dir
        let output = Pipe()
        process.standardOutput = output
        try process.run()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else {
            throw FixtureError(message: "git log failed in \(dir.path)")
        }
        let text = String(data: output.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        return text.trimmingCharacters(in: .newlines)
    }

    /// `git rev-list --count main`, the fixture origin's history length.
    private static func commitCount(_ origin: URL) throws -> String {
        try capture(origin, ["rev-list", "--count", "main"])
    }

    /// `git -C <dir> <args>` stdout, trimmed.
    private static func capture(_ dir: URL, _ arguments: [String]) throws -> String {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/git")
        process.arguments = arguments
        process.currentDirectoryURL = dir
        let output = Pipe()
        process.standardOutput = output
        try process.run()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else {
            throw FixtureError(message: "git \(arguments.joined(separator: " ")) failed in \(dir.path)")
        }
        let text = String(data: output.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        return text.trimmingCharacters(in: .newlines)
    }

    private static func run(
        _ arguments: [String],
        token: String? = nil,
        environment extra: [String: String] = [:]
    ) throws -> Run {
        let process = Process()
        process.executableURL = Self.root.appendingPathComponent("Scripts/bump-tap.sh")
        process.currentDirectoryURL = Self.root
        process.arguments = arguments
        var environment = ProcessInfo.processInfo.environment
        environment.removeValue(forKey: "TAP_TOKEN")
        if let token { environment["TAP_TOKEN"] = token }
        for (key, value) in extra { environment[key] = value }
        process.environment = environment
        let output = Pipe()
        let errors = Pipe()
        process.standardOutput = output
        process.standardError = errors
        try process.run()
        process.waitUntilExit()
        return Run(
            output: String(data: output.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? "",
            errors: String(data: errors.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? "",
            status: process.terminationStatus
        )
    }

    // MARK: the tests

    @Test("without TAP_TOKEN it refuses and names the secret")
    func withoutTokenItRefuses() throws {
        let sums = try Self.checksums([(Self.digest035, "Tandem-0.35.0.zip")])
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("tap-bump-absent-\(UUID().uuidString)")
        let tap = root.appendingPathComponent("homebrew-tap")

        let run = try Self.run(["v0.35.0", sums.path, "--tap-dir", tap.path])

        #expect(run.status != 0)
        #expect(run.errors.contains("TAP_TOKEN"))
        #expect(!FileManager.default.fileExists(atPath: tap.path))
    }

    @Test("it bumps an older cask to the tag and pushes one commit")
    func itBumpsAnOlderCask() throws {
        let (work, origin) = try Self.makeTap(cask: Self.cask034)
        let sums = try Self.checksums([(Self.digest035, "Tandem-0.35.0.zip")])

        let run = try Self.run(["v0.35.0", sums.path, "--tap-dir", work.path])

        #expect(run.status == 0, "\(run.errors)")
        let bumped = try String(contentsOf: work.appendingPathComponent("Casks/tandem.rb"), encoding: .utf8)
        #expect(bumped == Self.cask035)
        #expect(try Self.commitCount(origin) == "2")
        #expect(try Self.log(origin, "%s") == "tandem 0.35.0")
        #expect(try Self.log(origin, "%cn") == "github-actions[bot]")
    }

    @Test("without a tap dir it clones through the token and strips it")
    func itClonesThroughTheTokenAndStripsIt() throws {
        let (_, origin) = try Self.makeTap(cask: Self.cask034)
        let token = "tap-token-fixture"
        let fixture = FileManager.default.temporaryDirectory
            .appendingPathComponent("tap-bump-clone-\(UUID().uuidString)")
        let bin = fixture.appendingPathComponent("bin")
        let brewPrefix = fixture.appendingPathComponent("brew")
        let brew = bin.appendingPathComponent("brew")
        try Self.write("#!/bin/sh\necho \"\(brewPrefix.path)\"\n", to: brew)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: brew.path)

        // Only the tokenised URL resolves, and it resolves to the fixture origin:
        // a push through anything else cannot authenticate, which is the
        // runner's fresh-clone path.
        let gitconfig = fixture.appendingPathComponent("gitconfig")
        try Self.write("""
        [url "\(origin.path)"]
        \tinsteadOf = https://x-access-token:\(token)@github.com/gianniacquisto/homebrew-tap.git

        """, to: gitconfig)
        let sums = try Self.checksums([(Self.digest035, "Tandem-0.35.0.zip")])
        let path = "\(bin.path):\(ProcessInfo.processInfo.environment["PATH"] ?? "/usr/bin:/bin")"

        let run = try Self.run(
            ["v0.35.0", sums.path],
            token: token,
            environment: [
                "GIT_CONFIG_GLOBAL": gitconfig.path,
                "GIT_TERMINAL_PROMPT": "0",
                "PATH": path,
            ]
        )

        #expect(run.status == 0, "\(run.errors)")
        let tap = brewPrefix.appendingPathComponent("Library/Taps/gianniacquisto/homebrew-tap")
        let bumped = try String(contentsOf: tap.appendingPathComponent("Casks/tandem.rb"), encoding: .utf8)
        #expect(bumped == Self.cask035)
        #expect(try Self.commitCount(origin) == "2")
        #expect(try Self.log(origin, "%s") == "tandem 0.35.0")
        // The push authenticated through the tokenised remote; the clone's own
        // config carries the plain URL and no trace of the token afterwards.
        #expect(try Self.capture(tap, ["remote", "get-url", "origin"])
            == "https://github.com/gianniacquisto/homebrew-tap.git")
        let config = try String(contentsOf: tap.appendingPathComponent(".git/config"), encoding: .utf8)
        #expect(!config.contains(token))
    }

    @Test("a cask already at the tag's version pushes nothing")
    func anUpToDateCaskPushesNothing() throws {
        let (work, origin) = try Self.makeTap(cask: Self.cask034)
        let sums = try Self.checksums([(Self.digest035, "Tandem-0.35.0.zip")])
        let arguments = ["v0.35.0", sums.path, "--tap-dir", work.path]

        let first = try Self.run(arguments)
        #expect(first.status == 0, "\(first.errors)")
        let head = try Self.log(origin, "%H")

        let second = try Self.run(arguments)

        #expect(second.status == 0, "\(second.errors)")
        #expect(second.output.contains("up to date"))
        #expect(try Self.log(origin, "%H") == head)
        #expect(try Self.commitCount(origin) == "2")
    }

    @Test("a checksums file with no line for the tag is refused")
    func aChecksumsFileWithoutTheTagIsRefused() throws {
        let (work, origin) = try Self.makeTap(cask: Self.cask034)
        let sums = try Self.checksums([(Self.digest035, "Tandem-0.34.0.zip")])
        let head = try Self.log(origin, "%H")

        let run = try Self.run(["v0.35.0", sums.path, "--tap-dir", work.path])

        #expect(run.status != 0)
        #expect(run.errors.contains("Tandem-0.35.0.zip"))
        #expect(try Self.log(origin, "%H") == head)
        #expect(try Self.commitCount(origin) == "1")
    }

    @Test("a tag without the v prefix is refused")
    func aTagWithoutThePrefixIsRefused() throws {
        let sums = try Self.checksums([(Self.digest035, "Tandem-0.35.0.zip")])
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("tap-bump-absent-\(UUID().uuidString)")
        let tap = root.appendingPathComponent("homebrew-tap")

        let run = try Self.run(["0.35.0", sums.path, "--tap-dir", tap.path])

        #expect(run.status != 0)
        #expect(run.errors.contains("0.35.0"))
        #expect(!FileManager.default.fileExists(atPath: tap.path))
    }

    @Test("a cask without its sha256 line is refused")
    func aCaskWithoutItsShaIsRefused() throws {
        let cask = Self.cask034.replacingOccurrences(
            of: "  sha256 \"b30ddae202193c06a005733848d38a2da3393c7d30aa33261c43b6d31ebfa7ec\"\n",
            with: ""
        )
        let (work, origin) = try Self.makeTap(cask: cask)
        let sums = try Self.checksums([(Self.digest035, "Tandem-0.35.0.zip")])
        let head = try Self.log(origin, "%H")

        let run = try Self.run(["v0.35.0", sums.path, "--tap-dir", work.path])

        #expect(run.status != 0)
        #expect(run.errors.contains("Casks/tandem.rb"))
        #expect(try Self.log(origin, "%H") == head)
        #expect(try Self.commitCount(origin) == "1")
    }
}
