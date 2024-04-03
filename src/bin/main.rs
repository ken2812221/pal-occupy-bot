use std::collections::HashMap;

use anyhow::Context;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();
    let map = toml::from_str::<HashMap<String, String>>(
        std::fs::read_to_string("Secrets.dev.toml")?.as_str(),
    )?;
    let pool = sqlx::Pool::connect(map.get("POSTGRESQL_URI").context("context")?).await?;
    let mut client = pal_occupy_bot::init(map, pool).await?;
    client.start().await?;
    Ok(())
}
