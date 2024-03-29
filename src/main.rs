use anyhow::{Context as _, Error, Result};
use commands::Data;
use poise::serenity_prelude::{
    self as serenity, ClientBuilder, Context, CreateCommandOption, CreateInteractionResponse,
    CreateInteractionResponseMessage, EventHandler, GatewayIntents, Interaction,
};
use shuttle_runtime::{self, async_trait, Error as ShuttleError, SecretStore, Service};
use tokio::sync::Mutex;
use std::{net::SocketAddr, num::ParseIntError, sync::Arc};
use structs::OrePoint;
mod commands;
mod db;
mod list;
mod structs;

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

    let mut occupy_command = commands::occupy();
    let mut force_occupy_command = commands::force_occupy();

    let ore_point_type_setter = Some(|option: CreateCommandOption| -> CreateCommandOption {
        option
            .kind(serenity::CommandOptionType::Integer)
            .min_int_value(1)
            .max_int_value(OrePoint::iter().count() as u64)
    } as fn(_) -> _);

    // Set max ore point id

    occupy_command.parameters.first_mut().unwrap().type_setter = ore_point_type_setter;
    force_occupy_command
        .parameters
        .get_mut(1)
        .unwrap()
        .type_setter = ore_point_type_setter;

    let discord_bot = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![commands::list(), occupy_command, force_occupy_command],
            event_handler: (|serenity_ctx, event, _ctx, data| {
                Box::pin(async move {
                    let handler = Handler(data.clone(), Arc::new(Mutex::const_new(Ok(()))));
                    event.clone().dispatch(serenity_ctx.clone(), &handler).await;
                    let mut lock = handler.1.lock().await;
                    std::mem::replace(&mut *lock, Ok(()))
                })
            }),
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
        .map_err(Error::new)?;

    Ok(BotService { client })
}

struct Handler(Data, Arc<Mutex<Result<()>>>);

impl Handler {
    async fn interaction(&self, ctx: Context, interaction: Interaction) -> Result<()> {
        let Interaction::Component(c) = interaction else  { return Ok(()); };
        let (page_index, page_size): (u32, u32) = c
            .data
            .custom_id
            .strip_prefix("list:")
            .and_then(|x| x.split_once(':'))
            .and_then(|(a, b)| {
                (|| -> Result<_, ParseIntError> { Ok((a.parse()?, b.parse()?)) })().ok()
            })
            .context("parse custom_id error")?;
        let content = list::list(
            &self.0.pool,
            c.guild_id.context("Unknown guild_id")?.get(),
            page_index,
            page_size,
        )
        .await?;
        c.create_response(
            ctx,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .embed(content.embed)
                    .components(content.component),
            ),
        )
        .await?;
        Ok(())
    }
}

#[async_trait]
impl EventHandler for Handler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        let result = self.interaction(ctx, interaction).await;
        *self.1.lock().await = result;
    }
}
