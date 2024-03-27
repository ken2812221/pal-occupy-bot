use once_cell::sync::OnceCell;
use poise::{ChoiceParameter, CommandParameterChoice};
use sqlx::{prelude::FromRow, PgPool};
use std::collections::HashMap;

use crate::db;

#[derive(PartialEq, Eq, Clone, Debug, FromRow)]
pub(crate) struct OreType {
    pub id: i32,
    pub name: String,
}

#[derive(Clone, Debug, FromRow)]
pub(crate) struct OrePoint {
    pub id: i32,
    pub ore_type: i32,
    pub x: i32,
    pub y: i32,
    pub name: String,
}

pub static ORE_POINTS: OnceCell<Vec<OrePoint>> = OnceCell::new();
pub static ORE_TYPES: OnceCell<Vec<OreType>> = OnceCell::new();

impl OrePoint {
    async fn init(pool: &PgPool) {
        let data = db::get_ore_points(pool)
            .await
            .expect("Cannot get ORE_POINTS");
        ORE_POINTS
            .set(data)
            .expect("ORE_POINTS set more than once.");
    }

    pub fn iter() -> impl Iterator<Item = Self> {
        ORE_POINTS
            .get()
            .expect("ORE_POINTS not initialize")
            .iter()
            .cloned()
    }
}

impl OreType {
    async fn init(pool: &PgPool) {
        let data = db::get_ore_types(pool).await.expect("Cannot get ORE_TYPES");
        ORE_TYPES.set(data).expect("ORE_TYPES set more than once.");
    }

    pub fn iter() -> impl Iterator<Item = Self> {
        ORE_TYPES
            .get()
            .expect("ORE_TYPES not initialize")
            .iter()
            .cloned()
    }
}

impl ChoiceParameter for OrePoint {
    fn list() -> Vec<CommandParameterChoice> {
        OrePoint::iter()
            .map(|ore_point| CommandParameterChoice {
                name: format!("{} ({}, {})", ore_point.name, ore_point.x, ore_point.y),
                localizations: HashMap::new(),
                __non_exhaustive: (),
            })
            .collect()
    }

    fn from_index(index: usize) -> Option<Self> {
        OrePoint::iter().nth(index)
    }

    fn from_name(name: &str) -> Option<Self> {
        OrePoint::iter().find(|item| match item.name().split_once(' ') {
            Some((n, _)) => n == name,
            None => false,
        })
    }

    fn name(&self) -> &'static str {
        ""
    }

    fn localized_name(&self, _locale: &str) -> Option<&'static str> {
        None
    }
}

pub async fn init(pool: &PgPool) {
    OreType::init(pool).await;
    OrePoint::init(pool).await;
}
