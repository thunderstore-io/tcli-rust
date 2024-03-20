use crate::error::Error;
use crate::ts::package_reference::PackageReference;
use crate::ts::v1::models::package::PackageListing;
use crate::ts::{CLIENT, CM, V1};

pub async fn get_all() -> Result<Vec<PackageListing>, Error> {
    Ok(CLIENT
        .get(format!("{V1}/package/"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

pub async fn get_community_all(community: &str) -> Result<Vec<PackageListing>, Error> {
    Ok(CLIENT
        .get(format!("{CM}/{community}/api/v1/package/"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

pub fn download_for_package(ident: &PackageReference) -> String {
    format!("https://thunderstore.io/package/download/{}/{}/{}/", ident.namespace, ident.name, ident.version)
}
