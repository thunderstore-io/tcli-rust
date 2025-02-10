use std::path::PathBuf;

use crate::error::{IoError, Error};
use crate::project::manifest::ProjectManifest;
use crate::ts::experimental::models::publish::PackageSubmissionMetadata;
use crate::ts::experimental::publish;

use super::error::ProjectError;

pub async fn publish(
    manifest: &ProjectManifest,
    archive_path: PathBuf,
) -> Result<(), Error> {
    let package = manifest
        .package
        .as_ref()
        .ok_or(ProjectError::MissingTable("package"))?;

    if !archive_path.is_file() {
        Err(IoError::FileNotFound(archive_path.clone()))?;
    }

    let publish = manifest.publish.as_ref().unwrap();

    let usermedia = publish::upload_file(archive_path).await?;
    publish::package_submit(&PackageSubmissionMetadata {
        author_name: package.namespace.to_string(),
        communities: publish
            .iter()
            .map(|p| p.community.clone())
            .collect(),
        has_nsfw_content: package.contains_nsfw_content,
        community_categories: publish
            .iter()
            .map(|p| (p.community.clone(), p.categories.clone()))
            .collect(),
        upload_uuid: usermedia.uuid,
    })
    .await?;

    Ok(())
}
