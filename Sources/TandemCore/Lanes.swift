import Foundation

/// The outcome of `deleteLane`: how many tickets fell back to the default area, and the
/// lane list after the write. One value, so the caller reports the result of the write
/// it made rather than a second read of the store.
public struct LaneDeletion: Sendable, Equatable {
    public let lanes: [String]
    public let moved: Int

    public init(lanes: [String], moved: Int) {
        self.lanes = lanes
        self.moved = moved
    }
}

/// A cycle lane: its name, its **key**, and the last number it issued
/// (ticket 6598E091). The key is the `TAN` of `TAN-42`, uppercase alphanumerics,
/// unique across lanes case-insensitively, and frozen once any ticket in the lane
/// holds a number. `lastNumber` is the lane's counter: it only ever rises, so a
/// number is never handed out twice even when the ticket that held it moved away.
public struct Lane: Sendable, Equatable {
    public var name: String
    public var key: String
    public var lastNumber: Int

    public init(name: String, key: String, lastNumber: Int = 0) {
        self.name = name
        self.key = key
        self.lastNumber = lastNumber
    }
}

/// The lane key's rules and the ref's rendering — one home for both faces, so the
/// generator the migration backfills with and the generator `lane add` uses cannot
/// disagree.
public enum LaneKey {
    /// Whether `raw` is a key: non-empty, uppercase ASCII letters and digits only.
    public static func isValid(_ raw: String) -> Bool {
        !raw.isEmpty && raw.unicodeScalars.allSatisfy {
            (65...90).contains($0.value) || (48...57).contains($0.value)
        }
    }

    /// The key as stored: trimmed and uppercased, then validated. An empty or
    /// non-alphanumeric key is a domain failure, never silently repaired.
    static func normalize(_ raw: String) throws -> String {
        let key = raw.trimmingCharacters(in: .whitespacesAndNewlines).uppercased()
        guard isValid(key) else { throw DomainError.invalidLaneKey(raw) }
        return key
    }

    /// The first three alphanumerics of `name`, separators stripped: `Tandem` → `TAN`,
    /// `code-rank` → `COD`. A collision extends the prefix (`TAND`, `TANDE`, …) and,
    /// past the whole name, numbers the three-letter stem (`TAN2`) — deterministic, so
    /// the migration's backfill and a later read agree.
    static func generate(from name: String, taken: Set<String>) -> String {
        var base = name.uppercased().filter { $0.isASCII && ($0.isLetter || $0.isNumber) }
        if base.isEmpty { base = "L" }
        var length = min(3, base.count)
        while length <= base.count {
            let candidate = String(base.prefix(length))
            if !taken.contains(candidate) { return candidate }
            length += 1
        }
        var suffix = 2
        while true {
            let candidate = String(base.prefix(min(3, base.count))) + String(suffix)
            if !taken.contains(candidate) { return candidate }
            suffix += 1
        }
    }
}

/// A ticket reference: `KEY-N` (`TAN-42`), the human handle every id-taking verb
/// accepts beside the UUID (ticket 6598E091).
public enum TicketRef {
    /// The ref as written: no zero padding.
    public static func render(key: String, number: Int) -> String {
        "\(key)-\(number)"
    }

    /// A raw handle split into the lane key and the number it names. The key is
    /// matched case-insensitively (`prb-2` is `PRB-2`); a token without a dash, or
    /// with anything but digits after it, is not a ref and reads as an unknown id.
    public static func parse(_ raw: String) -> (key: String, number: Int)? {
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        let parts = trimmed.split(separator: "-", omittingEmptySubsequences: false)
        guard parts.count == 2, let number = Int(parts[1]), number > 0 else { return nil }
        let key = String(parts[0]).uppercased()
        guard LaneKey.isValid(key) else { return nil }
        return (key, number)
    }
}

/// The store's contents: the ordered lanes and the tickets, in stored order.
///
/// **Stored order is the order.** The `todos` array position is authoritative (new tickets
/// land on top by default); it is never recomputed from `createdAt`. It is also kept
/// **grouped by lane** — first the default (ungrouped) area, then each lane in the
/// declared order — so the GUI can walk it once and emit a header whenever the lane
/// changes, and hand-edited files stay legible.
struct StoreContents: Sendable {
    /// Declared lanes, in render order after the default area.
    var lanes: [Lane]
    /// Tickets in stored order (grouped by lane).
    var todos: [Ticket]

    /// From a store that already has keys and numbers (the database's rows).
    init(lanes: [Lane], todos: [Ticket]) {
        self.lanes = lanes
        self.todos = todos
        refreshRefs()
    }

    /// From lane names — the legacy JSON import, where the file carries names only.
    /// The keys are generated and the tickets numbered exactly as the schema
    /// migration does, so an imported board and a migrated one read alike.
    init(laneNames names: [String], todos: [Ticket]) {
        self.lanes = names.map { Lane(name: $0, key: "", lastNumber: 0) }
        self.todos = todos
        prepare()
    }

    /// A lane key: `nil` is the default (ungrouped) area, always first.
    typealias Key = String?

    /// Every key in render order: the default first, then each declared lane, then any
    /// name that appears on a ticket but is not declared (a hand edit), in first-appearance
    /// order — so a hand-edited file loses no grouping.
    var orderedKeys: [Key] {
        var keys: [Key] = [nil]
        for lane in lanes where !keys.contains(lane.name) { keys.append(lane.name) }
        for todo in todos where todo.lane != nil {
            if !keys.contains(todo.lane) { keys.append(todo.lane) }
        }
        return keys
    }

    /// Every name that is safe to file into: each declared lane plus any dangling name
    /// already on a ticket.
    var knownLanes: [String] { orderedKeys.compactMap { $0 } }

    /// The index at which `key`'s block begins — the cumulative length of every preceding
    /// group. For a key that is neither declared nor present, that is the end of the list.
    func blockStart(of key: Key) -> Int {
        var start = 0
        for k in orderedKeys {
            if k == key { return start }
            start += todos.lazy.filter { $0.lane == k }.count
        }
        return start
    }

    /// Re-establishes the grouping invariant, stably: groups in `orderedKeys` order, each
    /// keeping its current relative order. Run before every read-modify-write body and
    /// again before the write, so the invariant holds for every operation. The refs are
    /// refreshed with it, so a ticket's `ref` always agrees with its lane's current key.
    mutating func normalize() {
        let keys = orderedKeys
        todos = keys.flatMap { key in todos.filter { $0.lane == key } }
        refreshRefs()
        refreshBlocked()
    }

    /// Makes the ref scheme true for contents that predate it, idempotently: every
    /// ticket's lane is declared, every lane has a key, and a ticket in a lane without a
    /// number gets the lane's next one in `createdAt` order (ties by UUID). The read
    /// path runs this too, so a version-2 database reads the refs its migration will
    /// make true — and a read still writes nothing.
    mutating func prepare() {
        declareLanes()
        assignKeys()
        numberUnnumbered()
        refreshRefs()
        refreshBlocked()
    }

    /// A lane a ticket names but the lane list does not carry (a hand edit) is declared,
    /// in first-appearance order, so it can be keyed and numbered.
    private mutating func declareLanes() {
        for todo in todos {
            guard let name = todo.lane else { continue }
            guard !lanes.contains(where: { $0.name == name }) else { continue }
            lanes.append(Lane(name: name, key: "", lastNumber: 0))
        }
    }

    /// Every lane without a key gets one, in render order, among the keys already
    /// taken. Deterministic, so two readers of the same pre-key store agree.
    private mutating func assignKeys() {
        var taken = Set(lanes.map { $0.key.uppercased() }.filter { !$0.isEmpty })
        for i in lanes.indices where lanes[i].key.isEmpty {
            let key = LaneKey.generate(from: lanes[i].name, taken: taken)
            taken.insert(key)
            lanes[i].key = key
        }
    }

    /// A ticket in a lane without a number takes the lane's next one, oldest first —
    /// the backfill an installed build's board needs once, and the same order the
    /// migration writes.
    private mutating func numberUnnumbered() {
        for i in lanes.indices {
            let name = lanes[i].name
            var highest = lanes[i].lastNumber
            var missing = todos.indices.filter {
                todos[$0].lane == name && todos[$0].number == nil
            }
            missing.sort { lhs, rhs in
                if todos[lhs].createdAt != todos[rhs].createdAt {
                    return todos[lhs].createdAt < todos[rhs].createdAt
                }
                return todos[lhs].id.uuidString < todos[rhs].id.uuidString
            }
            for index in missing {
                highest += 1
                todos[index].number = highest
            }
            lanes[i].lastNumber = max(lanes[i].lastNumber, highest)
        }
    }

    /// The derived half: each ticket's `ref` is its lane's key and its own number, or
    /// `nil` when either is missing (the default area has neither).
    mutating func refreshRefs() {
        var keys: [String: String] = [:]
        for lane in lanes where !lane.key.isEmpty { keys[lane.name] = lane.key }
        for i in todos.indices {
            guard let name = todos[i].lane,
                  let key = keys[name],
                  let number = todos[i].number
            else {
                todos[i].ref = nil
                continue
            }
            todos[i].ref = TicketRef.render(key: key, number: number)
        }
    }

    /// The dependency half, derived from the tickets just read or written (TAN-232): each
    /// ticket's `blockedBy` names the other ticket as it is now (its ref, its title, its
    /// status), and `blocks` is the same edges read from the far end.
    ///
    /// Called wherever `refreshRefs` is, so the read path and the write path fill the
    /// fields identically and a `block` result carries the edge it just wrote. The edge's
    /// key is the UUID, so a lane move or a key move updates the display without touching
    /// the edge. A target the contents do not carry cannot be named and cannot be shown,
    /// so it is dropped here; `delete` strips the far end in the same write.
    mutating func refreshBlocked() {
        var shown: [UUID: TicketBlock] = [:]
        for todo in todos {
            shown[todo.id] = TicketBlock(id: todo.id, ref: todo.ref,
                                         title: todo.title, status: todo.status)
        }
        for i in todos.indices {
            todos[i].blockedBy = todos[i].blockedBy.compactMap { shown[$0.id] }
        }
        var far: [UUID: [TicketBlock]] = [:]
        for todo in todos {
            guard let self2 = shown[todo.id] else { continue }
            for edge in todo.blockedBy {
                far[edge.id, default: []].append(self2)
            }
        }
        for i in todos.indices {
            todos[i].blocks = far[todos[i].id] ?? []
        }
    }

    /// Issue the lane's next number — the counter rises and never goes back, so a
    /// number a moved-away ticket used is not handed out again. The caller has already
    /// validated the lane.
    mutating func issueNumber(in lane: String) -> Int {
        guard let i = lanes.firstIndex(where: { $0.name == lane }) else { return 0 }
        lanes[i].lastNumber += 1
        return lanes[i].lastNumber
    }

    /// The key a lane files under, or `nil` when the lane is not declared.
    func key(of lane: String) -> String? {
        lanes.first { $0.name == lane }?.key
    }

    /// Guards a lane argument from the GUI: `nil` (the default area) is always valid;
    /// any other name must be a known lane.
    func validateLane(_ lane: String?) throws {
        guard let lane else { return }
        guard knownLanes.contains(lane) else {
            throw DomainError.unknownLane(lane)
        }
    }
}
