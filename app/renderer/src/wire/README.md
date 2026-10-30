# The wire's generated types

Every `.ts` file in this directory is generated from the Rust wire types by
`ts-rs`. Do not edit them by hand: the edit is lost on the next generation and
`rust/core/tests/wire_types.rs` fails on a committed file that no longer
matches its Rust source.

Regenerate from the repository root:

```sh
cd rust && cargo run -p tandem-core --example export-wire
```

The Rust types are `rust/core/src/wire/rpc.rs` (the protocol) and
`rust/core/src/wire/views.rs` (the ticket and comment shapes). The list of
exported files is `rust/core/src/wire/typescript.rs`.
