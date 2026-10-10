import AppKit

/// Single-instance guard across launch identities.
///
/// A bare executable (terminal `tandem`, `swift run Tandem`) and the bundled
/// `Tandem.app` (Launchpad/Spotlight) are distinct identities to LaunchServices,
/// so both could run at once — two Dock icons, two menu bar items. This guard
/// makes them one: the first instance to start claims `~/.tandem/app.pid`; a
/// later launch activates the running instance and exits.
///
/// The claim is an atomic `O_CREAT | O_EXCL` (no read-modify-write race). A
/// stale file — holder crashed or was `kill -9`'d — is reclaimed: the pid must
/// name a live GUI process, or the file is treated as abandoned.
enum SingleInstance {
    /// Claims the single-instance slot. Returns `true` if another instance is
    /// already running (it has been activated) and this process should exit.
    static func claimOrActivate() -> Bool {
        let fm = FileManager.default
        try? fm.createDirectory(
            at: pidFileURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )

        if claim() {
            installSigtermRouting()
            return false
        }
        if let pid = aliveClaimant() {
            // Another instance owns the slot: bring it forward and exit.
            NSRunningApplication(processIdentifier: pid)?.activate(options: [])
            return true
        }
        // Stale claim: reclaim it and run.
        try? fm.removeItem(at: pidFileURL)
        if claim() {
            installSigtermRouting()
            return false
        }
        // Lost the reclaim race — run anyway rather than refuse to start.
        return false
    }

    /// Releases the single-instance slot on a clean exit (call from
    /// `applicationWillTerminate`), but only if the file still names *this*
    /// process — a slow exit must never delete a successor's claim. A crash or
    /// `kill -9` leaves the file stale; the next launch reclaims it.
    static func release() {
        let expected = "tandem \(ProcessInfo.processInfo.processIdentifier)\n"
        guard let raw = try? String(contentsOf: pidFileURL, encoding: .utf8),
              raw == expected else { return }
        try? FileManager.default.removeItem(at: pidFileURL)
    }

    // MARK: - Internals

    /// `~/.tandem/app.pid` — a sidecar in the store's directory, like
    /// `tandem.lock` (ADR 0001), but for app lifecycle rather than store writes.
    private static var pidFileURL: URL {
        FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent(".tandem")
            .appendingPathComponent("app.pid")
    }

    /// Atomically creates the pidfile containing `tandem <pid>`.
    private static func claim() -> Bool {
        let fd = pidFileURL.path.withCString { open($0, O_WRONLY | O_CREAT | O_EXCL, 0o644) }
        guard fd >= 0 else { return false }
        defer { close(fd) }
        let payload = "tandem \(ProcessInfo.processInfo.processIdentifier)\n"
        return payload.withCString { write(fd, $0, strlen($0)) == strlen($0) }
    }

    /// The pid of the live GUI process that owns the pidfile, or `nil` if the
    /// file is missing, malformed, stale, or names a process we cannot activate
    /// (e.g. a reused pid belonging to a non-GUI process — treated as abandoned
    /// so a launch never dies silently).
    private static func aliveClaimant() -> pid_t? {
        guard let raw = try? String(contentsOf: pidFileURL, encoding: .utf8) else { return nil }
        let fields = raw.split(separator: "\n").first?.split(separator: " ").map(String.init)
        guard let fields, fields.count == 2, fields[0] == "tandem",
              let pid = Int32(fields[1]), pid > 0
        else { return nil }
        guard kill(pid, 0) == 0 else { return nil }  // Stale: the holder is gone.
        // A reused pid belonging to a non-GUI process is not a claimant: treating
        // it as abandoned means a launch degrades to a second instance rather
        // than dying silently.
        guard NSRunningApplication(processIdentifier: pid) != nil else { return nil }
        return pid
    }

    /// Routes SIGTERM through AppKit's normal termination path, so `kill <pid>`
    /// cleans up like Cmd-Q (`applicationWillTerminate` → `release()`). If the
    /// main thread is wedged and the clean termination has not run within 2 s,
    /// the process is killed hard — `kill` must always eventually work.
    private static func installSigtermRouting() {
        signal(SIGTERM) { _ in
            DispatchQueue.main.async { NSApp.terminate(nil) }
            DispatchQueue.global().asyncAfter(deadline: .now() + 2) {
                kill(getpid(), SIGKILL)
            }
        }
    }
}
