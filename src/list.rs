use crate::db;
use anyhow::{Error, Ok, Result};
use poise::serenity_prelude::{
    Color, CreateActionRow, CreateButton, CreateEmbed,
};
use sqlx::PgPool;

const PAGE_SIZE: u32 = 5;

pub struct ListContent {
    pub embed: CreateEmbed,
    pub component: Vec<CreateActionRow>,
}

pub async fn list(pool: &PgPool, guild_id: u64, page_index: u32) -> Result<ListContent, Error> {
    let start = PAGE_SIZE * page_index;

    let max_page = db::get_point_count(pool).await?.div_ceil(PAGE_SIZE);
    let data = db::get_point_data(pool, guild_id, start, PAGE_SIZE).await?;

    let content = data
        .into_iter()
        .map(|row| {
            format!(
                "`{:>2}` {} {} `({}, {})`\n{}{}{}\n",
                row.id,
                row.emoji(),
                row.name,
                row.x,
                row.y,
                row.user_id
                    .map_or(String::new(), |user_id| format!("佔領者: <@{}>\n", user_id)),
                row.due_time.map_or(String::new(), |due_time| format!(
                    "佔領期限: <t:{}:F>\n",
                    due_time.timestamp()
                )),
                row.battle_user_id
                    .map_or(String::new(), |battle_user_id| format!(
                        "<@{}> 已發起挑戰\n",
                        battle_user_id
                    ))
            )
        })
        .chain(std::iter::once(format!("**{}/{}**", page_index + 1, max_page)))
        .collect::<String>();

    let buttons = CreateActionRow::Buttons(vec![
        if page_index > 0 {
            CreateButton::new(format!("list:{}", page_index - 1))
        } else {
            CreateButton::new("0").disabled(true)
        }.emoji('◀'),
        if page_index + 1 < max_page {
            CreateButton::new(format!("list:{}", page_index + 1))
        } else {
            CreateButton::new("0").disabled(true)
        }.emoji('▶'),
    ]);

    Ok(ListContent {
        embed: CreateEmbed::new().description(content).color(Color::BLUE),
        component: vec![buttons],
    })
}
