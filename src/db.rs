use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};

use crate::structs::{OrePoint, OreType};

#[derive(FromRow)]
struct OccupyDB {
    ore_point_id: i32,
    guild_id: i64,
    user_id: i64,
    due_time: DateTime<Utc>,
    battle_user_id: Option<i64>,
}

pub struct OccupyData {
    pub ore_point_id: i32,
    pub guild_id: u64,
    pub user_id: u64,
    pub due_time: DateTime<Utc>,
    pub battle_user_id: Option<u64>,
}

impl From<OccupyDB> for OccupyData {
    fn from(value: OccupyDB) -> Self {
        OccupyData {
            ore_point_id: value.ore_point_id,
            guild_id: value.guild_id as u64,
            user_id: value.user_id as u64,
            due_time: value.due_time,
            battle_user_id: value.battle_user_id.map(|x| x as u64),
        }
    }
}

impl From<OccupyData> for OccupyDB {
    fn from(value: OccupyData) -> Self {
        OccupyDB {
            ore_point_id: value.ore_point_id,
            guild_id: value.guild_id as i64,
            user_id: value.user_id as i64,
            due_time: value.due_time,
            battle_user_id: value.battle_user_id.map(|x| x as i64),
        }
    }
}

pub async fn list_by_guild_id(
    pool: &PgPool,
    guild_id: u64,
) -> Result<Vec<OccupyData>, sqlx::Error> {
    let occupy_data: Vec<OccupyDB> =
        sqlx::query_as("SELECT * FROM occupy_table WHERE guild_id = $1")
            .bind(guild_id as i64)
            .fetch_all(pool)
            .await?;
    Ok(occupy_data.into_iter().map(|x| x.into()).collect())
}

pub async fn occupy(pool: &PgPool, data: OccupyData) -> Result<(), sqlx::Error> {
    let data: OccupyDB = data.into();
    sqlx::query(
        "INSERT INTO occupy_table(ore_point_id, user_id, due_time, guild_id) VALUES ($1, $2, $3, $4)",
    )
    .bind(data.ore_point_id)
    .bind(data.user_id)
    .bind(data.due_time)
    .bind(data.guild_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_ore_types(pool: &PgPool) -> Result<Vec<OreType>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM ore_type ORDER BY id")
        .fetch_all(pool)
        .await
}

pub async fn get_ore_points(pool: &PgPool) -> Result<Vec<OrePoint>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM ore_point ORDER BY id")
        .fetch_all(pool)
        .await
}

pub async fn has_occupy_type(
    pool: &PgPool,
    guild_id: u64,
    user_id: u64,
    ore_type: i32,
) -> Result<bool, sqlx::Error> {
    let row = sqlx::query("SELECT * FROM occupy_table INNER JOIN ore_point ON occupy_table.ore_point_id = ore_point.id WHERE guild_id = $1 AND ( user_id = $2 OR battle_user_id = $2 ) AND ((ore_type & $3) <> 0) LIMIT 1")
        .bind(guild_id as i64)
        .bind(user_id as i64)
        .bind(ore_type)
        .fetch_optional(pool)
        .await?;
    Ok(row.is_some())
}

pub async fn get_occupy_data(
    pool: &PgPool,
    guild_id: u64,
    ore_point_id: i32,
) -> Result<Option<OccupyData>, sqlx::Error> {
    let row: Option<OccupyDB> =
        sqlx::query_as("SELECT * FROM occupy_table WHERE guild_id = $1 AND ore_point_id = $2")
            .bind(guild_id as i64)
            .bind(ore_point_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|x| x.into()))
}

pub async fn update_occupy_data(pool: &PgPool, data: OccupyData) -> Result<(), sqlx::Error> {
    let data: OccupyDB = data.into();
    sqlx::query("UPDATE occupy_table SET user_id = $1, due_time = $2, battle_user_id = $3 WHERE guild_id = $4 AND ore_point_id = $5")
        .bind(data.user_id)
        .bind(data.due_time)
        .bind(data.battle_user_id)
        .bind(data.guild_id)
        .bind(data.ore_point_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn force_occupy(pool: &PgPool, data: OccupyData) -> Result<(), sqlx::Error> {
    let data: OccupyDB = data.into();
    sqlx::query(
        r#"INSERT INTO occupy_table(ore_point_id, user_id, due_time, guild_id) VALUES ($1, $2, $3, $4)
            ON CONFLICT (ore_point_id, guild_id) DO UPDATE SET user_id = $2, due_time = $3, battle_user_id = NULL
        "#,
    )
    .bind(data.ore_point_id)
    .bind(data.user_id)
    .bind(data.due_time)
    .bind(data.guild_id)
    .execute(pool)
    .await?;
    Ok(())
}
