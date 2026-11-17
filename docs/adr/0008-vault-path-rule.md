# Vault path hygiene: one rule, not three copies

[ADR 0004](0004-vault-protocol.md) put the vault behind a path-hygiene rule — refuse an empty path, any empty component (`//`), and any `.` or `..` component; the boundary that keeps a resolved path inside the vault. It lived in **three** places. `Bikeshed.fileURL(for:)` (TandemCore) and `VaultPaths.url` (the GUI target) were documented as copies "kept in step by review, not by code", and `BikeshedModel.existingVaultFile` carried a third, subtly different variant (a case-insensitive walk). The copies had already diverged: `VaultPaths.url` standardized the root, the core's did not. We decided: **one public rule**, `VaultPath.url(under:for:)` in TandemCore. The core, the GUI (`EditorHost`'s scheme handler and `BikeshedModel`'s link routing), and the model's markdown-link walk all call it; the GUI's `VaultPaths` is deleted.

## Why

- **"In step by review" is not a guardrail.** The rule guards a security boundary — a vault escape resolves outside the vault — so a silent divergence between copies is exactly the failure to design out, not to document.
- **The divergence was already live, not hypothetical.** The GUI copy standardized the root and the core copy did not; two copies that disagree about their input handling is the risk the comment warned about, already realized.
- **One place to test.** The rule had no direct test. It is now unit-tested for its whole contract: legal paths (including spaces), and every refusal (empty, `//`, leading/trailing slash, `.`, `..`, mid-path escapes), plus standardization.
- **Small and stable is an argument for one copy, not three.** A rule this small is one small thing in one place.

## Rejected

- **Leave the copies and keep the comment.** The copies had already diverged; the comment was doing the opposite of its job.
- **Make `Bikeshed.fileURL` public and delete only the GUI copy.** It leaves the model's third, case-insensitive variant and the standardization drift in place, and would expose a core-internal as a general API rather than naming the rule.
- **Fold the case-insensitive link walk into the shared rule.** That is a *different* rule — case-insensitive on-disk resolution for markdown links — deliberately distinct from the case-sensitive hygiene check (ADR 0004). Only the hygiene half is shared.

## Consequences

- **Standardization moves into the rule** (the GUI's prior behavior). The core's resolved URLs are now standardized too. No observed difference — callers pass clean absolute roots — and `EditorHost`'s containment check keeps calling `standardizedFileURL` as defence in depth.
- `Bikeshed.fileURL(for:)` becomes a one-line delegate; its signature and its tolerance of `#` / `|` (pre-existing on-disk violators) are unchanged.
- `BikeshedModel.existingVaultFile` validates through the rule before its case-insensitive walk, dropping its inline copy of the component check.
- The divergence cannot recur: there is one copy.

**A link is never followed.** A symlink (and on Windows a junction) inside the vault is refused by the same rule: a path with an existing component that is a link is no URL, so `read`, `write`, `create`, `move`, `rename`, `delete` and `asset` refuse it before the file is touched, and the tree, search, health, backlinks and stats walks skip it, which also bounds them against a cycle. The vault root may itself be a link, since a human may keep the vault on another volume: the walk starts below the root, so the root is never refused. The rule is one check in `BikeshedPath::url`, because a link inside the vault is a read and write path out of it once `tandem serve` binds beyond loopback.
