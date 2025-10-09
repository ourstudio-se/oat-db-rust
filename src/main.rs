use axum::serve;
use oat_db_rust::api::routes::create_router;
use oat_db_rust::config::AppConfig;
use oat_db_rust::seed;
use oat_db_rust::store::PostgresStore;
use std::sync::Arc;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load environment variables from .env file if it exists
    dotenvy::dotenv().ok();

    // Initialize logging with explicit filter to suppress sqlx debug logs
    use env_logger::Builder;
    use log::LevelFilter;

    Builder::new()
        .filter_level(LevelFilter::Info)      // Default to Info for everything
        .filter_module("sqlx", LevelFilter::Warn)  // Suppress sqlx Debug logs
        .init();

    println!("OAT-DB: Combinatorial Database Server");

    // Load configuration
    let config = AppConfig::load()?;
    println!(
        "Configuration loaded: server={}:{}",
        config.server.host, config.server.port
    );

    println!("Connecting to PostgreSQL...");
    let database_url = config.database_url()?;
    let postgres_store = PostgresStore::new(&database_url).await?;

    println!("Running database migrations...");
    postgres_store.migrate().await?;
    println!("Database ready with git-like schema");

    let store = Arc::new(postgres_store);

    // Load seed data for demonstration (optional)
    if std::env::var("LOAD_SEED_DATA").unwrap_or_default() == "true" {
        println!("Loading seed data...");
        seed::load_seed_data(&*store).await?;
        println!("Seed data loaded successfully");
    }

    run_server(create_router().with_state(store), &config).await?;

    Ok(())
}

async fn run_server(app: axum::Router, config: &AppConfig) -> anyhow::Result<()> {
    let bind_address = config.server_address();
    let listener = TcpListener::bind(&bind_address).await?;
    println!("OAT-DB server running on http://{}", bind_address);
    println!(
        "API documentation available at http://{}/docs",
        bind_address
    );

    serve(listener, app).await?;

    Ok(())
}
