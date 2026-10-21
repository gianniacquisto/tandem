import Testing
@testable import TandemCore

/// TEMPORARY — a deliberate failure, added to prove that a red `test` job turns the
/// `release` aggregator red (a failed check) and not skipped (which GitHub counts
/// as a passing status for a required check). Removed in the next commit.
@Suite("Temporary red probe")
struct TempRedProbeTests {
    @Test("fails on purpose")
    func redOnPurpose() {
        #expect(1 == 2, "the red probe: this test must fail so the aggregator's green-check is exercised")
    }
}
