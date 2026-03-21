use clap::Parser;
use std::net::SocketAddr;

#[derive(Parser, Debug)]
#[command(name = "geo-stream-http", about = "Geospatial stream engine — HTTP JSON API")]
struct Args {
    /// Address to bind (e.g. 127.0.0.1:3000 or 0.0.0.0:8080)
    #[arg(long, default_value = "0.0.0.0:8080")]
    listen: SocketAddr,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
    eprintln!("geo-stream-http listening on http://{}", args.listen);
    if let Err(e) = http_adapter::run_server(args.listen).await {
        eprintln!("geo-stream-http: {e}");
        std::process::exit(1);
    }
}
