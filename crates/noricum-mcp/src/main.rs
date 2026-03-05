use std::process;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "noricum_mcp=info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    eprintln!(
        "noricum-mcp-server v{} starting (stdio transport)",
        noricum_mcp::version()
    );

    if let Err(e) = noricum_mcp::server::run_server() {
        eprintln!("fatal: {e}");
        process::exit(1);
    }
}
