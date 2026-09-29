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

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect_with(db_options)
        .await?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .inspect(|_| info!("Database migrations executed successfully"))
        .inspect_err(|e| error!(error = %e, "Database migration failed"))?;

    let state = AppState { db: pool, metrics };

    let app = routes::create_router(config.is_prod)
        // Wrap all routes with our metrics tracker BEFORE providing the state
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::track_metrics,
        ))
        .with_base_middleware()
        // Provide the state last so it satisfies the Router<AppState> requirement
        .with_state(state);

    // 5. Server with graceful shutdown
    let address = "0.0.0.0".to_owned();
    let port = "3000";
    let conn_details = (address + ":" + port).clone();
    let listener = tokio::net::TcpListener::bind(conn_details.clone()).await?;
    info!("Server listening on {conn_details}");

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
