#![cfg(test)]

use sea_orm::{ConnectionTrait, Database, DatabaseConnection, DbErr, Statement};
use serde_json::json;

use crate::core::{
    db,
    services::{
        artifacts::{self, RecordArtifactInput},
        execution_targets::{self, RegisterExecutionTargetInput},
        model_backends::{self, RegisterModelBackendInput},
        runs::{self, SubmitRunInput, UpdateRunStatusInput},
    },
};

async fn test_db() -> Result<DatabaseConnection, DbErr> {
    let db = Database::connect("sqlite::memory:").await?;
    db.execute(Statement::from_string(
        db.get_database_backend(),
        "PRAGMA foreign_keys = ON".to_owned(),
    ))
    .await?;
    db::migrate_database(&db).await?;
    Ok(db)
}

fn sample_model_backend_input() -> RegisterModelBackendInput {
    RegisterModelBackendInput {
        slug: "openfold".into(),
        label: "OpenFold".into(),
        version: Some("1.0".into()),
        description: Some("Reference folding backend".into()),
        capabilities_json: json!({"family": "structure_prediction"}).to_string(),
        artifact_capabilities_json: json!({
            "structure": { "formats": ["pdb", "cif"], "required": true },
            "confidence_metrics": { "formats": ["json"], "required": false }
        })
        .to_string(),
        parameter_schema_json: json!({
            "type": "object",
            "properties": {
                "num_recycles": { "type": "integer", "minimum": 0, "default": 3 }
            }
        })
        .to_string(),
    }
}

fn sample_execution_target_input() -> RegisterExecutionTargetInput {
    RegisterExecutionTargetInput {
        slug: "local-mock".into(),
        label: "Local Mock".into(),
        target_type: "local".into(),
        description: Some("Test execution target".into()),
        parameter_schema_json: json!({
            "type": "object",
            "properties": {
                "gpu_count": { "type": "integer", "minimum": 0, "default": 0 },
                "walltime": { "type": "string", "default": "00:05:00" }
            }
        })
        .to_string(),
    }
}

#[tokio::test]
async fn creates_model_backend() -> Result<(), DbErr> {
    let db = test_db().await?;

    let backend = model_backends::register_model_backend(&db, sample_model_backend_input()).await?;

    assert_eq!(backend.slug, "openfold");
    assert!(
        serde_json::from_str::<serde_json::Value>(&backend.capabilities_json)
            .expect("capabilities_json should parse")
            .is_object()
    );
    assert!(
        serde_json::from_str::<serde_json::Value>(&backend.artifact_capabilities_json)
            .expect("artifact_capabilities_json should parse")
            .is_object()
    );
    Ok(())
}

#[tokio::test]
async fn creates_execution_target() -> Result<(), DbErr> {
    let db = test_db().await?;

    let target =
        execution_targets::register_execution_target(&db, sample_execution_target_input()).await?;

    assert_eq!(target.slug, "local-mock");
    assert_eq!(target.target_type, "local");
    Ok(())
}

#[tokio::test]
async fn creates_run_with_separate_parameter_sets() -> Result<(), DbErr> {
    let db = test_db().await?;
    let backend = model_backends::register_model_backend(&db, sample_model_backend_input()).await?;
    let target =
        execution_targets::register_execution_target(&db, sample_execution_target_input()).await?;

    let run = runs::submit_run(
        &db,
        SubmitRunInput {
            model_backend_id: backend.id,
            execution_target_id: target.id,
            status: "submitted".into(),
            input_sequence: "MSTNPKPQRITF".into(),
            model_parameters_json: json!({"num_recycles": 5}).to_string(),
            execution_parameters_json: json!({"gpu_count": 1, "walltime": "02:00:00"}).to_string(),
        },
    )
    .await?;

    assert_eq!(run.model_backend_id, backend.id);
    assert_eq!(run.execution_target_id, target.id);
    assert!(run.started_at.is_none());
    Ok(())
}

#[tokio::test]
async fn rejects_non_object_json_parameters() -> Result<(), DbErr> {
    let db = test_db().await?;
    let backend = model_backends::register_model_backend(&db, sample_model_backend_input()).await?;
    let target =
        execution_targets::register_execution_target(&db, sample_execution_target_input()).await?;

    let error = runs::submit_run(
        &db,
        SubmitRunInput {
            model_backend_id: backend.id,
            execution_target_id: target.id,
            status: "submitted".into(),
            input_sequence: "MSTNPKPQRITF".into(),
            model_parameters_json: "[]".into(),
            execution_parameters_json: json!({"gpu_count": 1}).to_string(),
        },
    )
    .await
    .expect_err("non-object model parameters should fail");

    assert!(
        error
            .to_string()
            .contains("model_parameters must be a JSON object")
    );
    Ok(())
}

#[tokio::test]
async fn records_artifact_manifest_entry() -> Result<(), DbErr> {
    let db = test_db().await?;
    let backend = model_backends::register_model_backend(&db, sample_model_backend_input()).await?;
    let target =
        execution_targets::register_execution_target(&db, sample_execution_target_input()).await?;
    let run = runs::submit_run(
        &db,
        SubmitRunInput {
            model_backend_id: backend.id,
            execution_target_id: target.id,
            status: "submitted".into(),
            input_sequence: "MSTNPKPQRITF".into(),
            model_parameters_json: json!({"num_recycles": 2}).to_string(),
            execution_parameters_json: json!({"gpu_count": 0}).to_string(),
        },
    )
    .await?;

    let artifact = artifacts::record_artifact_manifest_entry(
        &db,
        RecordArtifactInput {
            run_id: run.id,
            artifact_type: "structure".into(),
            format: "pdb".into(),
            storage_uri: "file:///tmp/run-1/model.pdb".into(),
            metadata_json: json!({"bytes": 1280, "sha256": "abc123"}).to_string(),
        },
    )
    .await?;

    assert_eq!(artifact.storage_uri, "file:///tmp/run-1/model.pdb");
    assert!(artifact.metadata_json.contains("sha256"));
    Ok(())
}

#[tokio::test]
async fn retrieves_run_with_artifacts() -> Result<(), DbErr> {
    let db = test_db().await?;
    let backend = model_backends::register_model_backend(&db, sample_model_backend_input()).await?;
    let target =
        execution_targets::register_execution_target(&db, sample_execution_target_input()).await?;
    let run = runs::submit_run(
        &db,
        SubmitRunInput {
            model_backend_id: backend.id,
            execution_target_id: target.id,
            status: "submitted".into(),
            input_sequence: "MSTNPKPQRITF".into(),
            model_parameters_json: json!({"num_recycles": 2}).to_string(),
            execution_parameters_json: json!({"gpu_count": 0}).to_string(),
        },
    )
    .await?;

    let _artifact = artifacts::record_artifact_manifest_entry(
        &db,
        RecordArtifactInput {
            run_id: run.id,
            artifact_type: "logs".into(),
            format: "txt".into(),
            storage_uri: "file:///tmp/run-1/stdout.log".into(),
            metadata_json: json!({"line_count": 42}).to_string(),
        },
    )
    .await?;

    let hydrated = runs::get_run_with_artifacts(&db, run.id)
        .await?
        .expect("run should exist");

    assert_eq!(hydrated.run.id, run.id);
    assert_eq!(hydrated.artifacts.len(), 1);
    Ok(())
}

#[tokio::test]
async fn artifact_manifest_stores_uri_and_metadata_only() -> Result<(), DbErr> {
    let db = test_db().await?;
    let backend = model_backends::register_model_backend(&db, sample_model_backend_input()).await?;
    let target =
        execution_targets::register_execution_target(&db, sample_execution_target_input()).await?;
    let run = runs::submit_run(
        &db,
        SubmitRunInput {
            model_backend_id: backend.id,
            execution_target_id: target.id,
            status: "submitted".into(),
            input_sequence: "MSTNPKPQRITF".into(),
            model_parameters_json: json!({"num_recycles": 2}).to_string(),
            execution_parameters_json: json!({"gpu_count": 0}).to_string(),
        },
    )
    .await?;

    let artifact = artifacts::record_artifact_manifest_entry(
        &db,
        RecordArtifactInput {
            run_id: run.id,
            artifact_type: "confidence_metrics".into(),
            format: "json".into(),
            storage_uri: "s3://vizfold/runs/1/confidence.json".into(),
            metadata_json: json!({"bytes": 256, "content_type": "application/json"}).to_string(),
        },
    )
    .await?;

    assert_eq!(artifact.format, "json");
    assert!(!artifact.metadata_json.contains("ATOM"));
    assert!(!artifact.storage_uri.starts_with("{"));
    Ok(())
}

#[tokio::test]
async fn updates_run_status() -> Result<(), DbErr> {
    let db = test_db().await?;
    let backend = model_backends::register_model_backend(&db, sample_model_backend_input()).await?;
    let target =
        execution_targets::register_execution_target(&db, sample_execution_target_input()).await?;
    let run = runs::submit_run(
        &db,
        SubmitRunInput {
            model_backend_id: backend.id,
            execution_target_id: target.id,
            status: "submitted".into(),
            input_sequence: "MSTNPKPQRITF".into(),
            model_parameters_json: json!({"num_recycles": 2}).to_string(),
            execution_parameters_json: json!({"gpu_count": 0}).to_string(),
        },
    )
    .await?;

    let updated = runs::update_run_status(
        &db,
        run.id,
        UpdateRunStatusInput {
            status: "completed".into(),
            completed_at: Some(Some(chrono::Utc::now())),
            ..Default::default()
        },
    )
    .await?;

    assert_eq!(updated.status, "completed");
    assert!(updated.completed_at.is_some());
    Ok(())
}
