import Foundation

/// Where Tandem keeps its state: the todo store, the bikeshed, the lockfiles,
/// the pid file. **A dev build never touches the real one.**
///
/// The rule: the real `~/.tandem` belongs to the *shipped app* — a release build
/// running inside a `.app` bundle, which is what the cask installs and what agent
/// configs point at. Everything else — `swift run Tandem`, a bare
/// `.build/…/tandem-cli`, any debug build — gets `~/.tandem-dev`. So a run from
/// source can add, complete, reorder, or wipe as violently as it likes without
/// going near the list the human actually keeps.
///
/// This is not a nicety: on 2026-10-17 a from-source run of the GUI was reading and
/// writing the real store, so a schema mistake in it was one build away from the
/// real list. Separate directories make that class of accident impossible rather
/// than unlikely.
///
/// `TANDEM_HOME` overrides both, for pointing a single run at a throwaway
/// directory.
public enum TandemHome {

    /// The directory this process keeps its state in.
    public static var directory: URL {
        resolve(environment: ProcessInfo.processInfo.environment,
                executablePath: executablePath,
                home: NSHomeDirectory(),
                isDebugBuild: isDebugBuild)
    }

    /// Why a process resolved where it did — the human-facing half of the rule,
    /// surfaced by `tandem-cli doctor` (ADR 0014). One owner: `source` is the
    /// rule, `resolve` derives the directory from it.
    public enum Source: Sendable, Equatable {
        /// `TANDEM_HOME` was set non-empty and wins outright.
        case environmentOverride
        /// A release build living inside an `.app` bundle — the shipped app.
        case shippedApp
        /// Anything else — `swift run`, a bare `.build/…` binary, any debug build.
        case devBuild
    }

    /// The pure rule, factored out so it can be tested without building two
    /// binaries.
    ///
    /// Precedence: `TANDEM_HOME` wins outright; otherwise a release build that
    /// lives inside an `.app` bundle is the shipped app (real `~/.tandem`) and
    /// anything else is a dev build (`~/.tandem-dev`).
    static func resolve(environment: [String: String],
                        executablePath: String,
                        home: String,
                        isDebugBuild: Bool) -> URL {
        switch source(environment: environment,
                      executablePath: executablePath,
                      isDebugBuild: isDebugBuild) {
        case .environmentOverride:
            let override = environment["TANDEM_HOME"] ?? ""
            return URL(fileURLWithPath: (override as NSString).expandingTildeInPath)
                .standardizedFileURL
        case .shippedApp:
            return URL(fileURLWithPath: home).appendingPathComponent(".tandem")
        case .devBuild:
            return URL(fileURLWithPath: home).appendingPathComponent(".tandem-dev")
        }
    }

    /// Which branch of the rule this process takes, as a value (the `resolve`
    /// switch reads it, and `doctor` reports it verbatim). Same precedence as
    /// `resolve`; the two cannot drift because `resolve` is defined in terms of
    /// this.
    static func source(environment: [String: String],
                       executablePath: String,
                       isDebugBuild: Bool) -> Source {
        if let override = environment["TANDEM_HOME"], !override.isEmpty {
            return .environmentOverride
        }
        let shippedApp = !isDebugBuild && isInsideAppBundle(resolvedPath(executablePath))
        return shippedApp ? .shippedApp : .devBuild
    }

    /// Whether `path` is an executable inside a `.app` bundle
    /// (`…/Tandem.app/Contents/MacOS/Tandem`) — i.e. installed, not built and run
    /// in place. An `.app` component anywhere in the path counts: `Bundle.main` for
    /// the in-bundle `tandem-cli` resolves to the enclosing app, so its executable
    /// path is still the one with `Tandem.app` in it.
    static func isInsideAppBundle(_ path: String) -> Bool {
        path.split(separator: "/").contains { $0.hasSuffix(".app") }
    }

    /// The executable's real path. `Bundle.main` reports the path the process was
    /// *started* by — so a `PATH` symlink (`/opt/homebrew/bin/tandem-cli` →
    /// `…/Tandem.app/Contents/MacOS/tandem-cli`) arrives without the `.app`
    /// component, and would read as a dev build, splitting the linked CLI onto
    /// `~/.tandem-dev` while the GUI beside it writes `~/.tandem`. Resolving the
    /// link first keeps the `.app` test true for both invocation styles.
    static func resolvedPath(_ path: String) -> String {
        (path as NSString).resolvingSymlinksInPath
    }

    /// This executable's path, with a fallback for the (unlikely) case where the
    /// bundle machinery has nothing to offer.
    static var executablePath: String {
        Bundle.main.executableURL?.path ?? CommandLine.arguments.first ?? ""
    }

    /// A debug build is never the shipped app, whatever it is nested in.
    static var isDebugBuild: Bool {
        #if DEBUG
        true
        #else
        false
        #endif
    }
}
