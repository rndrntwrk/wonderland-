#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("Gateway stopped: {error}");
        std::process::exit(1);
    }
}
async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config = wonderland_browser_gateway::config::GatewayConfig::from_env().await?;
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    eprintln!(
        "Wonderland browser gateway listening on {}",
        listener.local_addr()?
    );
    axum::serve(listener, wonderland_browser_gateway::app(config)?)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
