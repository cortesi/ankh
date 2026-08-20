# Contributing to Ankh

Ankh is the shared identity layer consumed by `../restless` and `../verber-web`.
It is developed greenfield: there are no backwards-compatibility constraints, but
both leaf products build against Ankh today, so changes must keep them working.

## Sibling checkout layout

All three repositories are developed as sibling checkouts:

```text
../ankh
../restless
../verber-web
```

The leaves consume Ankh through Cargo `path` dependencies and frontend `file:`
dependencies, so this layout is required for development. See `docs/contracts.md`
for the authoritative shared-surface contract.

## Local gates

Testing is local-only; there is no CI. The `cargo xtask` commands are the gates:

```sh
cargo xtask db start    # start the local Postgres used by integration tests
cargo xtask tidy        # Rust + frontend formatting and lints
cargo xtask test        # Rust tests (auto-starts Postgres) + frontend smoke tests
cargo xtask demo --seed # run the full stack locally with seeded demo identities
```

`cargo nextest` is required for `cargo xtask test`
(`cargo install cargo-nextest --locked`). See `DEV.md` for more.

## Changing a surface the leaves consume

Most Ankh crates and the `@ankh/*` frontend packages are consumed by both leaves,
so a change to any of them is a breaking change. When you change a consumed
surface:

1. Update `../restless` and `../verber-web` in the same change.
2. Remember that `ankh-types` DTO changes force each leaf to regenerate its
   checked-in `generated.d.ts` (the leaves prepend Ankh's shared declarations to
   their own product declarations); `cargo xtask tidy` in each leaf checks this.
3. Run `cargo xtask check-siblings` from Ankh before committing. It runs each
   present leaf's own `cargo xtask tidy` and `cargo xtask test` gates against the
   current Ankh working tree, and skips a leaf cleanly if it is not checked out.

Purely additive changes (new crates, new tests, new xtask subcommands, docs) do
not require leaf edits.
