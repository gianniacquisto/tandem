# Security lens

The checklist for a diff that touches a secret, an input, a shell command, SQL, the store, or a permission.

## Read

- Every line the diff adds that reaches a shell, a child process or `tandem-cli`, and whether its arguments are passed as a list or interpolated into one string.
- Every SQL string in `rust/core/src/store/`, and what is bound versus pasted.
- Every path built from input: a note's name, a lane's name, a project name, a CLI argument. `rust/core/src/bikeshed/` and `rust/core/src/store/locked.rs` are the boundaries it must not leave.
- Every log line, error message and PR body for a token, a key, a home path or a ticket body that was not the author's to publish.
- Every write that removes or rewrites store rows, and the transaction it sits in (ADR 0018, ADR 0031).
- `--force`, `--admin`, `chmod`, `rm -rf`, `git worktree remove --force`, and any secrets or permission change in CI config.

## Blocks on

- A secret or token in the diff, a log line, a test fixture or the PR body. Cite the line it lands on.
- A shell command or SQL statement assembled by interpolating input, where a bound argument or an argument list exists: `rust/cli/src/main.rs`, `rust/core/src/store/`.
- A path built from input without the guard `rust/core/src/bikeshed/` already provides, so `..` or an absolute path escapes the bikeshed or the home.
- A write that can lose or expose store data: a delete or rewrite outside the transaction that owns it, a copy of the database that is not the rolling backup (ADR 0018).
- A destructive or privileged git or filesystem flag added without a `--force-with-lease`-shaped narrowing (AGENTS.md, Git), or a permission or CI secret change.

## Never blocks on

- Which of two safe spellings of the same guard the author chose.
- A missing comment on a security check that is readable as written.
- A dependency the change needs, when the brief named it and no lockfile or supply-chain rule is broken.
