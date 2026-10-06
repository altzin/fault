use std::env;

#[derive(Clone, Debug)]
pub struct OtlpConfig {
    pub host: String,
    pub port: u16,
    pub org: String,
    pub auth_header: String,
}

impl OtlpConfig {
    /// Constructs the base OpenObserve OTLP URL
    pub fn base_url(&self) -> String {
        format!("http://{}:{}/api/{}", self.host, self.port, self.org)
    }
}

pub struct AppConfig {
    pub service_name: String,
    pub db_url: String,
    pub is_prod: bool,
    pub otlp: Option<OtlpConfig>,
    pub nats_url: String,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let service_name = env::var("SERVICE_NAME").expect("SERVICE_NAME must be set");
        let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        let is_prod = env::var("APP_ENV").map(|v| v == "prod").unwrap_or(false);
        let nats_url = env::var("NATS_URL").expect("NATS_URL MISSING and must be set");

        let otlp = env::var("OTEL_AUTH_HEADER").ok().map(|auth| OtlpConfig {
            host: env::var("OTEL_HOST").unwrap_or_else(|_| "localhost".to_string()),
            org: env::var("OTEL_ORG").unwrap_or_else(|_| "default".to_string()),
            port: env::var("OTEL_PORT")
                .unwrap_or_else(|_| "5080".to_string())
                .parse()
                .unwrap_or(5080),
            auth_header: auth,
        });

        Self {
            service_name,
            db_url,
            is_prod,
            otlp,
            nats_url,
        }
    }
}
