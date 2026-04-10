use clap::Parser;
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
        Engine::new()
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
        if let Err(e) = store.save(&engine.snapshot()) {
            eprintln!("geo-stream: failed to save snapshot: {e}");
            std::process::exit(1);
        }
    }
}
