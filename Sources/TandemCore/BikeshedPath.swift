import Foundation

/// The bikeshed path-hygiene rule — the one place that decides whether a
/// bikeshed-root-relative path is legal or a bikeshed escape.
///
/// Refused (`nil`): an empty path, any empty component (`//`), and any `.` or
/// `..` component. This is the security boundary that keeps a resolved path
/// inside the bikeshed, so it lives in exactly one place: the core's
/// `Bikeshed.fileURL(for:)`, the GUI's `EditorHost` and `BikeshedModel`, and
/// the `tandem:` scheme handler all call it (ADR 0008). It is deliberately
/// separate from the stricter create/rename naming rule
/// (`Bikeshed.validateNewName`), which additionally rejects hidden names and
/// `#` / `|`.
///
/// Absolute targets never reach here: a leading `/` means "bikeshed-root-relative"
/// and callers strip it first.
public enum BikeshedPath {

    /// The note's name — its file name without the `.md` extension. A note is any
    /// file ending `.md` (case-insensitively), so the extension is dropped either
    /// way.
    public static func name(of path: String) -> String {
        var name = path.split(separator: "/").last.map(String.init) ?? path
        if name.lowercased().hasSuffix(".md") { name = String(name.dropLast(3)) }
        return name
    }

    /// The note's folder — the path without its last component, or `root` for a note
    /// at the bikeshed's top.
    public static func folder(of path: String) -> String {
        if let slash = path.lastIndex(of: "/") { return String(path[..<slash]) }
        return "root"
    }

    /// The bikeshed-root-relative `path` as a file URL under `root`, or `nil` for
    /// a bikeshed escape (see the type's documentation). `root` is standardized so
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
