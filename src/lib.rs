use anyhow::{Context as _, Error, Result};
use db::BotDB;
use poise::{
    serenity_prelude::{
        self as serenity, ClientBuilder, Context, CreateInteractionResponse,
        CreateInteractionResponseMessage, EventHandler, FullEvent, FutureExt, GatewayIntents,
        Interaction,
    },
    Framework,
};
use shuttle_runtime::{self, async_trait, Error as ShuttleError};
use std::{collections::HashMap, ops::DerefMut, sync::Arc};
use tokio::sync::Mutex;
mod commands;
mod db;
mod interaction;
mod structs;

type FrameworkContext<'a> = poise::FrameworkContext<'a, BotDB, Error>;
type FrameworkError<'a> = poise::FrameworkError<'a, BotDB, Error>;
type PoiseContext<'a> = poise::Context<'a, BotDB, Error>;

pub async fn bind(mut client: serenity::Client) -> Result<(), ShuttleError> {
    tokio::spawn(async move { client.start().await });
    Ok(())
}

pub async fn init(
    secrets: HashMap<String, String>,
    pool: sqlx::PgPool,
) -> Result<serenity::Client> {
    let db = BotDB::new(pool);
    structs::init(&db).await;

    let discord_bot = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: commands::get_commands(),
            event_handler: |a, b, c, d| event_handler(a, b, c, d).boxed(),
            on_error: |a| on_error(a).boxed(),
            post_command: |a| write_log(a).boxed(),
            ..Default::default()
        })
        .setup(|ctx, _, framework| setup(ctx, framework, db).boxed())
        .build();

    let token = secrets
        .get("DISCORD_TOKEN")
        .context("'DISCORD_TOKEN' was not found")?;
    let client = ClientBuilder::new(token, GatewayIntents::empty())
        .framework(discord_bot)
        .await
        .map_err(Error::new)?;

    Ok(client)
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
            .and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)))
            .context("parse custom_id error")?;
        let content = interaction::list(
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

async fn event_handler(
    ctx: &Context,
    event: &FullEvent,
    _: FrameworkContext<'_>,
    data: &BotDB,
) -> Result<()> {
    let handler = Handler(data.clone(), Arc::new(Mutex::const_new(Ok(()))));
    event.clone().dispatch(ctx.clone(), &handler).await;
    std::mem::replace(handler.1.lock_owned().await.deref_mut(), Ok(()))
}

async fn on_error(err: FrameworkError<'_>) {
    tracing::error!("{err}");
}

async fn write_log(ctx: PoiseContext<'_>) {
    _ = ctx
        .data()
        .write_log(
            ctx.guild_id().map(|x| x.get()),
            ctx.channel_id().get(),
            ctx.author().id.get(),
            &ctx.invocation_string(),
        )
        .await;
}

async fn setup(ctx: &Context, fw: &Framework<BotDB, Error>, db: BotDB) -> Result<BotDB> {
    poise::builtins::register_globally(ctx, &fw.options().commands).await?;
    Ok(db)
}

#[async_trait]
impl EventHandler for Handler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        let result = self.interaction(ctx, interaction).await;
        *self.1.lock().await = result;
    }
}
