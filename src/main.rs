use anyhow::{Context as _, Error, Result};
use db::BotDB;
use poise::{
    serenity_prelude::{
        self as serenity, ClientBuilder, Context, CreateInteractionResponse,
        CreateInteractionResponseMessage, EventHandler, FullEvent, GatewayIntents, Interaction,
    },
    BoxFuture,
};
use shuttle_runtime::{self, async_trait, Error as ShuttleError, SecretStore, Service};
use std::{net::SocketAddr, num::ParseIntError, ops::DerefMut, sync::Arc};
use tokio::sync::Mutex;
mod commands;
mod db;
mod list;
mod structs;

type FrameworkContext<'a> = poise::FrameworkContext<'a, BotDB, Error>;
type FrameworkError<'a> = poise::FrameworkError<'a, BotDB, Error>;
type PoiseContext<'a> = poise::Context<'a, BotDB, Error>;

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
    let db = BotDB::new(pool);
    structs::init(&db).await;

    let discord_bot = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: commands::get_commands(),
            event_handler,
            on_error,
            post_command: write_log,
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(db)
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

struct Handler(BotDB, Arc<Mutex<Result<()>>>);

impl Handler {
    async fn interaction(&self, ctx: Context, interaction: Interaction) -> Result<()> {
        let Interaction::Component(c) = interaction else {
            return Ok(());
        };
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
            &self.0,
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

fn event_handler<'a>(
    ctx: &'a Context,
    event: &'a FullEvent,
    _: FrameworkContext<'a>,
    data: &'a BotDB,
) -> BoxFuture<'a, Result<()>> {
    Box::pin(async move {
        let handler = Handler(data.clone(), Arc::new(Mutex::const_new(Ok(()))));
        event.clone().dispatch(ctx.clone(), &handler).await;
        std::mem::replace(handler.1.lock_owned().await.deref_mut(), Ok(()))
    })
}

fn on_error(err: FrameworkError<'_>) -> BoxFuture<'_, ()> {
    Box::pin(async move {
        tracing::error!("{err}");
    })
}

fn write_log(ctx: PoiseContext<'_>) -> BoxFuture<'_, ()> {
    Box::pin(async move {
        _ = ctx
            .data()
            .write_log(
                ctx.guild_id().map(|x| x.get()),
                ctx.channel_id().get(),
                ctx.author().id.get(),
                &ctx.invocation_string(),
            )
            .await;
    })
}
#[async_trait]
impl EventHandler for Handler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        let result = self.interaction(ctx, interaction).await;
        *self.1.lock().await = result;
    }
}
