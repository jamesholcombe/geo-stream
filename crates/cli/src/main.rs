use clap::Parser;
use engine::Engine;
use std::io;
use stdin_stdout::{run, RunConfig};

#[derive(Parser, Debug)]
#[command(
    name = "geo-stream",
    about = "Geospatial stream engine — NDJSON stdin/stdout"
)]
struct Args {
    /// Point updates per engine ingest. Use `0` to buffer all updates until EOF (one ingest).
    #[arg(long, default_value_t = 1)]
    batch_size: usize,
}

fn main() {
    let args = Args::parse();
    let mut engine = Engine::new();
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
}
