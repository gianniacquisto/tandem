import Foundation

/// The one text comparison, shared by everything that matches a human's word
/// against stored text (ADR 0008's "one rule, one place").
///
/// The store's query (`TodoQuery`) and the quick-open palette both match the
/// same way: a case- and diacritic-insensitive substring. Two surfaces, one
/// comparison — so "app" finds the same things in the list, in the CLI's
/// `list --text`, and in the ⌘K palette, and the palette cannot drift into a
/// second, cleverer rule.
public enum TextMatch {

    /// The comparison options: case-insensitive and diacritic-insensitive.
    public static let options: String.CompareOptions = [.caseInsensitive, .diacriticInsensitive]

    /// Whether `haystack` contains `needle` under those options.
    public static func contains(_ needle: String, in haystack: String) -> Bool {
        haystack.range(of: needle, options: options) != nil
    }
}
