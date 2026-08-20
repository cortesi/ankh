# Ankh

Ankh is the shared identity layer for Restless and Verber Web. It owns the
common account, organization, session, device authorization, mail, admin, CLI,
and auth UI substrate while each leaf product keeps its product routes, product
tables, branding, and deployment shape.

The shared crates and frontend packages are implemented and consumed by both
leaf checkouts (`../restless` and `../verber-web`) through Cargo `path` and
frontend `file:` dependencies. `docs/contracts.md` is the authoritative contract
for the shared surface and the sibling-checkout layout.

## Crates

- `ankh-types` — shared IDs and DTOs (and the TypeScript generation pipeline).
- `ankh-names` — shared namespace name policies.
- `ankh-constants` — identity/session/mail/admin defaults.
- `ankh-db` — canonical identity schema and database methods.
- `ankh-mail` — transactional mail rendering and delivery.
- `ankh-web` — Axum routers, extractors, services, hooks, and audit surfaces.
- `ankh-cli` — common admin CLI plumbing.
- `ankh-testdata` — deterministic identity fixtures and harness helpers.
- `ankh-demo` — local demo server that boots the whole stack against dev Postgres.
- `ankh-xtask` — repository automation.

## Frontend Packages

- `@ankh/types` — generated TypeScript declarations.
- `@ankh/ui` — unbranded auth/org/device primitives. Ships an optional base
  stylesheet (`import "@ankh/ui/ankh.css"`) whose `--ankh-*` CSS variables a leaf
  can override to rebrand; leaves may also supply their own CSS instead.
- `@ankh/auth-react` — shared React auth, org, and device-session flows.
- `@ankh/demo-web` — minimal SPA assembling the shared flows against `ankh-demo`,
  for exercising the whole stack in a browser in isolation.

## Quick start

```sh
cargo xtask db start    # start the local Postgres used by integration tests
cargo xtask tidy        # Rust + frontend formatting and lints
cargo xtask test        # Rust tests (auto-starts Postgres) + frontend smoke tests
cargo xtask demo --seed # build the demo UI + run the full stack with seeded identities
```

`cargo xtask demo --seed` serves the demo UI and the API from one origin
(http://127.0.0.1:8080). For hot-reload UI work, add `--no-frontend` and run
`pnpm --filter @ankh/demo-web dev` alongside it.

Testing is local-only; there is no CI. When changing a surface consumed by the
leaves, run `cargo xtask check-siblings` to validate `../restless` and
`../verber-web`. See `DEV.md` for details.
