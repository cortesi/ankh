# Ankh Extraction Contracts

This document records the greenfield contracts for the shared identity split.
Restless and Verber Web are both pre-production consumers, so Ankh is the source
of truth for the shared surface and no compatibility shims are required.

## Sibling Checkout Layout

During path-dependency development, all three repositories are sibling
checkouts:

```text
../ankh
../restless
../verber-web
```

Both leaves already consume Ankh through the sibling layout. A leaf checkout
that consumes Ankh without the sibling checkout is not a supported development
shape during extraction.

Current leaf consumption:

- Restless: path-deps `ankh-constants`, `ankh-names`, `ankh-types`, `ankh-db`,
  `ankh-mail`, `ankh-web`, `ankh-cli`, `ankh-testdata`, and `ankh-xtask`; its
  `xtask` crate consumes the `ankh_xtask::command` library module. Frontend
  `packages/web` `file:`-deps `@ankh/types`, `@ankh/ui`, and `@ankh/auth-react`.
- Verber Web: path-deps the same Rust crate set (constants, names, types, db,
  mail, web, cli, testdata, xtask); its `xtask` consumes the `ankh_xtask`
  `admin`, `web`, `frontend`, and `command` library modules. Frontend
  `packages/web` `file:`-deps `@ankh/types`, `@ankh/ui`, and `@ankh/auth-react`.

Rust path dependencies use sibling paths such as `../ankh/crates/ankh-types`
from a leaf workspace. Frontend package references use sibling `file:`
dependencies such as `file:../../../../ankh/frontend/packages/auth-react` from
packages under a leaf `frontend/packages/*` directory.

### Changing a consumed API

Because both leaves build against these crates and packages, any change to a
consumed surface is a breaking change that must be made in the same step as the
matching leaf edits:

1. Make the change in Ankh and update `../restless` and `../verber-web` together.
2. Ankh DTO changes (`ankh-types`) force regeneration of each leaf's checked-in
   `generated.d.ts`, since each leaf prepends Ankh's shared declarations to its
   own; run each leaf's `cargo xtask tidy` to catch staleness.
3. Validate with `cargo xtask check-siblings` from Ankh before committing.

The sequential smoke gate after leaf consumption begins is:

```sh
(cd ../ankh && cargo xtask test)
(cd ../verber-web && cargo xtask test)
(cd ../restless && cargo xtask test)
```

## Shared Rust Crates

- `ankh-types` owns shared ID newtypes, identity DTOs, org DTOs, sysadmin DTOs,
  session DTOs, and device authorization DTOs.
- `ankh-names` owns namespace normalization, syntax validation, and composed
  shared plus product-specific reserved-name policies.
- `ankh-constants` owns identity, session, mail, device authorization, invite,
  password, rate-limit, and admin pagination defaults.
- `ankh-db` owns the canonical identity schema, shared Postgres setup, hashing,
  pagination, identity models, and concrete `AnkhDb` methods.
- `ankh-mail` owns provider-agnostic transactional mail, dev mail, recording
  mail, catalogs, branding, rendering, and readback helpers.
- `ankh-web` owns shared Axum extractors, auth/org/device/admin services,
  routers, hook dispatch, error envelopes, rate limits, and audit seams.
- `ankh-cli` owns common admin client plumbing, config/profile handling,
  rendering, errors, and shared command handlers.
- `ankh-testdata` owns deterministic shared identities, orgs, invites, mail
  helpers, seed helpers, and test harness utilities.
- `ankh-xtask` owns Ankh automation and later shared dev-kit helpers.

## Shared Frontend Packages

- `@ankh/types` contains generated TypeScript declarations for shared DTOs.
- `@ankh/ui` contains unbranded auth/org/device primitives only.
- `@ankh/auth-react` contains common auth, org, invite, and device-session UI
  flows with product customization slots.

## Shared Database Tables

Ankh owns:

- `ankh_schema_version`
- `ankh_settings`
- `namespaces`
- `users`
- `sessions`
- `device_auth_grants`
- `device_sessions`
- `tokens`
- `invites`
- `sysadmins`
- `sysadmin_tokens`
- `organizations`
- `org_members`
- `org_invites`

Leaf schemas keep their own `schema_version` tables and product resources.
Leaves bootstrap in this order: `AnkhDb::apply_schema`, leaf
`apply_product_schema`, `AnkhDb::initialize`, then leaf `initialize`. Ankh never
writes leaf schema versions, and leaves never write `ankh_schema_version`.

## Device Session Contract

`device_auth_grants` store PKCE S256 loopback grants. Raw grant codes are
returned once, hashed at rest, attempt-limited, and consumed exactly once.

`device_sessions` store bearer device sessions. Raw tokens are 32 random bytes
encoded with base64url, returned once, and never stored. Validation rejects
missing, expired, or revoked sessions and touches `last_used_at` on success.

`DevicePlatform` is validated in Rust and serialized as text. Known values are
`macos`, `windows`, `linux`, `web`, and `other`; unknown database values become
`Other(String)`.

Password reset and user deletion revoke web sessions, device sessions, and
outstanding identity tokens inside Ankh. Any path that revokes device sessions
dispatches `on_device_sessions_revoked` after the database mutation succeeds.

## Restless Device Route Migration

| Old Restless route | New Ankh route |
| --- | --- |
| `GET /player/authorize` | `GET /api/v1/device/authorize` |
| `POST /api/player/auth/exchange` | `POST /api/v1/device/token` |
| `GET /api/v1/player-sessions` | `GET /api/v1/device-sessions` |
| `DELETE /api/v1/player-sessions/{id}` | `DELETE /api/v1/device-sessions/{id}` |
| `POST /api/v1/player-token` | `POST /api/v1/device-sessions` |
| `GET /api/player/sessions` | deleted; use `GET /api/v1/device-sessions` |
| `DELETE /api/player/sessions/{id}` | deleted; use `DELETE /api/v1/device-sessions/{id}` |
| `GET /admin/v1/player-sessions` | `GET /admin/v1/device-sessions` |
| `POST /admin/v1/player-sessions/{id}/revoke` | `POST /admin/v1/device-sessions/{id}/revoke` |

## Public Routes

- `/api/v1/auth/signup`
- `/api/v1/auth/login`
- `/api/v1/auth/logout`
- `/api/v1/auth/me`
- `/api/v1/auth/waitlist-status`
- `/api/v1/auth/verify-email`
- `/api/v1/auth/resend-verification`
- `/api/v1/auth/forgot-password`
- `/api/v1/auth/validate-reset-token`
- `/api/v1/auth/reset-password`
- `/api/v1/orgs`
- `/api/v1/orgs/{id}`
- `/api/v1/orgs/{id}/membership`
- `/api/v1/orgs/{id}/leave`
- `/api/v1/orgs/{id}/members`
- `/api/v1/orgs/{id}/invites`
- `/api/v1/orgs/{id}/members/{member_id}`
- `/api/v1/orgs/{id}/invites/{invite_id}`
- `/api/v1/org-invites/{token}`
- `/api/v1/org-invites/{token}/accept`
- `GET /api/v1/device-sessions`
- `POST /api/v1/device-sessions`
- `DELETE /api/v1/device-sessions/{id}`
- `GET /api/v1/device/authorize`
- `POST /api/v1/device/token`

## Admin Routes

- `POST /admin/v1/auth/login`
- `GET /admin/v1/sysadmins`
- `GET /admin/v1/sysadmins/me`
- `GET /admin/v1/users`
- `GET /admin/v1/users/{id}`
- `DELETE /admin/v1/users/{id}`
- `POST /admin/v1/users/release`
- `POST /admin/v1/users/invite`
- `GET /admin/v1/sessions`
- `POST /admin/v1/sessions/{id}/revoke`
- `GET /admin/v1/device-sessions`
- `POST /admin/v1/device-sessions/{id}/revoke`
- `GET /admin/v1/settings`
- `POST /admin/v1/settings/waitlist`
- `GET /admin/v1/orgs`
- `POST /admin/v1/orgs`
- `GET /admin/v1/orgs/{id}`
- `PATCH /admin/v1/orgs/{id}`
- `DELETE /admin/v1/orgs/{id}`
- `GET /admin/v1/orgs/{id}/members`
- `POST /admin/v1/orgs/{id}/members`
- `DELETE /admin/v1/orgs/{id}/members/{user_id}`
- `PATCH /admin/v1/orgs/{id}/members/{user_id}`
- `POST /admin/v1/orgs/{id}/transfer`
- `GET /admin/v1/orgs/{id}/invites`
- `POST /admin/v1/orgs/{id}/invites`
- `DELETE /admin/v1/orgs/{id}/invites/{invite_id}`
- `POST /admin/v1/namespaces/{id}/suspend`
- `POST /admin/v1/namespaces/{id}/reinstate`

## CLI Command Groups

`rcli` and `vcli` keep their leaf binary names. Common Ankh command groups are
top-level variants beside product command groups:

- auth
- users
- web sessions
- device sessions
- sysadmins
- settings
- orgs
- members
- invites

Shared global flags are `--config`, `--profile`, `--base-url`, `--format`,
`--quiet`, `--verbose`, and `--trace-id`.

## Mail Contract

Ankh starts with provider-agnostic dev-mode delivery. The initial API includes
`Email`, async `Mailer`, `DevMailer`, `RecordingMailer`, `MailCatalog`, and
`MailBranding`. `Email` has `to`, `from`, `subject`, `text_body`, and optional
`html_body`; it does not include SES-specific fields.

The shared catalog covers verification, password reset, waitlist invite,
waitlist release, and org invite mail. Templates use `{app_name}` rather than
hardcoded product names. Leaves configure app name, public base URL, sender,
support address, output directory, and any real copy overrides.

SES delivery is a post-Stage-Ten adapter over the async `Mailer` trait. The
initial mail contract deliberately excludes AWS configuration, SES-specific
metadata, bounce handling, attachments, and batching.

## Harness Contracts

Ankh tests run without depending on Restless or Verber. Harness code uses
config structs and injected clocks, UUID generators, token generators, mail
sinks, audit sinks, and hook recorders. Tests do not use sleeps to wait for
side effects.

Ankh local Postgres defaults to port `55435` and `../ankh/tmp/pgdata`.
`ankh_db::test_support::with_fresh_db` creates a unique database per call,
applies only Ankh schema, runs a caller-supplied seed callback, and drops the
database during teardown.

## Staged Test Gates

| Stage | `cargo xtask test` in `../ankh` must cover |
| --- | --- |
| 1 | Workspace/crate/package smoke tests and harness trait skeletons only. |
| 2 | Type/name/constant tests and `@ankh/types` generation freshness. |
| 3 | DB integration tests and fresh database helper tests. |
| 4 | Mail rendering, `RecordingMailer`, and `DevMailer` tests. |
| 5 | In-process public web/router tests. |
| 6 | In-process admin web/router tests. |
| 7 | CLI command tests against the in-process admin router. |
| 8 | Frontend unit tests and Ankh-only web harness smoke tests. |
| 9 | `ankh-xtask` orchestration tests. |

## Leaf-Owned Product Surfaces

Restless keeps channels, soundscapes, samples, API submission tokens, listener
tokens, Cloudflare edge logic, audio/player code, blob storage, product route
mounting, product CLI commands, frontend shell, player app, side nav,
soundscape editor, brand, testdata overlays, and edge/teststream/desktop/wasm
xtask commands.

Verber Web keeps product routes, product services, product schema, `vcli`,
config paths, frontend shell, landing/vision/dashboard pages, brand assets,
theme, route targets, testdata overlays, xtask port choices, state directory
names, and product build steps.
