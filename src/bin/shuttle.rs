use std::net::SocketAddr;

use shuttle_runtime::{SecretStore, Service, Error as ShuttleError};
use poise::serenity_prelude as serenity;
#[shuttle_runtime::main]
async fn main(
    #[shuttle_runtime::Secrets] secrets: SecretStore,
    #[shuttle_shared_db::Postgres(local_uri = "{secrets.POSTGRESQL_URI}")] pool: sqlx::PgPool,
) -> Result<impl Service, ShuttleError> {
    let client = pal_occupy_bot::init(secrets.into_iter().collect(), pool).await?;
    Ok(BotService {
        client
    })

}
struct BotService {
    client: serenity::Client,
}

#[shuttle_runtime::async_trait]
impl Service for BotService {
    async fn bind(self, _addr: SocketAddr) -> Result<(), ShuttleError> {
        let Self { client } = self;
        pal_occupy_bot::bind(client).await?;
        Ok(())
    }
}