import Foundation

/// The vault path-hygiene rule — the one place that decides whether a
/// vault-root-relative path is legal or a vault escape.
///
/// Refused (`nil`): an empty path, any empty component (`//`), and any `.` or
/// `..` component. This is the security boundary that keeps a resolved path
/// inside the vault, so it lives in exactly one place: the core's
/// `Bikeshed.fileURL(for:)`, the GUI's `EditorHost` and `BikeshedModel`, and
/// the `tandem:` scheme handler all call it (ADR 0008). It is deliberately
/// separate from the stricter create/rename naming rule
/// (`Bikeshed.validateNewName`), which additionally rejects hidden names and
/// `#` / `|`.
///
/// Absolute targets never reach here: a leading `/` means "vault-root-relative"
/// and callers strip it first.
public enum VaultPath {

    /// The vault-root-relative `path` as a file URL under `root`, or `nil` for
    /// a vault escape (see the type's documentation). `root` is standardized so
    /// the result is stable regardless of a trailing slash or redundant
    /// separators.
    public static func url(under root: URL, for path: String) -> URL? {
        let components = path
            .split(separator: "/", omittingEmptySubsequences: false)
            .map(String.init)
        guard !components.isEmpty else { return nil }
        for component in components
        where component.isEmpty || component == "." || component == ".." {
            return nil
        }
        var url = root.standardizedFileURL
        for component in components { url = url.appendingPathComponent(component) }
        return url
    }
}
