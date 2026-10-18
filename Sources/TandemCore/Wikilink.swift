import Foundation

/// A wikilink (GLOSSARY: Wikilink) as written in a note:
/// `[[target[#fragment][|alias]]]`. Parsed per the spec's v1 grammar — split
/// at the first `#`, then the first `|` (either order); the fragment is
/// parsed but inert in v1 (`[[note#Heading]]` displays "Heading", the click
/// opens the note at the top).
public struct Wikilink: Sendable, Equatable {
    /// The note the link points at, as written (no `#fragment`, no `|alias`).
    public let target: String
    /// The `#fragment` part, if present (inert in v1).
    public let fragment: String?
    /// The `|alias` part, if present.
    public let alias: String?

    /// The text the editor displays: the alias if present, else the fragment
    /// if present, else the target as written.
    public var displayText: String {
        alias ?? fragment ?? target
    }

    /// Parses the content of a `[[…]]` token — the text between the brackets.
    /// Returns nil for what the grammar says is not a link: an empty or
    /// whitespace-only target (which also covers the same-note form
    /// `[[#Heading]]`). Unclosed `[[`, embeds `![[…]]`, and the code-fence /
    /// inline-code exemption are scanner-level rules (see `Bikeshed.links(in:)`).
    public init?(raw: String) {
        let hash = raw.firstIndex(of: "#")
        let pipe = raw.firstIndex(of: "|")
        let targetEnd = min(hash ?? raw.endIndex, pipe ?? raw.endIndex)
        let target = String(raw[..<targetEnd])
        guard !target.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            return nil
        }
        self.target = target
        if let hash, let pipe {
            if hash < pipe {
                self.fragment = Self.present(String(raw[raw.index(after: hash)..<pipe]))
                self.alias = Self.present(String(raw[raw.index(after: pipe)...]))
            } else {
                self.alias = Self.present(String(raw[raw.index(after: pipe)..<hash]))
                self.fragment = Self.present(String(raw[raw.index(after: hash)...]))
            }
        } else if let hash {
            self.fragment = Self.present(String(raw[raw.index(after: hash)...]))
            self.alias = nil
        } else if let pipe {
            self.alias = Self.present(String(raw[raw.index(after: pipe)...]))
            self.fragment = nil
        } else {
            self.fragment = nil
            self.alias = nil
        }
    }

    /// An empty or whitespace-only fragment/alias counts as absent (the
    /// display text falls back to the next-present part).
    private static func present(_ s: String) -> String? {
        s.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : s
    }
}

/// Where a wikilink target resolves, computed live from the current bikeshed
/// file set — derived state, no stored link index (spec: wikilink model).
public enum LinkResolution: Sendable, Equatable {
    /// Exactly one note matches; `path` is its bikeshed-root-relative path.
    case resolved(path: String)
    /// Zero or multiple matches: a broken link.
    case broken
}

/// One wikilink occurrence in a note's text: where the `[[…]]` token is, its
/// parsed form, where it resolves, and the line it sits on.
public struct LinkOccurrence: Sendable, Equatable {
    /// The range of the whole token, brackets included, in the text.
    public let range: Range<String.Index>
    public let link: Wikilink
    public let resolution: LinkResolution
    /// The line of the text containing the token (without its newline).
    public let line: String
}
