use std::vec;

use crate::{
    db::{BotDB, OccupyData},
    structs::OrePoint,
};
use anyhow::{Context as _, Error};
use chrono::{Days, Utc};
use poise::serenity_prelude::{
    self as serenity, ButtonStyle, Color, ComponentInteraction, ComponentInteractionCollector,
    CreateActionRow, CreateButton, CreateEmbed, CreateEmbedFooter, CreateInteractionResponse,
    CreateInteractionResponseFollowup, CreateInteractionResponseMessage, EmojiId,
};

type Result<U = (), E = Error> = std::result::Result<U, E>;

pub struct ListContent {
    pub embed: CreateEmbed,
    pub component: Vec<CreateActionRow>,
}

pub async fn list(
    db: &BotDB,
    guild_id: u64,
    page_index: u32,
    page_size: u32,
    is_admin: bool,
) -> Result<ListContent, Error> {
    // let page_index = 0u32;
    // let page_size = 20u32;
    let start = page_size * page_index;

    let max_page = db.get_point_count().await?.div_ceil(page_size);
    let data = db.get_point_data(guild_id, start, page_size).await?;

    let mut embed = CreateEmbed::new()
        .color(Color::BLUE)
        .title("礦點列表")
        .footer(CreateEmbedFooter::new(format!(
            "{}/{}",
            page_index + 1,
            max_page
        )));

    for row in data {
        embed = embed.field(
            format!("{} {} `({}, {})`", row.emoji(), row.name, row.x, row.y),
            format!(
                "{}{}{}",
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
            ),
            false,
        );
    }

    let page_buttons = CreateActionRow::Buttons(vec![
        if page_index > 0 {
            CreateButton::new(format!("list_points:{}:{}", page_index - 1, page_size))
        } else {
            CreateButton::new("0").disabled(true)
        }
        .emoji('◀'),
        CreateButton::new(format!("list_points:{}:{}", page_index, page_size))
            .emoji('🔄')
            .style(ButtonStyle::Success),
        if page_index + 1 < max_page {
            CreateButton::new(format!("list_points:{}:{}", page_index + 1, page_size))
        } else {
            CreateButton::new("0").disabled(true)
        }
        .emoji('▶'),
    ]);

    let mut command_buttons = vec![
        CreateButton::new(format!("show_occupy:{}:{}", page_index, page_size)).label("佔領"),
        CreateButton::new(format!("show_challenge:{}:{}", page_index, page_size)).label("挑戰"),
    ];

    if is_admin {
        command_buttons.push(
            CreateButton::new(format!("show_judge:{}:{}", page_index, page_size)).label("裁決"),
        );
    }

    let command_buttons = CreateActionRow::Buttons(command_buttons);

    Ok(ListContent {
        embed,
        component: vec![page_buttons, command_buttons],
    })
}

pub struct InteractionController {
    pub sc: serenity::Context,
    pub db: BotDB,
}

enum ShowType {
    Occupy,
    Challenge,
    Judge,
}

impl InteractionController {
    pub async fn interaction_loop(&self) {
        while let Some(ref ci) = ComponentInteractionCollector::new(&self.sc).await {
            if let Err(err) = self.interaction_income(ci).await {
                tracing::error!("{err}");
            }
        }
    }
    pub async fn interaction_income(&self, ci: &ComponentInteraction) -> Result {
        let custom_id = ci.data.custom_id.clone();
        let (command_name, args) = match custom_id.split_once(':') {
            Some((a, b)) => (a, b),
            None => (custom_id.as_str(), ""),
        };
        let args = &*args.split(':').collect::<Box<[_]>>();
        match command_name {
            "list_points" => self.list_points(ci, args).await?,
            "show_occupy" => {
                self.show_target_ore_point_buttons(ShowType::Occupy, ci, args)
                    .await?
            }
            "show_challenge" => {
                self.show_target_ore_point_buttons(ShowType::Challenge, ci, args)
                    .await?
            }
            "show_judge" => {
                self.show_target_ore_point_buttons(ShowType::Judge, ci, args)
                    .await?
            }
            "occupy" => self.occupy(ci, args).await?,
            _ => anyhow::bail!("Unknown command"),
        }
        Ok(())
    }

    async fn show_target_ore_point_buttons(
        &self,
        t: ShowType,
        ci: &ComponentInteraction,
        args: &[&str],
    ) -> Result {
        let guild_id = ci.guild_id.context("No guild id")?.get();
        let user_id = ci.user.id.get();
        let db = &self.db;

        let [page_index, page_size] = args else {
            anyhow::bail!("Unknown command");
        };
        let page_index: u32 = page_index.parse()?;
        let page_size = page_size.parse()?;
        let start = page_index * page_size;
        let occupy_ore_type = db.get_user_ocuppy_type(guild_id, user_id).await?;
        let list_data = db.get_point_data(guild_id, start, page_size).await?;
        let list_data = list_data.into_iter().filter(|x| match t {
            ShowType::Occupy => x.user_id.is_none() && (x.ore_type & occupy_ore_type) == 0,
            ShowType::Challenge => {
                x.user_id.is_some()
                    && x.due_time.is_some_and(|x| x < Utc::now())
                    && x.battle_user_id.is_none()
                    && (x.ore_type & occupy_ore_type) == 0
            }
            ShowType::Judge => x.user_id.is_some() && x.battle_user_id.is_some(),
        });
        let command_name = match t {
            ShowType::Occupy => "occupy",
            ShowType::Challenge => "challenge",
            ShowType::Judge => "judge"
        };
        let mut components = vec![];
        let mut current_row = vec![];
        for curr in list_data {
            if current_row.len() >= 5 {
                components.push(CreateActionRow::Buttons(std::mem::take(&mut current_row)));
            }
            current_row.push(
                CreateButton::new(format!("{}:{}:{}:{}", command_name, curr.id, page_index, page_size))
                    .label(format!("{} ({}, {})", curr.name, curr.x, curr.y))
                    .emoji(EmojiId::new(curr.first_emoji_id().context("no emoji id")?)),
            );
        }
        if !current_row.is_empty() {
            components.push(CreateActionRow::Buttons(std::mem::take(&mut current_row)));
        }
        components.push(CreateActionRow::Buttons(vec![CreateButton::new(format!(
            "list_points:{}:{}",
            page_index, page_size
        ))
        .label("返回")
        .style(ButtonStyle::Secondary)]));

        // ci.message
        //     .clone()
        //     .edit(&self.sc, EditMessage::new().components(components))
        //     .await?;
        ci.create_response(
            &self.sc,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new().components(components),
            ),
        )
        .await?;

        Ok(())
    }

    async fn list_points(&self, ci: &ComponentInteraction, args: &[&str]) -> Result {
        let guild_id = ci.guild_id.context("No guild id")?.get();

        let is_admin = ci
            .member
            .as_ref()
            .context("no member")?
            .permissions
            .context("no permission")?
            .manage_guild();
        match args {
            [""] => {
                // New message
                let list_data = list(&self.db, guild_id, 0, 20, is_admin).await?;
                ci.create_response(&self.sc, CreateInteractionResponse::Acknowledge)
                    .await?;
                ci.create_followup(
                    &self.sc,
                    CreateInteractionResponseFollowup::new()
                        .ephemeral(true)
                        .embed(list_data.embed)
                        .components(list_data.component),
                )
                .await?;
            }
            [page_index, page_size] => {
                let page_index = page_index.parse()?;
                let page_size = page_size.parse()?;
                let list_data = list(&self.db, guild_id, page_index, page_size, is_admin).await?;
                ci.create_response(
                    &self.sc,
                    CreateInteractionResponse::UpdateMessage(
                        CreateInteractionResponseMessage::new()
                            .add_embed(list_data.embed)
                            .components(list_data.component),
                    ),
                )
                .await?;
            }
            _ => (),
        }
        Ok(())
    }

    async fn occupy(&self, ci: &ComponentInteraction, args: &[&str]) -> Result {
        let point_id: i32 = args.first().context("no point_id argument")?.parse()?;

        let guild_id = ci.guild_id.context("Missing guild id")?.get();
        let user_id = ci.user.id.get();

        let point = OrePoint::iter()
            .find(|p| p.id == point_id)
            .context("找不到礦點")?;

        // begin transaction
        let trans = self.db.begin_transaction().await?;

        // 確認是否擁有同類礦點
        if self
            .db
            .has_occupy_type(guild_id, user_id, point.ore_type)
            .await?
        {
            // ci.create_response(&self.sc, CreateInteractionResponse::Acknowledge)
            //     .await?;
            ci.create_followup(
                &self.sc,
                CreateInteractionResponseFollowup::new()
                    .ephemeral(true)
                    .content("你已佔領同類礦點或已發起挑戰"),
            )
            .await?;
            return Ok(());
        }
        self.db
            .occupy(OccupyData {
                ore_point_id: point_id,
                guild_id,
                user_id,
                due_time: Utc::now()
                    .checked_add_days(Days::new(14))
                    .context("Failed to add days")?,
                battle_user_id: None,
            })
            .await?;
        trans.commit().await?;
        self.list_points(ci, &args[1..]).await?;
        Ok(())
    }
}
