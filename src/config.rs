use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub connection_string: Option<String>,
    pub max_connections: Option<u32>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            database: DatabaseConfig::default(),
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 3001,
        }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            connection_string: None,
            max_connections: Some(20),
        }
    }
}

impl AppConfig {
    /// Load configuration from environment variables and config file
    pub fn load() -> anyhow::Result<Self> {
        let mut config = config::Config::builder();
        
        // Add default configuration
        config = config.add_source(config::Config::try_from(&AppConfig::default())?);
        
        // Add config file if it exists
        config = config.add_source(
            config::File::with_name("config").required(false)
        );
        
        // Add environment variables with prefix "OAT_"
        config = config.add_source(
            config::Environment::with_prefix("OAT")
                .separator("_")
                .prefix_separator("_")
        );

        let config = config.build()?;
        let app_config: AppConfig = config.try_deserialize()?;
        
        Ok(app_config)
    }

    /// Get the database URL from config or environment
    pub fn database_url(&self) -> anyhow::Result<String> {
        if let Some(connection_string) = &self.database.connection_string {
            return Ok(connection_string.clone());
        }

        // Fall back to environment variable
        if let Ok(url) = std::env::var("DATABASE_URL") {
            return Ok(url);
        }

        // Default for local development
        Ok("postgres://postgres:password@localhost:5432/oatdb".to_string())
    }

    /// Get the server bind address
    pub fn server_address(&self) -> String {
        format!("{}:{}", self.server.host, self.server.port)
    }
}