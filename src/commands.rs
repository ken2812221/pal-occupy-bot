use crate::{
    db::{self, OccupyData},
    paginate,
    structs::{OrePoint, OreType},
};
use anyhow::{Context as _, Error, Result};
use chrono::{Days, Utc};
use poise::serenity_prelude::User;
use std::borrow::Cow;

#[derive(Clone)]
pub(crate) struct Data {
    pub pool: sqlx::PgPool,
}

type Context<'a> = poise::Context<'a, Data, Error>;

#[poise::command(slash_command, rename = "佔領")]
#[doc = "佔領一座的礦點"]
pub async fn occupy(
    ctx: Context<'_>,
    #[rename = "礦點"]
    #[description = "你想佔領的礦點"]
    point: OrePoint,
) -> Result<()> {
    let guild_id = ctx.guild_id().context("Missing guild id")?.get();
    let user_id = ctx.author().id.get();
    let pool = &ctx.data().pool;

    // begin transaction
    let trans = pool.begin().await?;

    // 確認是否擁有同類礦點
    if db::has_occupy_type(pool, guild_id, user_id, point.ore_type).await? {
        ctx.reply("你已佔領同類礦點或已發起挑戰").await?;
        return Ok(());
    }

    // 確認是否佔領
    match db::get_occupy_data(pool, guild_id, point.id).await? {
        Some(mut data) => {
            // 已被佔領

            if data.due_time > Utc::now() {
                // 佔領期限未到
                ctx.reply("礦點已被佔領").await?;
                return Ok(());
            }

            if data.battle_user_id.is_some() {
                // 已有人登記挑戰
                ctx.reply("礦點已有玩家登記挑戰").await?;
                return Ok(());
            }

            // 登記挑戰
            data.battle_user_id = Some(user_id);
            db::update_occupy_data(pool, data).await?;
            trans.commit().await?;

            ctx.reply(format!(
                "已登記挑戰 {} ({}, {})\n",
                point.name, point.x, point.y
            ))
            .await?;
            return Ok(());
        }
        None => {
            let data = OccupyData {
                ore_point_id: point.id,
                guild_id: guild_id,
                user_id: user_id,
                due_time: Utc::now()
                    .checked_add_days(Days::new(14))
                    .context("Failed to add days")?,
                battle_user_id: None,
            };

            db::occupy(pool, data).await?;
            trans.commit().await?;
            ctx.reply(format!(
                "已佔領 {} ({}, {})\n",
                point.name, point.x, point.y
            ))
            .await?;
            return Ok(());
        }
    }
}

#[poise::command(slash_command, rename = "強制佔領")]
#[doc = "強制佔領一座的礦點"]
pub async fn force_occupy(
    ctx: Context<'_>,
    #[rename = "玩家"]
    #[description = "佔領的玩家"]
    user: User,
    #[rename = "礦點"]
    #[description = "佔領的礦點"]
    point: OrePoint,
) -> Result<()> {
    let guild_id = ctx.guild_id().context("Missing guild id")?.get();
    let user_id = user.id.get();
    let pool = &ctx.data().pool;
    let data = OccupyData {
        ore_point_id: point.id,
        guild_id: guild_id,
        user_id: user_id,
        due_time: Utc::now()
            .checked_add_days(Days::new(14))
            .context("Failed to add days")?,
        battle_user_id: None,
    };

    db::force_occupy(pool, data).await?;
    ctx.reply(format!(
        "<@{}> 已佔領 {} ({}, {})\n",
        user_id, point.name, point.x, point.y
    ))
    .await?;
    return Ok(());
}

#[poise::command(slash_command, rename = "礦點")]
#[doc = "列出所有的礦點"]
pub async fn list(ctx: Context<'_>) -> Result<()> {
    let guild_id = ctx.guild_id().context("err")?;
    // let mut embed = CreateEmbed::default().title("所有礦點").color(Colour::BLUE);
    let user_data = db::list_by_guild_id(&ctx.data().pool, guild_id.get()).await?;

    let mut pages = Vec::<String>::new();

    let mut current_page = Vec::<String>::new();

    let mut current_page_item_count = 0;

    let item_per_page = 5;

    for t in OreType::iter() {
        // 印出每個礦點
        for p in OrePoint::iter().filter(|x| t.id == x.ore_type) {
            let occupy_user = user_data
                .iter()
                .find(|data| data.ore_point_id == p.id)
                .map_or(Cow::Borrowed(""), |data| {
                    let user_id = format!("占領者: <@{}>\n", data.user_id);
                    let due_time = format!("佔領期限: <t:{}:F>\n", data.due_time.timestamp());
                    let battle_user = if let Some(battle_uid) = data.battle_user_id {
                        format!("<@{}> 已發起挑戰\n", battle_uid)
                    } else {
                        String::new()
                    };
                    format!("{}{}{}", user_id, due_time, battle_user).into()
                });

            current_page.push(format!(
                "<{}> {} ({}, {})\n{}\n",
                t.emoji, p.name, p.x, p.y, occupy_user
            ));
            current_page_item_count += 1;
            if current_page_item_count >= item_per_page {
                pages.push(current_page.into_iter().collect());
                current_page = Vec::new();
                current_page_item_count = 0;
            }
        }
        if current_page_item_count > 0 {
            pages.push(current_page.into_iter().collect());
            current_page = Vec::new();
            current_page_item_count = 0;
        }
    }
    if current_page_item_count > 0 {
        pages.push(current_page.into_iter().collect());
    }

    let total_pages = pages.len();

    for (i, page) in pages.iter_mut().enumerate() {
        let page_str = format!("**{}/{}**", i + 1, total_pages);
        page.push_str(&page_str)
    }

    paginate::paginate_reply(ctx, &pages.iter().map(|s| s.as_str()).collect::<Vec<_>>()).await?;

    Ok(())
}
