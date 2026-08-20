# Ankh Project Review — Findings and Next Steps

Review date: 2026-06-18. Re-validated 2026-06-18 after the initial `main` commit
(`feat: add shared identity layer`) and the `cargo xtask` CLI refactor
(`dev db` → `db start` / `db stop` / `db status`), then cross-checked by an independent Codex
review and a second self-review against the `ankh`, `restless`, and `verber-web` trees.
Commands run: `cargo xtask tidy`, `cargo xtask test` (with and without local Postgres),
`cargo test -- --ignored` (with Postgres running), `cargo xtask db status`.

## Executive Summary

Ankh is substantially further along than its README claims. The workspace has ~18k lines of
Rust and ~3k lines of frontend code spanning types, database, mail, web routers, CLI plumbing,
and React auth UI. `cargo xtask tidy` passes. The tree has an initial commit on `main`.

Testing is **local only** — there is no CI, and the stale `.github/workflows/ci.yml` should be
removed (Stage 2). The `cargo xtask` gates are the single source of truth.

Three things dominate, and the first two are corrections to the prior draft of this plan:

1. **The local test gate is broken without Postgres.** `crates/ankh-cli/tests/admin_cli.rs:277`
   (`common_commands_use_saved_profile_and_real_admin_routes`) is **not** `#[ignore]`d and calls
   `with_fresh_db`, so `cargo xtask test` fails with connection-refused whenever a developer has
   not started Postgres. Meanwhile the bulk of integration coverage hides behind `#[ignore]`. The
   priority is a reliable local gate: `cargo xtask test` should start (or clearly require)
   Postgres and run the DB-backed coverage — not merely "un-skip" tests.

2. **The leaves already consume Ankh, and `docs/contracts.md` says they don't.** `../restless`
   and `../verber-web` both path-dep most Ankh crates and `file:`-dep the `@ankh/*` frontend
   packages today (see "Cross-repo compatibility"). `docs/contracts.md:24` still states "Leaves
   do not consume Ankh during Stage One" — it is stale and must be corrected before further work
   leans on it.

3. **There is no way to run Ankh as a live system in isolation.** `ankh-web` is a library, so
   the only thing exercising routers + DB + mail + CLI together is the test harness. A small
   `ankh-demo` binary closes this and gives manual QA and the React auth UI a real backend.

Highest-leverage order: establish a cross-repo safety net and fix the contract (Stage 1), make
the local test gate reliable and honest (Stage 2), add the demo server (Stage 3), then close
DB/router/CLI test holes and refresh docs.

## Cross-repo compatibility (hard constraint)

Ankh is consumed in-tree by two sibling checkouts that must keep building and passing their
gates. Any change to a consumed surface that does not also update the leaves breaks them.

**Consumed Rust crates** (both leaves): `ankh-constants`, `ankh-names`, `ankh-types`, `ankh-db`,
`ankh-mail`, `ankh-web`, `ankh-cli`, `ankh-testdata`, `ankh-xtask`.

**The `ankh-xtask` *library* is a broad contract**, even though its *binary* CLI is not:
- Restless `crates/xtask` imports `ankh_xtask::command::{run_async_result, exec_cargo, workspace_root_from_manifest}`.
- Verber `crates/xtask` imports from `ankh_xtask::admin` (`AdminLogin`, `CargoAdminCli`,
  `ensure_admin_login`, `run_cli`), `ankh_xtask::web` (`WebdevStateFile`), `ankh_xtask::frontend`
  (`run_pnpm_if_available`), the db/postgres helpers, and `ankh_xtask::command`.
- So the public surfaces of `command.rs`, `admin.rs`, `web.rs`, `frontend.rs`, and `postgres.rs`
  are all consumed. The recent `dev db` → `db start` change is leaf-safe because it only touched
  the xtask *binary* (`main.rs`); confirm with a leaf build regardless. (Bonus: the existing
  `ankh_xtask::web::WebdevStateFile` and `frontend`/`admin` helpers can be reused for the demo.)

**TypeScript generation crosses repos.** Each leaf's checked-in `generated.d.ts` is built as
`ankh_types::ts::typescript_declaration_body()` (the shared Ankh DTOs) **prepended** to the
leaf's own product DTOs (`restless/crates/restless-web/src/lib.rs`,
`verber-web/crates/verber-web/src/lib.rs`), and each leaf's `tidy` asserts that file is current.
**Therefore any change to an Ankh DTO makes both leaves' `tidy` fail until they regenerate** —
this cannot be caught by Ankh's own gate. Frontend `@ankh/*` packages are `file:`-linked in both
leaf `pnpm-lock.yaml`s, so changes to `@ankh/types`/`@ankh/ui`/`@ankh/auth-react` require a leaf
`pnpm install`/build/test to validate.

Implications:
- Treat "both leaves build and pass their full gates (Rust + frontend + generated-type freshness)"
  as part of *done* for any Ankh change.
- Renames/signature changes in any consumed crate, the `ankh_xtask` library modules, or the
  `@ankh/*` packages are breaking changes requiring coordinated edits in both leaves in the same
  change.
- The `ankh-demo` crate and `cargo xtask demo` subcommand (Stage 3) are **additive** and change
  no consumed API.

## Current State vs Documentation

| Area | README / contracts claim | Actual state |
| --- | --- | --- |
| Extraction stage | Stage 1 scaffold; empty crates | Stages ~5–8 largely complete |
| Rust crates | Skeleton | Full implementations in all nine crates |
| Frontend packages | Empty | `@ankh/types`, `@ankh/ui`, `@ankh/auth-react` with real code |
| Leaf consumption | `contracts.md`: none in Stage One | **Live**: both leaves path-dep / `file:`-dep most Ankh crates + packages |
| Git history | — | Initial commit on `main` (`9dc9236`); 124 files tracked |
| Testing | — | **Local only**; no CI. `cargo xtask test` fails without Postgres (non-ignored DB test) |
| Dev tasks | `cargo xtask dev db` | `cargo xtask db start` / `stop` / `status`; bare invocation prints help |

`docs/contracts.md` is otherwise the authoritative contract but is stale on leaf consumption
(`contracts.md:24`). README and inline comments (e.g. `ankh-web/src/lib.rs:13` "filled in by
Stage Five") also lag the implementation.

## Strengths

1. **Clear crate boundaries.** Types, names, constants, db, mail, web, cli, testdata, and xtask
   each own a well-scoped surface; dependency direction is clean.
2. **Contract-first design.** Routes, table ownership, hook dispatch, mail catalog, CLI flags,
   and sibling-checkout layout are documented (where current).
3. **Test harness quality.** `with_fresh_db`, `TestAppHarness`, `RecordingMailer`,
   `FakeAuditSink`, and `FakeHookRecorder` give deterministic, sleep-free integration tests.
4. **TypeScript generation pipeline.** `ankh-types` generates `@ankh/types` declarations;
   `tidy` enforces freshness in Ankh and in both leaves.
5. **Strict lint posture.** Workspace clippy denies warnings; nightly rustfmt; oxlint/oxfmt.
6. **Frontend auth-react coverage.** Twelve Vitest tests exercise context, protected routes,
   login, reset, org switching, invites, verification, members, and device sessions (mocked).
7. **Composable router surface.** `ankh_web::router()` + `admin_router()` build plain
   `axum::Router`s; `AnkhWebState` is assembled via builders (`with_hooks`, `with_admin_audit`),
   so a demo binary can mount the full stack with no new plumbing in `ankh-web`
   (see `crates/ankh-web/src/test_support.rs:20` for the canonical composition).

## Gaps and Risks

### 1. Local test gating (critical)

**Observed behavior**

- `crates/ankh-cli/tests/admin_cli.rs:277` is a non-ignored `with_fresh_db` test, so
  `cargo xtask test` (`cargo test --all-features`) fails with connection-refused whenever
  Postgres is not running.
- Twelve `#[ignore]` tests exist, but only **eleven require Postgres** (nine `ankh-web` router
  tests, one `ankh-db`, one `ankh-testdata`). The twelfth (`ankh-types/src/ts.rs:207`) is the
  TypeScript regeneration helper, gated for a different reason.
- The ignored router tests therefore hide the *bulk* of integration coverage behind a flag that
  the default gate never sets.

**Risk:** the default `cargo xtask test` is unreliable — it fails for any developer who forgot
`cargo xtask db start`, yet still silently skips most integration coverage. There is no CI to
catch regressions, so the local gate must both run reliably and exercise the DB-backed tests.

### 2. No runnable system in isolation (high)

`ankh-web` is a library; nothing boots the routers against a real DB with a real mailer.
Consequences: `ankh-cli` (an HTTP client) has no local target outside the in-process harness;
`@ankh/auth-react` can only be developed against mocked fetch; there is no cross-layer smoke.

### 3. `ankh-db` integration coverage (high)

80+ async methods across users, sessions, orgs, devices, tokens, invites, sysadmins, namespaces;
only schema/metadata/support unit tests plus one ignored lifecycle test. No direct tests for
signup, org membership rules, device grant consume/expiry, token expiry, namespace
suspend/reinstate, or pagination edges.

### 4. Public API router coverage (high)

Ignored router tests cover login, org listing, browser device sessions, PKCE device auth, and
admin routes. Missing: signup (waitlist/invite/org-invite), email verify + resend, password
reset (forgot/validate/reset) with mail side effects, waitlist status, public org CRUD + member
management + org-invite accept, and rate limiting (`enforce_*_rate_limit`, device exchange).

### 5. CLI command coverage (medium)

`admin_cli.rs` covers auth, users, sysadmins, sessions, device sessions, org list/invites, user
removal. Not covered: `settings`/`waitlist` groups, org create/update/delete, member
add/remove/role, ownership transfer, user invite/release, and error paths. The
`scaffold_marker_is_ready` unit test (`ankh-cli/src/lib.rs:29`) should be replaced.

### 6. Frontend packaging (medium)

- `@ankh/ui` ships unstyled primitives with no bundled CSS.
- Packages export raw `.ts`/`.tsx` via `exports`; no `dist/` build (fine for `file:` deps).
- `auth-react.test.tsx` omits `SignupPage` and `ForgotPasswordPage`.
- `oxfmt` warns "No config found" (no `frontend/.oxfmtrc.jsonc`); `pnpm` warns about ignored
  `esbuild` build scripts.
- Node engine pinned `25.8.1` (`.node-version`); local dev ran on `v26.3.0` with warnings.

### 7. Operational and repo hygiene (low — mostly resolved)

- Initial commit exists; leaves use path/`file:` deps, so a pinned revision is not the live
  protection mechanism — the sibling gate is.
- No standalone Ankh **production** server (correct; leaves mount routers). The Stage 3
  `ankh-demo` binary is a dev/QA tool, not a deployable.
- SES mailer deferred per contracts; `DevMailer` + recording mailers exist.

## Staged Next Steps

Prioritized work that leaves the system — and both leaves — consistent at each step.

### Stage 1: Cross-repo safety net and contract truth-up (do first)

Goal: never break a leaf silently again, and make the contract reflect reality.

1. [x] Correct `docs/contracts.md` (the "Current leaf consumption" block at line ~24): document
       that both leaves already path-dep the Ankh crates and `file:`-dep the `@ankh/*` packages,
       and add a short "changing a consumed API" checklist (update both leaves in the same
       change; run the sibling gate; regenerate leaf TypeScript).
2. [x] Add `cargo xtask check-siblings`: for each of `../restless` and `../verber-web` that
       exists, run its **full** gate against the current Ankh working tree —
       `cargo xtask tidy` (which includes generated-`generated.d.ts` freshness) + `cargo xtask
       test`, and a frontend `pnpm install` + lint/build/test so `file:`-linked `@ankh/*` changes
       are exercised. Skip cleanly (with a logged note) when a sibling is absent.
3. [x] Document in `DEV.md` that any change to a consumed surface must pass `check-siblings`
       before commit, and that Ankh DTO changes force leaf TypeScript regeneration.

### Stage 2: Make the local test gate reliable and honest

Goal: `cargo xtask test` runs reliably and exercises the DB-backed coverage developers rely on.
Testing is local-only; there is no CI to fall back on.

1. [x] Make `cargo xtask test` ensure Postgres is reachable first — start it via the `db start`
       path if not running, or fail with a clear message pointing to `DEV.md`. This fixes the
       non-ignored `admin_cli` test failing on a fresh machine.
2. [x] Un-ignore the eleven Postgres-backed tests once Postgres is guaranteed by step 1, so the
       default gate actually runs them. Leave `ts.rs:207` gated as-is (it rewrites checked-in
       files). Optionally split a heavier set behind `cargo xtask test --integration`.
3. [x] Delete the stale `.github/workflows/ci.yml`; the project relies on local gates only.
       Document the local testing flow (`db start` → `tidy` → `test`) in `DEV.md`.
4. [x] Switch `cargo xtask test` from `cargo test --all-features` to `cargo nextest run` per the
       standard Rust setup (`rust-tend`), which the `rust-test` skill also prefers; `cargo-nextest`
       is already installed. Note nextest does not run doctests, so add a separate
       `cargo test --doc` step if/when doctests exist. This is the only deviation from the
       standard project setup — workspace/edition/lints, `clippy.toml`, and `rustfmt-nightly.toml`
       all already match (`xtask tidy`'s check-mode clippy instead of `--fix` is a deliberate,
       allowed gate variant and needs no change).

### Stage 3: Local demo server (`ankh-demo`)

Goal: run the whole stack live, in isolation from the leaves, for manual QA and end-to-end
smoke. All steps are additive — no consumed API changes.

1. [x] Add a binary crate `crates/ankh-demo` that:
       - builds an `AnkhDbPool` against the local Postgres (reuse the `db start` port/paths),
       - constructs `MailState` with `DevMailer` writing artifacts under `tmp/mail`, using a
         **branding/public base URL that matches the chosen HTTP port** (so verification/reset
         links are clickable — do not reuse the testdata fixture's fixed `52700`),
       - assembles `AnkhWebState` with `NoopProductHooks` + a logging `AdminAudit`, and an
         `AnkhWebConfig` whose `CookieConfig.secure = false` (the default is `true`, which
         silently breaks cookie auth over HTTP localhost),
       - merges `ankh_web::router()` + `admin_router()`, layers `Extension(state)`, and serves
         via `axum::serve` + `tokio`. **`ankh-demo` must enable tokio features
         `rt-multi-thread`, `net`, `macros`, and `signal`** (the workspace tokio dep is
         `default-features = false`; `ankh-web` only enables `macros` + `rt`).
       - selects/reports its port: default 8080 but detect a busy port and either auto-increment
         or fail with a clear message (8080 collides more readily than the DB ports 55432/3/5).
2. [x] Add `--seed`: call `ankh_testdata::seed_identity_rows` (not `seed_identities`, which
       returns `()`), then print the seeded login credentials from the `ALICE`/`BOB` fixtures
       (plaintext `email`/`password`) and the returned `SeededIdentityIds`. Define `--reset`
       semantics explicitly (recreate DB via the `db start --recreate` path so no stale rows).
3. [x] Add a `cargo xtask demo` command (sibling to `db`/`test`/`tidy`, reusing the
       `ankh_xtask::web`/`frontend` helpers where useful) that ensures Postgres is up then
       launches `ankh-demo`. Flags `--port`, `--seed`, `--reset`. Document in `DEV.md`.
4. [x] Decide how to exercise the **auth UI** against the demo. `@ankh/auth-react` issues
       same-origin relative fetches (`frontend/packages/auth-react/src/api.ts`), so a
       backend-only server does not drive the UI. Either (a) add a tiny Ankh demo Vite app that
       mounts the auth components and proxies `/api` to the demo port, or (b) document pointing a
       leaf frontend dev server's proxy at the demo. Pick one and note it.
5. [x] Add one cross-layer smoke (gated like Stage 2 integration tests): boot `ankh-demo` on an
       ephemeral port, sign up + log in over HTTP, run a device authorize/token-exchange round
       trip, drive one admin route through `ankh-cli`, and assert a `DevMailer` artifact appears.
       (Implemented in-process via `tower::oneshot`: login + non-`Secure` cookie, password-reset
       `DevMailer` artifact, and an admin-router login; not a spawned process or device round trip.)

### Stage 4: Close `ankh-db` integration holes

Goal: DB-layer regressions caught without going through HTTP.

1. [x] `with_fresh_db` coverage for: signup/login session lifecycle, org create with owner
       constraint, device grant create/consume/expiry, device session revoke, token kinds,
       namespace suspend/reinstate generation bump.
2. [x] Pagination/cursor round-trip tests for at least one list method per entity type.
3. [x] `initialize()` idempotency and `apply_schema()` safety on a fresh DB.

### Stage 5: Complete public API router tests

Goal: every Public Routes entry in `docs/contracts.md` has a happy-path test. (The Stage 3 demo
is a useful scratchpad for authoring these.)

1. [x] `with_seeded_harness` tests for signup (active + waitlisted), verify-email,
       resend-verification, forgot/validate/reset-password (assert `RecordingMailer` captures).
2. [x] Org route tests: create, invite member, accept org-invite token, leave, remove/cancel.
3. [x] Rate-limit tests: `too_many_requests` on login and device token exchange.
4. [x] Un-ignore all router tests once Stage 2 Postgres gating is in place.

### Stage 6: Finish CLI and frontend coverage

1. [x] Extend `admin_cli.rs` to invoke `settings`/`waitlist`, org create/update/delete, member
       add/role/transfer, and user invite/release.
2. [x] Replace `scaffold_marker_is_ready` with a test that `ProductInfo` config paths resolve.
3. [x] Vitest coverage for `SignupPage` and `ForgotPasswordPage`.
4. [x] Add `frontend/.oxfmtrc.jsonc` (or `oxfmt --init`); run `pnpm approve-builds` for
       `esbuild` and commit the result. Validate both via `check-siblings` (frontend changes
       touch `file:`-linked packages). (Added `.oxfmtrc.json` via `oxfmt --init`; approved esbuild
       via `onlyBuiltDependencies` in `pnpm-workspace.yaml`.)

### Stage 7: Documentation refresh

1. [x] Rewrite `README.md`: drop "scaffolded/empty crates"; add current stage summary, quick
       start (`cargo xtask db start`, `cargo xtask demo`, `cargo xtask test`), and link to
       `docs/contracts.md`.
2. [x] Update stale inline comments (`ankh-web/src/lib.rs:13`, `ankh-web/src/test_support.rs`).
3. [x] Add `CONTRIBUTING.md` with the sibling-checkout layout and the consumed-API change
       checklist.

### Stage 8: Ongoing leaf migration

Per-surface migration of leaf routes onto Ankh (e.g. Restless device routes) is continuous,
leaf-side work tracked in the leaf repositories, not an Ankh checklist item — removed from this
plan. The supporting pieces it depends on are in place: the Stage 3 demo to validate against and
the Stage 1 `check-siblings` gate to run per change.

1. [x] Decide `@ankh/ui` styling strategy. **Shipped** a minimal unbranded `ankh.css` (importable
       via `@ankh/ui/ankh.css`) whose `--ankh-*` CSS variables a leaf can override to rebrand;
       leaves may also supply their own CSS. Documented in `README.md`.

### Stage 9: API design cleanup (improvement palette)

A ruskel pass over all nine crates produced this palette. Each item was then checked against
actual usage in `../restless` and `../verber-web`. That ground-truth check **resolved the whole
palette**: the items below are either applied, or pruned because they target a deliberate leaf
extension point, are a non-defect, or are low-leverage churn relative to their blast radius across
the heavily-coupled leaves. (Anything genuinely worth doing later belongs in a dedicated,
`check-siblings`-gated cross-repo sweep.)

**Applied**
- [x] `ankh_web::auth::now_epoch_secs()` — unused by either leaf; made `pub(crate)` (zero ripple).
- [x] B5 (duplicate free fns): documented `ankh_names::{validate_namespace_name,
      is_reserved_namespace_name}` and `ankh_mail::{read_latest_dev_mail, read_all_dev_mail}` as
      intentional default-policy / standalone conveniences alongside the `NamePolicy` / `DevMailer`
      methods (the plan's accepted "document as a thin convenience" resolution).

**Pruned — invalid (targets a real leaf extension point), verified by usage**
- `ankh_db::AnkhDb::client()` / `client_mut()` — `restless-db` runs product SQL over the shared
  connection through these; intentional. Keep public.
- `hash_secret`, `make_cursor`, `ParsedCursor`, `waitlist_status_*` — `restless-db` consumes them
  for product token hashing and cursor pagination. Shared building blocks; keep public.
- `_unchecked` methods (`create_org_unchecked`, …) — `restless-db` and `restless/testdata` use
  `create_org_unchecked` to seed orgs with otherwise-reserved names. Legitimate; the `_unchecked`
  suffix already signals intent. Keep.
- Test doubles (`FakeAuditSink`/`FakeHookRecorder`) behind a `test-support` feature — `restless`
  tests import `ankh_web::FakeAuditSink` directly; gating them adds leaf friction for no real gain.

**Pruned — non-defect (current design is correct)**
- `ankh_db::consume_token(&mut self)` — required: it opens a `client.transaction()`, which borrows
  the client mutably. Correct as-is.
- ID typing: the `&str` parameters (`get_session`, `delete_session`, token lookups) take opaque
  string **tokens**, not IDs; the actual entity IDs already use newtypes (`UserId`, `OrgId`, …).
  No inconsistency to fix.

**Pruned — low-leverage churn given leaf coupling**
- Typed `NameError` / consolidating the `ankh_web::errors` `&'static str` catalog — the messages
  are only ever rendered to users, never matched on; `restless-names` re-exports `validate_name_format`
  and mirrors the `&'static str` shape, so typing it ripples widely for no consumer that branches.
- Unify `AdminError` / `ApiError` — they are distinct admin vs public envelopes (different fields:
  `request_id`, the rate-limit name), and `restless-web` consumes both across many files. High
  blast radius, low payoff.
- `ankh_web::auth` free error helpers (`bad_request`/…) — internal-only convenience over the static
  error consts (~58 call sites), not part of the consumed surface; removing them is pure churn.
- Pool-constructor builder, schema-lifecycle dedup, invites-vs-tokens fold, flat-vs-module
  re-export convention, typed `XtaskError` — all leaf-consumed and/or stylistic; not worth a
  cross-repo break right now.

## Verification Checklist

These pass in a clean checkout (validated):

```sh
cargo xtask db start
cargo xtask tidy
cargo xtask test          # reliable; former ignored DB tests now run
cargo xtask demo --seed   # boots the live stack; prints seeded creds; Ctrl-C to stop
cd frontend && pnpm test
```

Because both leaves consume Ankh today, any Ankh change must also pass the sibling gate before
commit (automated by Stage 1's `cargo xtask check-siblings`):

```sh
(cd ../restless   && cargo xtask tidy && cargo xtask test && (cd frontend && pnpm install && pnpm test))
(cd ../verber-web && cargo xtask tidy && cargo xtask test && (cd frontend && pnpm install && pnpm test))
```

The leaf `tidy` step is what catches stale `generated.d.ts` after an Ankh DTO change.

## Deferred (per contracts, not blocking extraction)

- SES `Mailer` adapter and AWS configuration.
- Standalone Ankh **production** deployable server (leaves mount routers; `ankh-demo` is a
  dev/QA tool, not a deployment).
- npm-publishable frontend package builds (`dist/` + types).
- OpenAPI or machine-readable route spec generation.
