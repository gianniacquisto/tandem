# Shared storage mechanics: one `LockedDirectory` primitive owns the sidecar protocol

[ADR 0001](0001-store-protocol.md) fixed the store protocol — sidecar `flock`, atomic temp + fsync + `rename`, orphan-temp cleanup — and [ADR 0004](0004-vault-protocol.md) reused it for the vault. Both domains implemented it **separately**: `Store` and `Bikeshed` each carried the same 2 s retry window, the same 10–50 ms backoff, the same 60 s orphan age, and their own copies of lock acquire/release, `fsync`, the atomic write, and the orphan sweep — differing only in the temp-file prefix (`.todos-` / `.note-`) and the names of the errors they throw. We decided: a single core primitive, `LockedDirectory`, owns those mechanics. `Store` and `Bikeshed` keep their schema, their recovery policy, and their **own error vocabulary**; the primitive throws a neutral `LockedDirectoryError` that each domain translates into its own errors at the seam. No file format, no user-facing message, and no public surface changes.

## Why the primitive owns the mechanism, not the vocabulary

- **Neither domain can throw the other's errors.** A shared primitive that threw `DomainError.locked` would make the bikeshed depend on the todo domain, and vice versa. A neutral `LockedDirectoryError` plus a one-case translation per domain (`catch .locked { throw … }`) keeps each domain's words its own — "The Tandem store is locked…" and "The bikeshed is locked…" stay distinct — and keeps the existing contract tests (`#expect(throws: DomainError.locked)`, `#expect(throws: BikeshedError.locked)`) valid.
- **"In step by review" is not a guardrail.** A fix to the fsync/rename/orphan handling or a change to the retry window currently needs two edits and can silently land in only one. The vault protocol already copied the store's tuning with a comment saying so.
- **What varies is data, not structure.** The lockfile path, the temp prefix, and whether the orphan sweep recurses (the vault's temps sit beside each note; the store is one flat directory) are configuration on one type — not behaviour that needs a protocol and two adapters.
- **Testability.** The primitive can be unit-tested directly — lock acquisition, exclusion, timeout, atomic write, orphan sweep — instead of only being exercised through `Store`/`Bikeshed` integration paths.

## Rejected

- **Throwing domain errors from the primitive, or parameterizing it with an error factory** — more interface for the same translation the domain would do anyway.
- **A `protocol DirectoryStore` with two adapters** — the shared part is a value, not behaviour that varies by concrete implementation; a protocol here is ceremony (one adapter means a hypothetical seam).
- **Leaving it duplicated because each copy is small** — it is ~55–60 lines across six logical blocks per domain, and the divergence risk is exactly the kind "kept in step by review" fails.
- **A name like `SidecarStore`** — GLOSSARY's capital-S **Store** is the todo source of truth; `LockedDirectory` avoids overloading it.

## Consequences

- One place to change the contention window, backoff, orphan age, `fsync`, `rename`, or the orphan sweep; a fix applies to both domains.
- The primitive has its own suite (`LockedDirectoryTests`): lock acquisition / exclusion / timeout, atomic write and parent-folder creation, and the orphan sweep flat and recursive — a contract that was previously only exercised indirectly.
- `Store` and `Bikeshed` keep their internal surfaces (`directory`, `fileURL`, `lockURL`, `locked`, `atomicWrite`), so every caller and existing test is unchanged.
- Orphan cleanup now also runs on the store's corrupt-file **recovery read** path (it previously ran only on the write path). It is idempotent and bounded by the same 60 s age threshold, and the store's own documentation already stated the rule as "after acquiring the lock".
- The vault protocol's other duplicated rule — path hygiene — is a separate change.
