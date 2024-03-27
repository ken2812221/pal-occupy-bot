use anyhow::{Context as _, Error, Result};
use poise::serenity_prelude::{self as serenity, ClientBuilder, GatewayIntents};
use shuttle_runtime::{self, Error as ShuttleError, SecretStore, Service};
use std::net::SocketAddr;
mod commands;
mod db;
mod structs;
mod paginate;

struct BotService {
    client: serenity::Client,
}

#[shuttle_runtime::async_trait]
impl Service for BotService {
    async fn bind(mut self, _addr: SocketAddr) -> Result<(), ShuttleError> {
        _ = self.client.start().await;
        Ok(())
    }
}

#[shuttle_runtime::main]
async fn main(
    #[shuttle_runtime::Secrets] secrets: SecretStore,
    #[shuttle_shared_db::Postgres(local_uri = "{secrets.POSTGRESQL_URI}")] pool: sqlx::PgPool,
) -> Result<impl Service, ShuttleError> {
    structs::init(&pool).await;

    let discord_bot = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                commands::list(),
                commands::occupy(),
                commands::force_occupy(),
            ],
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(commands::Data { pool })
            })
        })
        .build();
    let token = secrets
        .get("DISCORD_TOKEN")
        .context("'DISCORD_TOKEN' was not found")?;
    let client = ClientBuilder::new(token, GatewayIntents::empty())
        .framework(discord_bot)
        .await
        .map_err(|err| Error::new(err))?;

    Ok(BotService { client })
}
