use sqlx::ConnectOptions; // Required for the .log_statements() methods
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::str::FromStr;
use std::time::Duration;

// src/main.rs
use opentelemetry::global;
use tracing::{error, info};

mod config;
mod middleware;
mod routes;
mod state;
mod telemetry;

pub mod error;

use middleware::RouterExt;

use crate::state::AppMetrics;
use crate::{config::AppConfig, state::AppState};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();

    let config = AppConfig::from_env();

    let telemetry_guards =
        telemetry::init_telemetry(&config.service_name, config.is_prod, config.otlp.as_ref());

    tracing::info!(
        "Starting service '{}' in {} mode",
        config.service_name,
        if config.is_prod {
            "production"
        } else {
            "development"
        }
    );

    let meter = global::meter("axum-test");
    let metrics = AppMetrics::new(&meter);
    let db_options = PgConnectOptions::from_str(&config.db_url)?
        // Push normal queries down to DEBUG (blocked by your EnvFilter)
        .log_statements(tracing::log::LevelFilter::Debug)
        // Escalate queries taking >1s to WARN (allowed by your EnvFilter)
        .log_slow_statements(tracing::log::LevelFilter::Warn, Duration::from_secs(1));
    //TODO: SLOW STATEMENTS REQUIRE PG STAT CONFIG?

    let pool = PgPoolOptions::new()
        .max_connections(5) //TODO: MAGIC NUMBER
        .connect_with(db_options)
        .await?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .inspect(|_| info!("Database migrations executed successfully"))
        .inspect_err(|e| error!(error = %e, "Database migration failed"))?;

    let nats_client = async_nats::connect(config.nats_url).await?;
    let js = async_nats::jetstream::new(nats_client);

    let state = AppState {
        db: pool,
        metrics,
        js,
    };

    let app = routes::create_router(config.is_prod)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::track_metrics,
        ))
        .with_base_middleware()
        .with_state(state);

    let address = "0.0.0.0";
    let port = 3000;
    let listener = tokio::net::TcpListener::bind((address, port)).await?;
    info!("Server listening on {address}:{port}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    telemetry_guards.shutdown();

    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install Ctrl+C signal handler");
    info!("Shutdown signal received, draining connections...");
}
