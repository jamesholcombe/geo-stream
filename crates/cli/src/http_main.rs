use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    let addr: SocketAddr = ([0, 0, 0, 0], 8080).into();
    eprintln!("geo-stream-http listening on http://{addr}");
    if let Err(e) = http_adapter::run_server(addr).await {
        eprintln!("geo-stream-http: {e}");
        std::process::exit(1);
    }
}
