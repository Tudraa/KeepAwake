//! Antigravity KeepAwake Daemon Entrypoint (systemd --user).

use antigravity_keepawake::freezer::{emergency_thaw_all, install_crash_watchdog};
use antigravity_keepawake::service::KeepAwakeService;
use std::time::Duration;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Initialize logging
    tracing_subscriber::fmt::init();
    info!("Starting Antigravity KeepAwake Daemon v0.1.0...");

    // 2. Install emergency crash / panic watchdog for SIGCONT
    install_crash_watchdog();

    // 3. Instantiate unified coordinator service
    let service = KeepAwakeService::new().await;

    // 4. Export service on Session D-Bus
    let _conn = match zbus::connection::Builder::session()?
        .name("org.antigravity.KeepAwake")?
        .serve_at("/org/antigravity/KeepAwake", service.clone())?
        .build()
        .await
    {
        Ok(c) => {
            info!("Successfully registered 'org.antigravity.KeepAwake' on Session D-Bus.");
            c
        }
        Err(e) => {
            error!("Failed to register on Session D-Bus: {e}");
            return Err(e.into());
        }
    };

    // 5. Periodic tick task (every 3 seconds) for state evaluation & guardrails
    let svc_clone = service.clone();
    let tick_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3));
        loop {
            interval.tick().await;
            svc_clone.tick().await;
        }
    });

    // 6. Wait for termination signal
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received Ctrl+C / SIGINT signal. Initiating graceful shutdown...");
        }
        Err(e) = tick_handle => {
            error!("Background tick loop terminated with error: {e}");
        }
    }

    // 7. Graceful cleanup: thaw all frozen applications before exit
    emergency_thaw_all();
    info!("Antigravity KeepAwake Daemon exited cleanly.");
    Ok(())
}
