import Foundation
import Testing
@testable import TandemCore

// Placeholder — the core test suite lands next.
@Suite("TandemCore (placeholder)")
struct PlaceholderTests {
    @Test("smoke")
    func smoke() {
        let dir = URL(fileURLWithPath: "/tmp").appendingPathComponent("tandem-placeholder-\(UUID().uuidString)")
        let store = Store(directory: dir)
        #expect(store.fileURL.lastPathComponent == "todos.json")
        #expect(store.lockURL.lastPathComponent == "tandem.lock")
        try? FileManager.default.removeItem(at: dir)
    }
}
