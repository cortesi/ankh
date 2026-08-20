# @ankh/demo-web

A minimal, unbranded single-page app that assembles the shared
`@ankh/auth-react` pages (login, signup, password reset, email verification,
org invites) and panels (org members, device sessions) against the local
`ankh-demo` backend. It exists so the whole identity stack can be exercised in
a browser in isolation from the leaf products.

## Running

The simplest path is the workspace task, which builds this app and serves it
from the Rust demo server at a single origin:

```sh
cargo xtask demo --seed
# open http://127.0.0.1:8080
```

For iterating on the UI with hot-reload, run the demo backend and the Vite dev
server side by side:

```sh
cargo xtask demo --seed --no-frontend   # backend + API on :8080
pnpm --filter @ankh/demo-web dev        # UI on :5173, proxies /api and /admin to :8080
```

`vite build` emits into `../../../crates/ankh-demo/dist`, which `ankh-demo`
serves as the SPA fallback. Override the proxy target with
`ANKH_DEMO_BACKEND_URL`.
