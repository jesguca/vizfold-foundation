use std::{env, path::PathBuf};

use executor::core::{
    config, db,
    entities::{
        artifacts as artifact_entities, execution_targets, model_backends,
        model_invocation_profiles, runs,
    },
    repositories, seed,
    services::artifacts::{self, RecordArtifactByTypeSlugInput},
};
use sea_orm::{ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), DbErr> {
    let paths = DemoPaths::from_environment();
    let database_url = config::database_url();
    let db = db::connect_and_migrate().await?;
    seed::seed_defaults(&db).await?;

    let run = find_or_create_demo_run(&db, &paths).await?;
    let mut summary = RegistrationSummary::default();
    register_directory(
        &db,
        run.id,
        "run_output_directory",
        &paths.output_dir,
        json!({
            "input_id": paths.input_id,
            "source": "register_openfold_demo_artifacts",
        }),
        &mut summary,
    )
    .await?;
    register_directory(
        &db,
        run.id,
        "attention_output_directory",
        &paths.attn_map_dir,
        json!({
            "input_id": paths.input_id,
            "source": "register_openfold_demo_artifacts",
            "triangle_residue_idx": paths.residue_idx,
        }),
        &mut summary,
    )
    .await?;

    println!("== OpenFold demo artifact registration ==");
    println!("database: {database_url}");
    println!("demo_run_id: {}", paths.demo_run_id);
    println!("run id used: {}", run.id);
    println!(
        "output_dir scanned/registered: {}",
        paths.output_dir.display()
    );
    println!(
        "attn_map_dir scanned/registered: {}",
        paths.attn_map_dir.display()
    );
    println!("artifacts created: {}", summary.created);
    println!(
        "artifacts skipped (already registered): {}",
        summary.already_registered
    );
    println!("missing directories skipped: {}", summary.missing);
    Ok(())
}

struct DemoPaths {
    demo_run_id: String,
    input_id: String,
    residue_idx: i64,
    output_dir: PathBuf,
    attn_map_dir: PathBuf,
}

impl DemoPaths {
    fn from_environment() -> Self {
        let repository_root = repository_root();
        let demo_run_id = env_or_value("VIZFOLD_OPENFOLD_DEMO_RUN_ID", "openfold-demo-run");
        let input_id = env_or_value("VIZFOLD_OPENFOLD_INPUT_ID", "6KWC_1");
        let residue_idx = env_or_i64("VIZFOLD_OPENFOLD_RESIDUE_IDX", 1);
        let output_dir = env_or_path(
            "VIZFOLD_OPENFOLD_OUTPUT_DIR",
            repository_root
                .join("science-gateway")
                .join("openfold-demo-output")
                .join(&demo_run_id),
        );
        let attn_map_dir = env_or_path(
            "VIZFOLD_OPENFOLD_ATTN_MAP_DIR",
            output_dir.join(format!("attention_files_{input_id}_demo_tri_{residue_idx}")),
        );

        Self {
            demo_run_id,
            input_id,
            residue_idx,
            output_dir,
            attn_map_dir,
        }
    }
}

#[derive(Default)]
struct RegistrationSummary {
    created: usize,
    already_registered: usize,
    missing: usize,
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("executor manifest should be nested under the repository root")
        .to_path_buf()
}

fn env_or_path(name: &str, default: impl Into<PathBuf>) -> PathBuf {
    env::var(name)
        .map(PathBuf::from)
        .unwrap_or_else(|_| default.into())
}

fn env_or_value(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.into())
}

fn env_or_i64(name: &str, default: i64) -> i64 {
    match env::var(name) {
        Ok(value) => value
            .parse()
            .unwrap_or_else(|_| panic!("{name} must be an integer")),
        Err(_) => default,
    }
}

async fn find_or_create_demo_run(
    db: &DatabaseConnection,
    paths: &DemoPaths,
) -> Result<runs::Model, DbErr> {
    // Runs have numeric IDs and no external run-key column. This example therefore
    // identifies its placeholder by input_id plus the recorded output_dir.
    if let Some(run) = runs::Entity::find()
        .filter(runs::Column::InputId.eq(&paths.input_id))
        .all(db)
        .await?
        .into_iter()
        .find(|run| execution_output_dir(run).as_deref() == Some(paths.output_dir.as_path()))
    {
        return Ok(run);
    }

    let backend = model_backends::Entity::find()
        .filter(model_backends::Column::Slug.eq("openfold"))
        .one(db)
        .await?
        .ok_or_else(|| DbErr::Custom("seeded OpenFold backend is missing".into()))?;
    let target = execution_targets::Entity::find()
        .filter(execution_targets::Column::Slug.eq("local-mock"))
        .one(db)
        .await?
        .ok_or_else(|| DbErr::Custom("seeded local target is missing".into()))?;
    let profile = model_invocation_profiles::Entity::find()
        .filter(model_invocation_profiles::Column::ModelBackendId.eq(backend.id))
        .filter(model_invocation_profiles::Column::ExecutionTargetId.eq(target.id))
        .one(db)
        .await?
        .ok_or_else(|| DbErr::Custom("seeded invocation profile is missing".into()))?;

    // This record is solely an artifact-indexing placeholder; it is never submitted
    // to the OpenFold planner or executor.
    repositories::runs::create(
        db,
        executor::core::services::runs::SubmitRunInput {
            model_backend_id: backend.id,
            execution_target_id: target.id,
            invocation_profile_id: profile.id,
            status: "artifact_indexed".into(),
            input_id: paths.input_id.clone(),
            input_sequence: "artifact-indexing-placeholder".into(),
            model_parameters_json: json!({}).to_string(),
            execution_parameters_json: json!({
                "demo_run_id": paths.demo_run_id,
                "output_dir": paths.output_dir,
                "attn_map_dir": paths.attn_map_dir,
            })
            .to_string(),
        },
    )
    .await
}

fn execution_output_dir(run: &runs::Model) -> Option<PathBuf> {
    serde_json::from_str::<serde_json::Value>(&run.execution_parameters_json)
        .ok()?
        .get("output_dir")?
        .as_str()
        .map(PathBuf::from)
}

async fn register_directory(
    db: &DatabaseConnection,
    run_id: i32,
    artifact_type_slug: &str,
    path: &std::path::Path,
    metadata: serde_json::Value,
    summary: &mut RegistrationSummary,
) -> Result<(), DbErr> {
    if !path.is_dir() {
        summary.missing += 1;
        return Ok(());
    }

    let artifact_type =
        executor::core::services::artifact_types::get_artifact_type_by_slug(db, artifact_type_slug)
            .await?
            .ok_or_else(|| {
                DbErr::Custom(format!("artifact type '{artifact_type_slug}' is missing"))
            })?;
    let storage_uri = path.display().to_string();
    let already_registered = artifact_entities::Entity::find()
        .filter(artifact_entities::Column::RunId.eq(run_id))
        .filter(artifact_entities::Column::ArtifactTypeId.eq(artifact_type.id))
        .filter(artifact_entities::Column::StorageUri.eq(&storage_uri))
        .one(db)
        .await?
        .is_some();

    if already_registered {
        summary.already_registered += 1;
        return Ok(());
    }

    artifacts::record_artifact_manifest_entry_by_type_slug(
        db,
        RecordArtifactByTypeSlugInput {
            run_id,
            artifact_type_slug: artifact_type_slug.into(),
            format: "directory".into(),
            storage_uri,
            metadata_json: metadata.to_string(),
        },
    )
    .await?;
    summary.created += 1;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{ConnectionTrait, Database, Statement};

    async fn test_db() -> Result<DatabaseConnection, DbErr> {
        let db = Database::connect("sqlite::memory:").await?;
        db.execute(Statement::from_string(
            db.get_database_backend(),
            "PRAGMA foreign_keys = ON".to_owned(),
        ))
        .await?;
        executor::core::db::migrate_database(&db).await?;
        seed::seed_defaults(&db).await?;
        Ok(db)
    }

    #[tokio::test]
    async fn skips_missing_paths() -> Result<(), DbErr> {
        let db = test_db().await?;
        let mut summary = RegistrationSummary::default();
        register_directory(
            &db,
            1,
            "run_output_directory",
            std::path::Path::new("definitely-missing-output-directory"),
            json!({}),
            &mut summary,
        )
        .await?;
        assert_eq!(summary.missing, 1);
        Ok(())
    }

    #[tokio::test]
    async fn registers_existing_directories_idempotently() -> Result<(), DbErr> {
        let db = test_db().await?;
        let existing_directory = env::current_dir().expect("current directory should be available");
        let paths = DemoPaths {
            demo_run_id: "test-run".into(),
            input_id: "test-input".into(),
            residue_idx: 1,
            output_dir: existing_directory.clone(),
            attn_map_dir: existing_directory.clone(),
        };
        let run = find_or_create_demo_run(&db, &paths).await?;
        let mut summary = RegistrationSummary::default();

        register_directory(
            &db,
            run.id,
            "run_output_directory",
            &existing_directory,
            json!({"input_id": "test-input"}),
            &mut summary,
        )
        .await?;
        register_directory(
            &db,
            run.id,
            "run_output_directory",
            &existing_directory,
            json!({"input_id": "test-input"}),
            &mut summary,
        )
        .await?;

        assert_eq!(summary.created, 1);
        assert_eq!(summary.already_registered, 1);
        assert_eq!(summary.missing, 0);
        let registered = artifact_entities::Entity::find()
            .filter(artifact_entities::Column::RunId.eq(run.id))
            .all(&db)
            .await?;
        assert_eq!(registered.len(), 1);
        assert_eq!(registered[0].format, "directory");
        assert_eq!(
            registered[0].storage_uri,
            existing_directory.display().to_string()
        );
        Ok(())
    }
}
