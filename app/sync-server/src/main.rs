use kitlane_sync_server::{Service, router};
use std::{
    env,
    net::{Ipv4Addr, SocketAddrV4},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let owner = env::var("KITLANE_OWNER_TOKEN").map_err(
        |_| "KITLANE_OWNER_TOKEN must contain 32 random bytes encoded as 64 hex characters",
    )?;
    let path = env::var("KITLANE_SYNC_DB").unwrap_or_else(|_| "kitlane-sync.redb".into());
    let port = env::var("KITLANE_SYNC_PORT")
        .unwrap_or_else(|_| "8787".into())
        .parse::<u16>()?;
    let service = Service::open(path, &owner)
        .map_err(|_| "Service initialization failed; check owner configuration and database")?;
    let listener =
        tokio::net::TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)).await?;
    eprintln!(
        "Kitlane local sync prototype listening on {}",
        listener.local_addr()?
    );
    axum::serve(listener, router(service))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
