use crate::{
    db::{self, OccupyData},
    list,
    structs::OrePoint,
};
use anyhow::{Context as _, Error, Result};
use chrono::{Days, Utc};
use poise::{serenity_prelude::User, CreateReply};

#[derive(Clone)]
pub(crate) struct Data {
    pub pool: sqlx::PgPool,
}

type Context<'a> = poise::Context<'a, Data, Error>;

#[poise::command(slash_command, rename = "佔領")]
#[doc = "佔領一座礦點"]
pub async fn occupy(
    ctx: Context<'_>,
    #[rename = "礦點"]
    #[description = "佔領的礦點編號"]
    point_id: i32,
) -> Result<()> {
    let guild_id = ctx.guild_id().context("Missing guild id")?.get();
    let user_id = ctx.author().id.get();
    let pool = &ctx.data().pool;

    let point = OrePoint::iter()
        .find(|p| p.id == point_id)
        .context("找不到礦點")?;

    // begin transaction
    let trans = pool.begin().await?;

    // 確認是否擁有同類礦點
    if db::has_occupy_type(pool, guild_id, user_id, point.ore_type).await? {
        ctx.send(
            CreateReply::default()
                .reply(true)
                .ephemeral(true)
                .content("你已佔領同類礦點或已發起挑戰"),
        )
        .await?;
        return Ok(());
    }

    // 確認是否佔領
    match db::get_occupy_data(pool, guild_id, point.id).await? {
        Some(mut data) => {
            // 已被佔領

            if data.due_time > Utc::now() {
                // 佔領期限未到
                ctx.send(
                    CreateReply::default()
                        .reply(true)
                        .ephemeral(true)
                        .content(format!(
                            "礦點已被佔領，可於 <t:{0}:R> (<t:{0}:F>) 發起挑戰",
                            data.due_time.timestamp()
                        )),
                )
                .await?;
                return Ok(());
            }

            if data.battle_user_id.is_some() {
                // 已有人登記挑戰
                ctx.send(
                    CreateReply::default()
                        .reply(true)
                        .ephemeral(true)
                        .content("礦點已有玩家登記挑戰"),
                )
                .await?;
                return Ok(());
            }

            // 登記挑戰
            data.battle_user_id = Some(user_id);
            db::update_occupy_data(pool, data).await?;
            trans.commit().await?;

            ctx.reply(format!(
                "已登記挑戰 {} {} ({}, {})\n",
                point.emoji(),
                point.name,
                point.x,
                point.y
            ))
            .await?;
            Ok(())
        }
        None => {
            let data = OccupyData {
                ore_point_id: point.id,
                guild_id,
                user_id,
                due_time: Utc::now()
                    .checked_add_days(Days::new(14))
                    .context("Failed to add days")?,
                battle_user_id: None,
            };

            db::occupy(pool, data).await?;
            trans.commit().await?;
            ctx.reply(format!(
                "已佔領 {} {} ({}, {})\n",
                point.emoji(),
                point.name,
                point.x,
                point.y
            ))
            .await?;
            Ok(())
        }
    }
}

#[poise::command(slash_command, rename = "強制佔領")]
#[doc = "強制佔領一座礦點"]
pub async fn force_occupy(
    ctx: Context<'_>,
    #[rename = "玩家"]
    #[description = "佔領的玩家"]
    user: User,
    #[rename = "礦點"]
    #[description = "佔領的礦點編號"]
    point_id: i32,
) -> Result<()> {
    let guild_id = ctx.guild_id().context("Missing guild id")?.get();
    let user_id = user.id.get();
    let pool = &ctx.data().pool;

    let point = OrePoint::iter()
        .find(|p| p.id == point_id)
        .context("找不到礦點")?;

    let data = OccupyData {
        ore_point_id: point_id,
        guild_id,
        user_id,
        due_time: Utc::now()
            .checked_add_days(Days::new(14))
            .context("Failed to add days")?,
        battle_user_id: None,
    };

    db::force_occupy(pool, data).await?;
    ctx.reply(format!(
        "<@{}> 已佔領 {} {} ({}, {})\n",
        user_id,
        point.emoji(),
        point.name,
        point.x,
        point.y
    ))
    .await?;
    Ok(())
}

#[poise::command(slash_command, rename = "礦點")]
#[doc = "列出所有的礦點"]
pub async fn list(
    ctx: Context<'_>,
    #[min = 1]
    #[max = 20]
    #[rename = "每頁礦點數量"]
    #[description = "每頁礦點數量"]
    page_size: Option<u32>,
) -> Result<()> {
    let pool = &ctx.data().pool;
    let guild_id = ctx.guild_id().context("err")?.get();
    let page_size = page_size.unwrap_or(20);

    let content = list::list(pool, guild_id, 0, page_size).await?;

    let reply = CreateReply::default()
        .embed(content.embed)
        .components(content.component)
        .reply(true)
        .ephemeral(true);

    ctx.send(reply).await?;

    Ok(())
}
