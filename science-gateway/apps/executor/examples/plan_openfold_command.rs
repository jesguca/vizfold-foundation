use chrono::Utc;
use executor::core::{
    entities::{execution_targets, model_backends, model_invocation_profiles, runs},
    model_runners::openfold::plan_openfold_command,
};
use serde_json::json;

fn main() -> Result<(), sea_orm::DbErr> {
    let now = Utc::now();

    let model_backend = model_backends::Model {
        id: 1,
        slug: "openfold".into(),
        label: "OpenFold".into(),
        version: Some("example".into()),
        description: None,
        artifact_capabilities_json: "{}".into(),
        parameter_schema_json: "{}".into(),
        created_at: now,
        updated_at: now,
    };

    let execution_target = execution_targets::Model {
        id: 2,
        slug: "local-runtime".into(),
        target_type: "local".into(),
        description: None,
        parameter_schema_json: "{}".into(),
        created_at: now,
        updated_at: now,
    };

    let invocation_profile = model_invocation_profiles::Model {
        id: 3,
        model_backend_id: model_backend.id,
        execution_target_id: execution_target.id,
        invocation_kind: "local_subprocess".into(),
        config_json: json!({
            "program": "python3",
            "script": "run_pretrained_openfold.py",
            "working_dir": "/path/to/vizfold-foundation",
            "env": {
                "PYTHONPATH": "/path/to/vizfold-foundation"
            }
        })
        .to_string(),
        parameter_schema_json: "{}".into(),
        created_at: now,
        updated_at: now,
    };

    let run = runs::Model {
        id: 4,
        model_backend_id: model_backend.id,
        execution_target_id: execution_target.id,
        invocation_profile_id: invocation_profile.id,
        status: "submitted".into(),
        input_sequence: "MSTNPKPQRITF".into(),
        model_parameters_json: json!({
            "config_preset": "model_1_ptm",
            "model_device": "cuda:0",
            "save_outputs": true,
            "demo_attn": true,
            "num_recycles_save": 1
        })
        .to_string(),
        execution_parameters_json: json!({
            "fasta_dir": "/tmp/vizfold/fasta",
            "output_dir": "/tmp/vizfold/output",
            "data_dir": "/data/openfold",
            "attn_map_dir": "/tmp/vizfold/attention",
            "use_precomputed_alignments": true,
            "alignment_dir": "/tmp/vizfold/alignments",
            "cpus": 14,
            "residue_idx": 1
        })
        .to_string(),
        submitted_at: now,
        started_at: None,
        completed_at: None,
        error_message: None,
    };

    let command =
        plan_openfold_command(&model_backend, &execution_target, &invocation_profile, &run)?;

    println!("{command:#?}");

    Ok(())
}
