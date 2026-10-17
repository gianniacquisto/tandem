import Foundation

/// Where Tandem keeps its state: the todo store, the notes vault, the lockfiles,
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
        if let override = environment["TANDEM_HOME"], !override.isEmpty {
            return URL(fileURLWithPath: (override as NSString).expandingTildeInPath)
                .standardizedFileURL
        }
        let shippedApp = !isDebugBuild && isInsideAppBundle(executablePath)
        return URL(fileURLWithPath: home)
            .appendingPathComponent(shippedApp ? ".tandem" : ".tandem-dev")
    }

    /// Whether `path` is an executable inside a `.app` bundle
    /// (`…/Tandem.app/Contents/MacOS/Tandem`) — i.e. installed, not built and run
    /// in place. An `.app` component anywhere in the path counts: `Bundle.main` for
    /// the in-bundle `tandem-cli` resolves to the enclosing app, so its executable
    /// path is still the one with `Tandem.app` in it.
    static func isInsideAppBundle(_ path: String) -> Bool {
        path.split(separator: "/").contains { $0.hasSuffix(".app") }
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
