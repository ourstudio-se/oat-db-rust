use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde_json::Value;
use sqlx::{PgPool, Row};
use std::io::{Read, Write};

/// Decompress gzip data
fn decompress_data(data: &[u8]) -> Result<Vec<u8>> {
    // Check if data is gzip-compressed by looking for gzip magic bytes (1f 8b)
    if data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b {
        // Data is gzip-compressed, decompress it
        let mut decoder = GzDecoder::new(data);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed)?;
        Ok(decompressed)
    } else {
        // Data is not compressed, return as-is
        Ok(data.to_vec())
    }
}

/// Compress data using gzip
fn compress_data(data: &[u8]) -> Result<Vec<u8>> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data)?;
    Ok(encoder.finish()?)
}

/// Migrate a DataType value
fn migrate_data_type(value: &str) -> &str {
    match value {
        "String" => "string",
        "Number" => "number",
        "Boolean" => "boolean",
        "Date" => "date",
        "Object" => "object",
        "Array" => "array",
        "StringList" => "string-list",
        other => other,
    }
}

/// Migrate a Quantifier value
fn migrate_quantifier(value: &str) -> &str {
    match value {
        "EXACTLY" => "exactly",
        "AT_LEAST" => "at-least",
        "AT_MOST" => "at-most",
        "BETWEEN" => "between",
        "ANY" => "any",
        other => other,
    }
}

/// Migrate a SelectionType value
fn migrate_selection_type(value: &str) -> &str {
    match value {
        "Manual" => "manual",
        "All" => "all",
        "Query" => "query",
        other => other,
    }
}

/// Migrate a ComparisonOp value
fn migrate_comparison_op(value: &str) -> &str {
    match value {
        "EQ" => "eq",
        "NE" => "ne",
        "GT" => "gt",
        "GTE" => "gte",
        "LT" => "lt",
        "LTE" => "lte",
        other => other,
    }
}

/// Recursively migrate enum values in a JSON value
fn migrate_json_value(value: &mut Value) {
    match value {
        Value::Object(map) => {
            // Handle DataType fields
            if let Some(data_type) = map.get("data_type").and_then(|v| v.as_str()) {
                map.insert("data_type".to_string(), Value::String(migrate_data_type(data_type).to_string()));
            }
            
            // Handle type fields (used in TypedValue within PropertyValue)
            if let Some(type_val) = map.get("type").and_then(|v| v.as_str()) {
                map.insert("type".to_string(), Value::String(migrate_data_type(type_val).to_string()));
            }
            
            // Handle Quantifier fields
            if let Some(quantifier) = map.get("quantifier").and_then(|v| v.as_str()) {
                map.insert("quantifier".to_string(), Value::String(migrate_quantifier(quantifier).to_string()));
            }
            
            // Handle SelectionType fields
            if let Some(selection) = map.get("selection").and_then(|v| v.as_str()) {
                map.insert("selection".to_string(), Value::String(migrate_selection_type(selection).to_string()));
            }
            
            // Handle ComparisonOp fields
            if let Some(op) = map.get("op").and_then(|v| v.as_str()) {
                map.insert("op".to_string(), Value::String(migrate_comparison_op(op).to_string()));
            }
            
            // Recurse into all values
            for (_, v) in map.iter_mut() {
                migrate_json_value(v);
            }
        }
        Value::Array(vec) => {
            for item in vec {
                migrate_json_value(item);
            }
        }
        _ => {}
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables
    dotenvy::dotenv().ok();
    
    // Connect to database
    let database_url = std::env::var("DATABASE_URL")
        .context("DATABASE_URL must be set")?;
    let pool = PgPool::connect(&database_url).await?;
    
    println!("Connected to database. Starting enum format migration...");
    
    // Migrate commits table
    println!("Migrating commits table...");
    let commits: Vec<(String, Vec<u8>)> = sqlx::query("SELECT hash, data FROM commits")
        .fetch_all(&pool)
        .await?
        .into_iter()
        .map(|row| (row.get::<String, _>("hash"), row.get::<Vec<u8>, _>("data")))
        .collect();
    
    let total_commits = commits.len();
    println!("Found {} commits to migrate", total_commits);
    
    for (i, (hash, data)) in commits.iter().enumerate() {
        if (i + 1) % 10 == 0 || i + 1 == total_commits {
            println!("Processing commit {}/{}: {}", i + 1, total_commits, hash);
        }
        
        // Decompress
        let decompressed = decompress_data(data)?;
        let json_str = String::from_utf8(decompressed)?;
        
        // Parse JSON
        let mut commit_data: Value = serde_json::from_str(&json_str)?;
        
        // Migrate enum values
        migrate_json_value(&mut commit_data);
        
        // Reserialize
        let new_json_str = serde_json::to_string(&commit_data)?;
        
        // Recompress
        let new_compressed = compress_data(new_json_str.as_bytes())?;
        
        // Update in database
        sqlx::query("UPDATE commits SET data = $1 WHERE hash = $2")
            .bind(&new_compressed)
            .bind(hash)
            .execute(&pool)
            .await?;
    }
    
    println!("Commits migration completed!");
    
    // Migrate working_commits table
    println!("\nMigrating working_commits table...");
    let working_commits: Vec<(String, Value)> = sqlx::query("SELECT id, schema_data FROM working_commits WHERE schema_data IS NOT NULL")
        .fetch_all(&pool)
        .await?
        .into_iter()
        .map(|row| (row.get::<String, _>("id"), row.get::<Value, _>("schema_data")))
        .collect();
    
    let total_working = working_commits.len();
    println!("Found {} working commits to migrate", total_working);
    
    for (i, (id, mut schema_data)) in working_commits.into_iter().enumerate() {
        if (i + 1) % 10 == 0 || i + 1 == total_working {
            println!("Processing working commit {}/{}: {}", i + 1, total_working, id);
        }
        
        // Migrate enum values
        migrate_json_value(&mut schema_data);
        
        // Update in database
        sqlx::query("UPDATE working_commits SET schema_data = $1 WHERE id = $2")
            .bind(&schema_data)
            .bind(&id)
            .execute(&pool)
            .await?;
    }
    
    println!("Working commits migration completed!");
    println!("\nAll migrations completed successfully!");
    
    Ok(())
}