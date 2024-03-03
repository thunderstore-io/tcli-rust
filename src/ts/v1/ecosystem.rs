use crate::error::Error;
use crate::ts::v1::models::ecosystem::EcosystemSchema;
use crate::ts::CLIENT;

pub async fn get_schema() -> Result<EcosystemSchema, Error> {
    Ok(CLIENT
        .get("https://thunderstore.io/api/experimental/schema/dev/latest/")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}
