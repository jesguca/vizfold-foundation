use sea_orm::{DatabaseConnection, DbErr};

use crate::core::{entities::artifacts, repositories};

use super::validation::require_json_object;

#[derive(Clone, Debug)]
pub struct RecordArtifactInput {
    pub run_id: i32,
    pub artifact_type: String,
    pub format: String,
    pub storage_uri: String,
    pub metadata_json: String,
}

pub async fn list_artifacts_for_run(
    db: &DatabaseConnection,
    run_id: i32,
) -> Result<Vec<artifacts::Model>, DbErr> {
    repositories::artifacts::list_by_run(db, run_id).await
}

pub async fn record_artifact_manifest_entry(
    db: &DatabaseConnection,
    input: RecordArtifactInput,
) -> Result<artifacts::Model, DbErr> {
    require_json_object("artifact metadata", &input.metadata_json)?;

    repositories::artifacts::create(db, input).await
}
