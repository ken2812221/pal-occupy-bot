use crate::db;
use once_cell::sync::OnceCell;
use sqlx::{prelude::FromRow, PgPool};

#[derive(PartialEq, Eq, Clone, Debug, FromRow)]
pub(crate) struct OreType {
    pub id: i32,
    pub name: String,
    pub emoji: String,
}

#[derive(Clone, Debug, FromRow)]
pub(crate) struct OrePoint {
    pub id: i32,
    pub ore_type: i32,
    pub x: i32,
    pub y: i32,
    pub name: String,
}

static ORE_POINTS: OnceCell<Vec<OrePoint>> = OnceCell::new();
static ORE_TYPES: OnceCell<Vec<OreType>> = OnceCell::new();

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

    pub fn emoji(&self) -> String {
        OreType::iter()
            .filter(|ore_type| (ore_type.id & self.ore_type) != 0)
            .map(|ore_type| format!("<{}>", ore_type.emoji))
            .collect::<String>()
    }
}

impl OreType {
    async fn init(pool: &PgPool) {
        let data = db::get_ore_types(pool).await.expect("Cannot get ORE_TYPES");
        ORE_TYPES.set(data).expect("ORE_TYPES set more than once.");
    }

    pub fn iter() -> impl Iterator<Item = &'static Self> {
        ORE_TYPES.get().expect("ORE_TYPES not initialize").iter()
    }
}

pub async fn init(pool: &PgPool) {
    OreType::init(pool).await;
    OrePoint::init(pool).await;
}
