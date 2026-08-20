#![warn(missing_docs)]

//! Command-line entrypoint for the local Ankh demo server.

use std::{error::Error, io, process};

use ankh_db::{AnkhDbPool, create_pg_pool_with_max_size, test_support::DEFAULT_POSTGRES_PORT};
use ankh_demo::{DEMO_DATABASE, MAIL_OUT_DIR, build_app, report_seeded};
use ankh_testdata::seed_identity_rows;
use clap::Parser;
use tokio::{net::TcpListener, signal::ctrl_c};

/// Maximum number of sequential ports tried when the requested HTTP port is busy.
const PORT_SCAN_LIMIT: u16 = 20;
/// Pool size for the demo server.
const POOL_MAX_SIZE: u32 = 8;

/// Command-line options for the demo server.
#[derive(Parser)]
#[command(
    name = "ankh-demo",
    about = "Run the Ankh identity stack locally for manual QA"
)]
struct Args {
    /// HTTP port to serve on. If busy, the next free port is used.
    #[arg(long, default_value_t = 8080)]
    port: u16,
    /// Postgres port to connect to (the workspace dev instance).
    #[arg(long, default_value_t = DEFAULT_POSTGRES_PORT)]
    db_port: u16,
    /// Seed deterministic demo identities (users, sysadmin, org) before serving.
    #[arg(long)]
    seed: bool,
}

/// Program entrypoint.
#[tokio::main]
async fn main() {
    if let Err(error) = run(Args::parse()).await {
        eprintln!("ankh-demo: {error}");
        process::exit(1);
    }
}

/// Build the stack and serve until interrupted.
async fn run(args: Args) -> Result<(), Box<dyn Error>> {
    let pool = create_pg_pool_with_max_size(
        format!(
            "host=localhost port={} dbname={DEMO_DATABASE}",
            args.db_port
        ),
        POOL_MAX_SIZE,
    )?;

    if args.seed {
        seed_and_report(&pool).await?;
    }

    let listener = bind_with_discovery(args.port).await?;
    let addr = listener.local_addr()?;
    let app = build_app(pool, &format!("http://{addr}"), MAIL_OUT_DIR)?;

    println!("Ankh demo listening on http://{addr} (UI + API; mail artifacts in {MAIL_OUT_DIR}/)");
    println!("Press Ctrl-C to stop.");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

/// Seed deterministic demo identities and print the credentials they create.
async fn seed_and_report(pool: &AnkhDbPool) -> Result<(), Box<dyn Error>> {
    let mut db = pool.get().await?;
    let ids = seed_identity_rows(&mut db).await?;
    report_seeded(&ids);
    Ok(())
}

/// Bind the requested port, scanning forward to the next free port if it is in use.
async fn bind_with_discovery(port: u16) -> Result<TcpListener, Box<dyn Error>> {
    for candidate in port..port.saturating_add(PORT_SCAN_LIMIT) {
        match TcpListener::bind(("127.0.0.1", candidate)).await {
            Ok(listener) => {
                if candidate != port {
                    println!("Port {port} busy; using {candidate} instead.");
                }
                return Ok(listener);
            }
            Err(error) if error.kind() == io::ErrorKind::AddrInUse => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(format!(
        "no free port found in range {port}..{} (is another demo already running?)",
        port.saturating_add(PORT_SCAN_LIMIT)
    )
    .into())
}

/// Resolve when the process receives Ctrl-C, triggering graceful shutdown.
async fn shutdown_signal() {
    if let Err(error) = ctrl_c().await {
        eprintln!("ankh-demo: failed to listen for shutdown signal: {error}");
    }
}
