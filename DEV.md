# Development Notes

## Testing

- `cargo xtask tidy` runs Rust formatting, Rust lints, frontend linting, and
  frontend formatting checks.
- `cargo xtask test` starts the local Postgres if it is not already running,
  runs the Rust tests with `cargo nextest`, then runs the frontend package smoke
  tests. The DB-backed integration tests are part of the default run, so a
  reachable Postgres is required (`cargo xtask test` provisions it for you).

`cargo nextest` is required for `cargo xtask test`; install it with
`cargo install cargo-nextest --locked`.

The Ankh frontend workspace uses Node `25.8.1` and pnpm `10.32.1`. Enable
Corepack before running frontend commands directly:

```sh
corepack enable
cd frontend
pnpm install --frozen-lockfile
```

## Local Postgres

Ankh database integration tests use a local Postgres instance managed by the
`cargo xtask db` commands:

- `cargo xtask db start` starts (and seeds) the instance. Pass `--recreate` to
  drop and rebuild the data directory, or `--port` to override the port.
- `cargo xtask db stop` stops the instance.
- `cargo xtask db status` reports whether it is running.

The default port is `55435`, with data in `tmp/pgdata`.

## Demo server

`cargo xtask demo` runs the whole Ankh stack (public + admin routers, the
identity database, and a `DevMailer`) as a live local server, in isolation from
the leaf products. It ensures Postgres is running first, then serves on
`http://localhost:8080` (the next free port if 8080 is busy).

- `cargo xtask demo --seed` seeds deterministic identities and prints their
  login credentials (a verified user, an unverified user, a sysadmin, and an
  org) before serving.
- `cargo xtask demo --reset` recreates the database first.
- `cargo xtask demo --port <PORT>` overrides the HTTP port.
- `cargo xtask demo --no-frontend` skips building the demo UI (serve an existing
  bundle, or run Vite separately for hot-reload).

The demo uses non-`Secure` session cookies so cookie auth works over plain HTTP
on localhost, and writes captured mail to `tmp/mail/` as `DevMailer` artifacts.
Point `ankh-cli` at `http://localhost:8080` to exercise the admin API.

## Demo UI

`@ankh/demo-web` is a minimal SPA that assembles the shared `@ankh/auth-react`
pages and panels against the demo backend. `cargo xtask demo` builds it (via
`vite build`) into `crates/ankh-demo/dist/`, and `ankh-demo` serves it as the
SPA fallback — so the UI and the API share one origin and `@ankh/auth-react`'s
same-origin requests reach the backend directly, no proxy required.

For hot-reload UI work, run the backend and the Vite dev server side by side:

```sh
cargo xtask demo --seed --no-frontend   # backend + API on :8080
pnpm --filter @ankh/demo-web dev         # UI on :5173, proxies /api and /admin to :8080
```

The dev server's proxy target defaults to `http://127.0.0.1:8080`; override it
with `ANKH_DEMO_BACKEND_URL`. A leaf product's own frontend dev server can be
proxied to the demo the same way.

## Workspace Layout

Ankh is developed as a sibling checkout beside its consumers:

```text
../ankh
../restless
../verber-web
```

Until the API is stable enough for pinned Git revisions, Restless and Verber
consume Ankh through Rust path dependencies and frontend `file:` dependencies.

## Cross-repo changes

Both leaves build against most Ankh crates and the `@ankh/*` frontend packages,
so any change to a consumed surface is a breaking change. Before committing such
a change:

- Update `../restless` and `../verber-web` in the same change.
- Remember that `ankh-types` DTO changes force each leaf to regenerate its
  checked-in `generated.d.ts` (the leaves prepend Ankh's shared declarations to
  their own product declarations).
- Run `cargo xtask check-siblings`, which runs each present leaf's own
  `cargo xtask tidy` and `cargo xtask test` gates (Rust, generated-TypeScript
  freshness, and frontend) against this Ankh working tree. Missing siblings are
  skipped with a note.

## Continuous integration

There is none. Testing is local-only; `cargo xtask tidy`, `cargo xtask test`,
and `cargo xtask check-siblings` are the gates.
