#![warn(missing_docs)]

//! Library backing the local Ankh demo server.
//!
//! The demo wires the public and admin routers to a Postgres pool and a [`DevMailer`], and serves
//! the built `@ankh/demo-web` single-page app as the fallback, so the whole identity stack can be
//! exercised in isolation from the leaf products — in a browser, by the `ankh-cli` admin client,
//! or by a frontend dev server proxied at the demo port. It is a development and QA tool, not a
//! production deployment.

use std::{error::Error, path::PathBuf};

use ankh_db::AnkhDbPool;
use ankh_mail::{DevMailer, MailBranding, MailCatalog, PublicBaseUrl};
use ankh_testdata::{ADMIN, ALICE, BOB, DEFAULT_ORG, MAIL, SeededIdentityIds};
use ankh_web::{AnkhWebConfig, AnkhWebState, CookieConfig, MailState, admin_router, router};
use axum::{Extension, Router};
use tower_http::services::{ServeDir, ServeFile};

/// Database that `cargo xtask db start` provisions and the demo connects to.
pub const DEMO_DATABASE: &str = "ankh-test";
/// Directory the demo writes [`DevMailer`] artifacts into, relative to the working directory.
pub const MAIL_OUT_DIR: &str = "tmp/mail";

/// Build the merged public + admin router wired to demo-friendly state, with the built
/// `@ankh/demo-web` single-page app served as the fallback.
///
/// `base_url` should match the address the server will actually be reached at so links in
/// captured mail are clickable; `mail_dir` receives [`DevMailer`] artifacts. Requests that match
/// no API or admin route fall through to the SPA at [`frontend_dist_dir`]; when that bundle has
/// not been built (e.g. `--no-frontend`), those requests simply 404.
pub fn build_app(
    pool: AnkhDbPool,
    base_url: &str,
    mail_dir: impl Into<PathBuf>,
) -> Result<Router, Box<dyn Error>> {
    let state = build_state(pool, base_url, mail_dir)?;
    Ok(Router::new()
        .merge(router())
        .merge(admin_router())
        .fallback_service(spa_service())
        .layer(Extension(state)))
}

/// Directory the built `@ankh/demo-web` bundle is emitted to (see its `vite.config.ts`).
pub fn frontend_dist_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("dist")
}

/// Serve the SPA bundle, falling back to `index.html` so client-side routes resolve on reload.
fn spa_service() -> ServeDir<ServeFile> {
    let dist = frontend_dist_dir();
    let index = dist.join("index.html");
    ServeDir::new(dist).fallback(ServeFile::new(index))
}

/// Assemble the shared web state with HTTP-localhost-friendly cookies and a dev mailer.
///
/// The session cookie's `Secure` attribute is cleared so cookie auth works over plain HTTP on
/// localhost (the default is `true`, which browsers drop over HTTP).
fn build_state(
    pool: AnkhDbPool,
    base_url: &str,
    mail_dir: impl Into<PathBuf>,
) -> Result<AnkhWebState, Box<dyn Error>> {
    let public_base_url = PublicBaseUrl::new(base_url)?;
    let branding = MailBranding::new("Ankh Demo", public_base_url, MAIL.sender, MAIL.support);
    let mail = MailState::new(DevMailer::new(mail_dir), MailCatalog::shared(), branding);

    let config = AnkhWebConfig {
        cookie: CookieConfig {
            secure: false,
            ..CookieConfig::default()
        },
        ..AnkhWebConfig::default()
    };
    Ok(AnkhWebState::with_config(pool, mail, config))
}

/// Print the login credentials produced by seeding.
pub fn report_seeded(ids: &SeededIdentityIds) {
    println!("Seeded demo identities:");
    println!(
        "  user (verified):   {} / {}  [{}]",
        ALICE.email, ALICE.password, ids.alice_user_id
    );
    println!(
        "  user (unverified): {} / {}  [{}]",
        BOB.email, BOB.password, ids.bob_user_id
    );
    println!("  sysadmin:          {} / {}", ADMIN.email, ADMIN.password);
    println!(
        "  org:               {}  [{}]",
        DEFAULT_ORG.name, ids.default_org_id
    );
}
