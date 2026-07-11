use sea_orm::{ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter};

use crate::core::{
    entities::{execution_targets, model_backends},
    services,
};

pub async fn seed_defaults(db: &DatabaseConnection) -> Result<(), DbErr> {
    if model_backends::Entity::find()
        .filter(model_backends::Column::Slug.eq("openfold"))
        .one(db)
        .await?
        .is_none()
    {
        services::model_backends::register_model_backend(
            db,
            services::model_backends::RegisterModelBackendInput {
                slug: "openfold".into(),
                label: "OpenFold".into(),
                version: Some("demo".into()),
                description: Some("OpenFold backend placeholder for executor core development.".into()),
                capabilities_json: r#"{"family":"structure_prediction"}"#.into(),
                artifact_capabilities_json:
                    r#"{"structure":{"formats":["pdb","cif"],"required":true},"confidence_metrics":{"formats":["json"],"required":false}}"#
                        .into(),
                parameter_schema_json:
                    r#"{"type":"object","properties":{"num_recycles":{"type":"integer","minimum":0,"default":3}}}"#
                        .into(),
            },
        )
        .await?;
    }

    if execution_targets::Entity::find()
        .filter(execution_targets::Column::Slug.eq("local-mock"))
        .one(db)
        .await?
        .is_none()
    {
        services::execution_targets::register_execution_target(
            db,
            services::execution_targets::RegisterExecutionTargetInput {
                slug: "local-mock".into(),
                label: "Local Mock".into(),
                target_type: "local".into(),
                description: Some("Local mock execution target for development and tests.".into()),
                parameter_schema_json:
                    r#"{"type":"object","properties":{"gpu_count":{"type":"integer","minimum":0,"default":0}}}"#
                        .into(),
            },
        )
        .await?;
    }

    Ok(())
}
