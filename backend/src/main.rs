mod api;
mod config;
mod control;
mod db;
mod reading_cache;
mod sensors;
mod tuya;

use anyhow::Result;
use std::time::Duration;
use tokio::{net::TcpListener, signal, time};
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::{
    config::Config,
    control::ControlService,
    reading_cache::ReadingCache,
    sensors::SensorService,
    tuya::TuyaClient,
};

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env (ignore error if file absent — env vars may be set externally)
    let _ = dotenvy::dotenv();

    // Initialise tracing
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    // Load config
    let config = Config::from_env()?;

    // Connect to DB and run migrations
    let pool = db::create_pool(&config.database_url).await?;
    db::run_migrations(&pool).await?;
    info!("Database ready");

    // Shared in-memory cache of latest readings per device
    let cache = ReadingCache::new();

    // Build shared Tuya client
    let tuya = TuyaClient::new(&config);

    // Spawn sensor-polling task
    {
        let pool = pool.clone();
        let tuya = tuya.clone();
        let cache = cache.clone();
        let device_ids = config.tuya_device_ids.clone();
        let interval = Duration::from_secs(config.poll_interval_secs);

        tokio::spawn(async move {
            let service = SensorService::new(pool, tuya, cache);
            let mut ticker = time::interval(interval);
            info!(interval_secs = interval.as_secs(), "Sensor polling loop started");

            loop {
                ticker.tick().await;
                for device_id in &device_ids {
                    if let Err(e) = service.fetch_and_persist(device_id).await {
                        tracing::error!(device_id = %device_id, error = %e, "Failed to fetch sensor reading");
                    }
                }
            }
        });
    }

    // Spawn control loop task — shares the same cache, no DB queries needed
    {
        let control = ControlService::new(tuya, cache, config.control_interval_secs);
        tokio::spawn(control.run());
    }

    // Start HTTP server
    let addr = format!("{}:{}", config.server_host, config.server_port);
    let listener = TcpListener::bind(&addr).await?;
    info!(addr = %addr, "HTTP server listening");

    axum::serve(listener, api::router(pool))
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("Shutdown signal received");
}
