use crate::{
    db::{BotDB, OccupyData},
    list,
    structs::OrePoint,
};
use anyhow::{Context as _, Error, Result};
use chrono::{Days, Utc};
use poise::{
    serenity_prelude::{
        self as serenity, CommandInteraction, CommandOptionType, Context as SerenityContext, CreateActionRow, CreateAllowedMentions, CreateButton, CreateCommandOption, CreateMessage, DiscordJsonError, ErrorResponse, HttpError, Message, ResolvedValue, Role, User
    },
    Command, CreateReply, SlashArgError, SlashArgument,
};
use shuttle_runtime::async_trait;
use std::fmt::Display;

type Context<'a> = poise::Context<'a, BotDB, Error>;

/// 佔領一座礦點
#[poise::command(slash_command, rename = "佔領")]
async fn occupy(
    ctx: Context<'_>,
    #[rename = "礦點"]
    #[description = "佔領的礦點編號"]
    point_id: i32,
) -> Result<()> {
    let guild_id = ctx.guild_id().context("Missing guild id")?.get();
    let user_id = ctx.author().id.get();
    let db = ctx.data();

    let point = OrePoint::iter()
        .find(|p| p.id == point_id)
        .context("找不到礦點")?;

    // begin transaction
    let trans = db.begin_transaction().await?;

    // 確認是否擁有同類礦點
    if db
        .has_occupy_type(guild_id, user_id, point.ore_type)
        .await?
    {
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
    match db.get_occupy_data(guild_id, point.id).await? {
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
            let original_user_id = data.user_id;
            db.update_occupy_data(data).await?;
            trans.commit().await?;

            // 找到登記通知的身分組
            let role_id = db.get_guild_notify_role(guild_id).await?;

            ctx.send(
                CreateReply::default()
                    .reply(true)
                    .allowed_mentions(CreateAllowedMentions::new().all_roles(true).all_users(true))
                    .content(format!(
                        "已登記挑戰由 <@{}> 佔領的 {} {} ({}, {}) {}\n",
                        original_user_id,
                        point.emoji(),
                        point.name,
                        point.x,
                        point.y,
                        role_id.map_or(String::new(), |role_id| format!("<@&{role_id}>"))
                    )),
            )
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

            db.occupy(data).await?;
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

/// 強制佔領一座礦點
#[poise::command(
    slash_command,
    rename = "強制佔領",
    default_member_permissions = "MANAGE_GUILD"
)]
async fn force_occupy(
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
    let db = ctx.data();

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

    db.force_occupy(data).await?;
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

/// 列出所有的礦點
#[poise::command(slash_command, rename = "礦點", ephemeral)]
async fn list_points(
    ctx: Context<'_>,
    #[min = 1]
    #[max = 20]
    #[rename = "每頁礦點數量"]
    #[description = "每頁礦點數量"]
    page_size: Option<u32>,
) -> Result<()> {
    let db = ctx.data();
    let guild_id = ctx.guild_id().context("err")?.get();
    let page_size = page_size.unwrap_or(20);

    let content = list::list(db, guild_id, 0, page_size).await?;

    let reply = CreateReply::default()
        .embed(content.embed)
        .components(content.component)
        .reply(true)
        .ephemeral(true);

    ctx.send(reply).await?;

    Ok(())
}

/// 列出所有的礦點
#[poise::command(
    slash_command,
    rename = "挑戰通知",
    default_member_permissions = "MANAGE_GUILD",
    ephemeral
)]
async fn set_notify(
    ctx: Context<'_>,
    #[rename = "身分組"]
    #[description = "要通知的身分組"]
    role: Role,
) -> Result<()> {
    let db = ctx.data();
    let guild_id = ctx.guild_id().context("err")?.get();
    let role_id = role.id.get();

    db.set_guild_notify_role(guild_id, role_id).await?;

    ctx.reply(format!("發起挑戰時將會通知 <@&{}>", role_id))
        .await?;

    Ok(())
}

enum Mentionable {
    User(User),
    Role(Role),
}

#[async_trait]
impl SlashArgument for Mentionable {
    async fn extract(
        _: &SerenityContext,
        _: &CommandInteraction,
        value: &ResolvedValue<'_>,
    ) -> Result<Self, SlashArgError> {
        match *value {
            ResolvedValue::User(user, _) => Ok(Self::User(user.clone())),
            ResolvedValue::Role(role) => Ok(Self::Role(role.clone())),
            _ => Err(SlashArgError::new_command_structure_mismatch(
                "Value should be user or role.",
            )),
        }
    }

    fn create(builder: CreateCommandOption) -> CreateCommandOption {
        builder.kind(CommandOptionType::Mentionable)
    }
}

impl Display for Mentionable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mentionable::User(user) => write!(f, "<@{}>", user.id.get()),
            Mentionable::Role(role) => write!(f, "<@&{}>", role.id.get()),
        }
    }
}

#[poise::command(
    slash_command,
    guild_only,
    default_member_permissions = "MANAGE_GUILD",
    rename = "測試",
    ephemeral
)]
async fn init(ctx: Context<'_>) -> Result<()> {
    ctx.defer_ephemeral().await?;
    let button_row = vec![
        CreateButton::new("occupy").label("佔領"),
        CreateButton::new("list").label("礦點佔領情形"),
    ];
    let result = ctx
        .channel_id()
        .send_message(
            ctx,
            CreateMessage::new().components(vec![CreateActionRow::Buttons(button_row)]),
        )
        .await;
    match result {
        Ok(_) => {
            let handle = ctx.reply("OK").await?;
            handle.delete(ctx).await?;
        }
        Err(serenity::Error::Http(HttpError::UnsuccessfulRequest(ErrorResponse {
            error: DiscordJsonError { code: 50001, .. },
            ..
        }))) => {
            ctx.reply("錯誤! 沒有發送訊息權限").await?;
        }
        Err(err) => {
            ctx.reply(format!("{err:#?}")).await?;
        }
    };
    Ok(())
}

#[poise::command(context_menu_command = "aaa", guild_only, ephemeral, default_member_permissions = "MANAGE_GUILD")]
async fn test2(ctx: Context<'_>, msg: Message) -> Result<()> {
    ctx.reply(msg.id.get().to_string()).await?;
    Ok(())
}

pub fn get_commands() -> Vec<Command<BotDB, Error>> {
    let mut occupy_command = occupy();
    let mut force_occupy_command = force_occupy();

    let ore_point_type_setter: Option<fn(CreateCommandOption) -> CreateCommandOption> =
        Some(|option| {
            option
                .kind(CommandOptionType::Integer)
                .min_int_value(1)
                .max_int_value(OrePoint::iter().count() as u64)
        });

    // Set max ore point id

    occupy_command.parameters.first_mut().unwrap().type_setter = ore_point_type_setter;
    force_occupy_command
        .parameters
        .get_mut(1)
        .unwrap()
        .type_setter = ore_point_type_setter;

    vec![
        // init(),
        set_notify(),
        list_points(),
        occupy_command,
        force_occupy_command,
    ]
}
