import Foundation

/// ISO-8601 UTC timestamp formatting for the store.
///
/// All Tandem timestamps are ISO-8601 UTC with a trailing `Z` (e.g.
/// `2026-10-09T14:03:22Z`), per the store schema (ADR 0001). `ISO8601DateFormatter`
/// always renders in UTC with the `Z` designator and parses both `Z` and numeric
/// offsets, so hand-edited files with an explicit offset still read correctly.
///
/// The formatter is wrapped in a `@unchecked Sendable` box guarded by a lock: it is a
/// non-`Sendable` class, and the Store may be used from multiple threads.
enum Timestamp {
    private final class Box: @unchecked Sendable {
        private let formatter = ISO8601DateFormatter()
        private let lock = NSLock()

        func string(from date: Date) -> String {
            lock.lock()
            defer { lock.unlock() }
            return formatter.string(from: date)
        }

        func date(from string: String) -> Date? {
            lock.lock()
            defer { lock.unlock() }
            // Accept whole-second (canonical) and fractional-second timestamps.
            if let d = formatter.date(from: string) { return d }
            let fractional = ISO8601DateFormatter()
            fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
            return fractional.date(from: string)
        }
    }

    private static let box = Box()

    static func encode(_ date: Date) -> String {
        box.string(from: date)
    }

    static func decode(_ string: String) -> Date? {
        box.date(from: string)
    }
}
