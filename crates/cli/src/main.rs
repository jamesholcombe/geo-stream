use clap::Parser;
#[cfg(feature = "redis-backend")]
use engine::EngineOptions;
use engine::{Engine, FileSnapshotStore, SnapshotStore};
use std::io;
use std::path::PathBuf;
use stdin_stdout::{run, RunConfig};

#[derive(Parser, Debug)]
#[command(
    name = "geo-stream",
    about = "Geospatial stream engine — NDJSON stdin/stdout"
)]
struct Args {
    /// Point updates per engine `process_batch`. Use `0` to buffer all updates until EOF (one batch).
    #[arg(long, default_value_t = 1)]
    batch_size: usize,

    /// Path to a snapshot file to restore engine state from before processing begins.
    #[arg(long)]
    restore_from: Option<PathBuf>,

    /// Path to write an engine state snapshot after processing completes.
    #[arg(long)]
    snapshot_file: Option<PathBuf>,
}

/// Build an engine using the backing store selected by `GEO_EVENTS_STATE_BACKEND`.
///
/// Supported values:
/// - `memory` (default) — in-process HashMap, no persistence.
/// - `redis://…` / `rediss://…` — Redis via `state-redis` (requires `--features redis-backend`).
fn build_engine() -> Engine {
    let backend = std::env::var("GEO_EVENTS_STATE_BACKEND").unwrap_or_default();
    let url = backend.trim();

    if url.starts_with("redis://") || url.starts_with("rediss://") {
        build_redis_engine(url)
    } else {
        Engine::new()
    }
}

#[cfg(feature = "redis-backend")]
fn build_redis_engine(url: &str) -> Engine {
    match state_redis::RedisStateStore::new(url, "geo-events") {
        Ok(store) => Engine::with_store(store, EngineOptions::default()),
        Err(e) => {
            eprintln!("geo-stream: failed to connect to Redis ({url}): {e}");
            std::process::exit(1);
        }
    }
}

#[cfg(not(feature = "redis-backend"))]
fn build_redis_engine(url: &str) -> Engine {
    eprintln!(
        "geo-stream: GEO_EVENTS_STATE_BACKEND={url} requires the `redis-backend` feature.\n\
         Rebuild with: cargo build --features redis-backend"
    );
    std::process::exit(1);
}

fn main() {
    let args = Args::parse();

    let mut engine = if let Some(path) = &args.restore_from {
        let store = FileSnapshotStore { path: path.clone() };
        match store.load() {
            Ok(Some(snap)) => match Engine::restore_from_snapshot(snap) {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("geo-stream: failed to restore snapshot: {e}");
                    std::process::exit(1);
                }
            },
            Ok(None) => {
                eprintln!("geo-stream: snapshot file not found: {}", path.display());
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!("geo-stream: failed to load snapshot: {e}");
                std::process::exit(1);
            }
        }
    } else {
        build_engine()
    };

    let stdin = io::stdin().lock();
    let stdout = io::stdout();
    let stderr = io::stderr();
    let config = RunConfig {
        batch_size: args.batch_size,
    };
    if let Err(e) = run(&mut engine, stdin, stdout, stderr, config) {
        eprintln!("geo-stream: {e}");
        std::process::exit(1);
    }

    if let Some(path) = &args.snapshot_file {
        let store = FileSnapshotStore { path: path.clone() };
        let snap = match engine.snapshot() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("geo-stream: failed to capture snapshot: {e}");
                std::process::exit(1);
            }
        };
        if let Err(e) = store.save(&snap) {
            eprintln!("geo-stream: failed to save snapshot: {e}");
            std::process::exit(1);
        }
    }
}
