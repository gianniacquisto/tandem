import Foundation

/// The GUI module's shared vault path hygiene: a vault-root-relative path
/// to a file URL under the vault root, or nil for a vault escape.
///
/// This is the ONE copy the GUI keeps. The core has its own, deliberately
/// separate copy at the module boundary — `Bikeshed.fileURL(for:)` in
/// TandemCore (the GUI cannot reach the core's internal, and the core must
/// not depend on the GUI); the rule is small and stable, so the two copies
/// are kept in step by review, not by code.
///
/// Refused (nil): an empty path, any empty component (`//`), and any `.`
/// or `..` component. Absolute targets never reach here — a leading `/`
/// means "vault-root-relative" and callers strip it before calling.
/// Callers that split the path themselves (the `tandem:` scheme handler
/// splits, percent-decodes, and re-joins before the URL is built) get the
/// same rule by passing the re-joined path.

enum VaultPaths {
    /// The vault-root-relative `path` as a file URL under `root`, or nil
    /// for a vault escape (see the type's documentation).
    static func url(under root: URL, for path: String) -> URL? {
        let components = path
            .split(separator: "/", omittingEmptySubsequences: false)
            .map(String.init)
        guard !components.isEmpty else { return nil }
        for c in components where c.isEmpty || c == "." || c == ".." {
            return nil
        }
        var url = root.standardizedFileURL
        for c in components { url = url.appendingPathComponent(c) }
        return url
    }
}
