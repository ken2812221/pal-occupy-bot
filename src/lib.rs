use anyhow::{Context as _, Error, Result};
use db::BotDB;
use interaction::InteractionController;
use poise::{
    serenity_prelude::{self as serenity, ClientBuilder, Context, FutureExt, GatewayIntents},
    Framework,
};
use std::collections::HashMap;
mod commands;
mod db;
mod interaction;
mod structs;

type FrameworkError<'a> = poise::FrameworkError<'a, BotDB, Error>;
type PoiseContext<'a> = poise::Context<'a, BotDB, Error>;

pub async fn bind(mut client: serenity::Client) -> Result<()> {
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
            // event_handler: |a, b, c, d| event_handler(a, b, c, d).boxed(),
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
    let controller = InteractionController {
        sc: ctx.clone(),
        db: db.clone(),
    };
    tokio::spawn(async move { controller.interaction_loop().await });
    Ok(db)
}
