use std::{collections::BTreeMap, path::PathBuf};

use sea_orm::DbErr;
use serde_json::Value;

use crate::core::{
    commands::CommandSpec,
    entities::{execution_targets, model_backends, model_invocation_profiles, runs},
};

pub fn plan_openfold_command(
    model_backend: &model_backends::Model,
    execution_target: &execution_targets::Model,
    invocation_profile: &model_invocation_profiles::Model,
    run: &runs::Model,
) -> Result<CommandSpec, DbErr> {
    validate_entity_consistency(model_backend, execution_target, invocation_profile, run)?;

    let config = parse_object(
        "model invocation profile config_json",
        &invocation_profile.config_json,
    )?;
    let model_parameters = parse_object("run model_parameters_json", &run.model_parameters_json)?;
    let execution_parameters = parse_object(
        "run execution_parameters_json",
        &run.execution_parameters_json,
    )?;

    let program = required_string(&config, "program")?;
    let script = required_string(&config, "script")?;
    let current_dir = optional_string(&config, "working_dir").map(PathBuf::from);
    let env = parse_env(&config)?;

    let fasta_dir = required_string(&execution_parameters, "fasta_dir")?;
    let output_dir = required_string(&execution_parameters, "output_dir")?;
    let data_dir = required_string(&execution_parameters, "data_dir")?;

    let config_preset =
        optional_string(&model_parameters, "config_preset").unwrap_or_else(|| "model_1_ptm".into());
    let model_device =
        optional_string(&model_parameters, "model_device").unwrap_or_else(|| "cuda:0".into());

    let mut args = vec![
        "-u".into(),
        script,
        fasta_dir,
        data_path(&data_dir, "pdb_mmcif/mmcif_files"),
        "--output_dir".into(),
        output_dir,
        "--config_preset".into(),
        config_preset,
        "--uniref90_database_path".into(),
        data_path(&data_dir, "uniref90/uniref90.fasta"),
        "--mgnify_database_path".into(),
        data_path(&data_dir, "mgnify/mgy_clusters_2022_05.fa"),
        "--pdb70_database_path".into(),
        data_path(&data_dir, "pdb70/pdb70"),
        "--uniclust30_database_path".into(),
        data_path(
            &data_dir,
            "uniclust30/uniclust30_2018_08/uniclust30_2018_08",
        ),
        "--bfd_database_path".into(),
        data_path(
            &data_dir,
            "bfd/bfd_metaclust_clu_complete_id30_c90_final_seq.sorted_opt",
        ),
        "--model_device".into(),
        model_device,
    ];

    if optional_bool(&model_parameters, "save_outputs").unwrap_or(false) {
        args.push("--save_outputs".into());
    }

    if let Some(cpus) = optional_i64(&execution_parameters, "cpus") {
        args.extend(["--cpus".into(), cpus.to_string()]);
    }

    if let Some(attn_map_dir) = optional_string(&execution_parameters, "attn_map_dir") {
        args.extend(["--attn_map_dir".into(), attn_map_dir]);
    }

    if let Some(num_recycles_save) = optional_i64(&model_parameters, "num_recycles_save") {
        args.extend(["--num_recycles_save".into(), num_recycles_save.to_string()]);
    }

    if let Some(residue_idx) = optional_i64(&execution_parameters, "residue_idx") {
        args.extend(["--triangle_residue_idx".into(), residue_idx.to_string()]);
    }

    if optional_bool(&model_parameters, "demo_attn").unwrap_or(false) {
        args.push("--demo_attn".into());
    }

    if optional_bool(&execution_parameters, "use_precomputed_alignments").unwrap_or(false) {
        let alignment_dir = required_string(&execution_parameters, "alignment_dir")?;
        args.extend(["--use_precomputed_alignments".into(), alignment_dir]);
    }

    // The OpenFold script in this repository exposes --config_preset, but not
    // --model_preset. Keep model_preset out of the resolved command for now.

    Ok(CommandSpec {
        program,
        args,
        current_dir,
        env,
    })
}

fn validate_entity_consistency(
    model_backend: &model_backends::Model,
    execution_target: &execution_targets::Model,
    invocation_profile: &model_invocation_profiles::Model,
    run: &runs::Model,
) -> Result<(), DbErr> {
    if run.model_backend_id != model_backend.id {
        return Err(DbErr::Custom(
            "run model_backend_id does not match loaded model backend".into(),
        ));
    }

    if run.execution_target_id != execution_target.id {
        return Err(DbErr::Custom(
            "run execution_target_id does not match loaded execution target".into(),
        ));
    }

    if run.invocation_profile_id != invocation_profile.id {
        return Err(DbErr::Custom(
            "run invocation_profile_id does not match loaded invocation profile".into(),
        ));
    }

    if invocation_profile.model_backend_id != model_backend.id
        || invocation_profile.execution_target_id != execution_target.id
    {
        return Err(DbErr::Custom(
            "model invocation profile does not match loaded model backend and execution target"
                .into(),
        ));
    }

    if invocation_profile.invocation_kind != "local_subprocess" {
        return Err(DbErr::Custom(format!(
            "OpenFold planner only supports local_subprocess invocation profiles, got '{}'",
            invocation_profile.invocation_kind
        )));
    }

    Ok(())
}

fn parse_object(field_name: &str, raw: &str) -> Result<Value, DbErr> {
    let value: Value = serde_json::from_str(raw)
        .map_err(|error| DbErr::Custom(format!("{field_name} must be valid JSON: {error}")))?;

    if !value.is_object() {
        return Err(DbErr::Custom(format!("{field_name} must be a JSON object")));
    }

    Ok(value)
}

fn parse_env(config: &Value) -> Result<BTreeMap<String, String>, DbErr> {
    let Some(env) = config.get("env") else {
        return Ok(BTreeMap::new());
    };

    let Some(env_object) = env.as_object() else {
        return Err(DbErr::Custom("config env must be a JSON object".into()));
    };

    let mut parsed = BTreeMap::new();
    for (key, value) in env_object {
        let Some(value) = value.as_str() else {
            return Err(DbErr::Custom(format!(
                "config env value for '{key}' must be a string"
            )));
        };
        parsed.insert(key.clone(), value.to_owned());
    }

    Ok(parsed)
}

fn required_string(object: &Value, field_name: &str) -> Result<String, DbErr> {
    optional_string(object, field_name)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| DbErr::Custom(format!("{field_name} is required")))
}

fn optional_string(object: &Value, field_name: &str) -> Option<String> {
    object
        .get(field_name)
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn optional_bool(object: &Value, field_name: &str) -> Option<bool> {
    object.get(field_name).and_then(Value::as_bool)
}

fn optional_i64(object: &Value, field_name: &str) -> Option<i64> {
    object.get(field_name).and_then(Value::as_i64)
}

fn data_path(data_dir: &str, suffix: &str) -> String {
    format!("{}/{}", data_dir.trim_end_matches(['/', '\\']), suffix)
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use serde_json::json;

    use crate::core::entities::{
        execution_targets, model_backends, model_invocation_profiles, runs,
    };

    use super::plan_openfold_command;

    fn model_backend() -> model_backends::Model {
        model_backends::Model {
            id: 1,
            slug: "openfold".into(),
            label: "OpenFold".into(),
            version: Some("test".into()),
            description: None,
            artifact_capabilities_json: "{}".into(),
            parameter_schema_json: "{}".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn execution_target() -> execution_targets::Model {
        execution_targets::Model {
            id: 2,
            slug: "local".into(),
            target_type: "local".into(),
            description: None,
            parameter_schema_json: "{}".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn invocation_profile(config_json: String) -> model_invocation_profiles::Model {
        model_invocation_profiles::Model {
            id: 3,
            model_backend_id: 1,
            execution_target_id: 2,
            invocation_kind: "local_subprocess".into(),
            config_json,
            parameter_schema_json: "{}".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn run(model_parameters_json: String, execution_parameters_json: String) -> runs::Model {
        runs::Model {
            id: 4,
            model_backend_id: 1,
            execution_target_id: 2,
            invocation_profile_id: 3,
            status: "submitted".into(),
            input_sequence: "MSTNPKPQRITF".into(),
            model_parameters_json,
            execution_parameters_json,
            submitted_at: Utc::now(),
            started_at: None,
            completed_at: None,
            error_message: None,
        }
    }

    fn config() -> String {
        json!({
            "program": "python3",
            "script": "run_pretrained_openfold.py",
            "working_dir": "/repo",
            "env": {
                "PYTHONPATH": "/repo"
            }
        })
        .to_string()
    }

    fn execution_parameters() -> serde_json::Value {
        json!({
            "fasta_dir": "/tmp/fasta",
            "output_dir": "/tmp/output",
            "data_dir": "/data"
        })
    }

    #[test]
    fn builds_basic_openfold_command_spec() {
        let command = plan_openfold_command(
            &model_backend(),
            &execution_target(),
            &invocation_profile(config()),
            &run(json!({}).to_string(), execution_parameters().to_string()),
        )
        .expect("command should plan");

        assert_eq!(command.program, "python3");
        assert_eq!(command.current_dir, Some("/repo".into()));
        assert_eq!(command.env["PYTHONPATH"], "/repo");
        assert_eq!(command.args[0], "-u");
        assert_eq!(command.args[1], "run_pretrained_openfold.py");
        assert!(command.args.contains(&"/tmp/fasta".into()));
        assert!(command.args.contains(&"/data/pdb_mmcif/mmcif_files".into()));
        assert!(command.args.contains(&"--output_dir".into()));
        assert!(command.args.contains(&"/tmp/output".into()));
        assert!(command.args.contains(&"--config_preset".into()));
        assert!(command.args.contains(&"model_1_ptm".into()));
        assert!(command.args.contains(&"--model_device".into()));
        assert!(command.args.contains(&"cuda:0".into()));
    }

    #[test]
    fn includes_optional_model_parameters_when_present() {
        let command = plan_openfold_command(
            &model_backend(),
            &execution_target(),
            &invocation_profile(config()),
            &run(
                json!({
                    "config_preset": "model_2_ptm",
                    "model_device": "cpu",
                    "save_outputs": true,
                    "num_recycles_save": 1,
                    "model_preset": "monomer"
                })
                .to_string(),
                execution_parameters().to_string(),
            ),
        )
        .expect("command should plan");

        assert!(command.args.contains(&"model_2_ptm".into()));
        assert!(command.args.contains(&"cpu".into()));
        assert!(command.args.contains(&"--save_outputs".into()));
        assert!(command.args.contains(&"--num_recycles_save".into()));
        assert!(command.args.contains(&"1".into()));
        assert!(!command.args.contains(&"--model_preset".into()));
    }

    #[test]
    fn includes_attention_demo_flags_when_enabled() {
        let mut execution = execution_parameters();
        execution["attn_map_dir"] = json!("/tmp/attn");
        execution["residue_idx"] = json!(7);

        let command = plan_openfold_command(
            &model_backend(),
            &execution_target(),
            &invocation_profile(config()),
            &run(
                json!({"demo_attn": true}).to_string(),
                execution.to_string(),
            ),
        )
        .expect("command should plan");

        assert!(command.args.contains(&"--attn_map_dir".into()));
        assert!(command.args.contains(&"/tmp/attn".into()));
        assert!(command.args.contains(&"--triangle_residue_idx".into()));
        assert!(command.args.contains(&"7".into()));
        assert!(command.args.contains(&"--demo_attn".into()));
    }

    #[test]
    fn includes_precomputed_alignment_flags_when_requested() {
        let mut execution = execution_parameters();
        execution["use_precomputed_alignments"] = json!(true);
        execution["alignment_dir"] = json!("/tmp/alignments");

        let command = plan_openfold_command(
            &model_backend(),
            &execution_target(),
            &invocation_profile(config()),
            &run(json!({}).to_string(), execution.to_string()),
        )
        .expect("command should plan");

        assert!(
            command
                .args
                .contains(&"--use_precomputed_alignments".into())
        );
        assert!(command.args.contains(&"/tmp/alignments".into()));
    }

    #[test]
    fn returns_clear_error_when_required_config_field_is_missing() {
        let error = plan_openfold_command(
            &model_backend(),
            &execution_target(),
            &invocation_profile(json!({"script": "run_pretrained_openfold.py"}).to_string()),
            &run(json!({}).to_string(), execution_parameters().to_string()),
        )
        .expect_err("missing program should fail");

        assert!(error.to_string().contains("program is required"));
    }

    #[test]
    fn returns_clear_error_when_required_execution_parameter_is_missing() {
        let error = plan_openfold_command(
            &model_backend(),
            &execution_target(),
            &invocation_profile(config()),
            &run(
                json!({}).to_string(),
                json!({
                    "fasta_dir": "/tmp/fasta",
                    "output_dir": "/tmp/output"
                })
                .to_string(),
            ),
        )
        .expect_err("missing data_dir should fail");

        assert!(error.to_string().contains("data_dir is required"));
    }

    #[test]
    fn validates_env_is_string_to_string_object() {
        let error = plan_openfold_command(
            &model_backend(),
            &execution_target(),
            &invocation_profile(
                json!({
                    "program": "python3",
                    "script": "run_pretrained_openfold.py",
                    "env": {"PYTHONPATH": 123}
                })
                .to_string(),
            ),
            &run(json!({}).to_string(), execution_parameters().to_string()),
        )
        .expect_err("non-string env values should fail");

        assert!(
            error
                .to_string()
                .contains("config env value for 'PYTHONPATH' must be a string")
        );
    }

    #[test]
    fn rejects_non_local_subprocess_invocation_profile() {
        let mut profile = invocation_profile(config());
        profile.invocation_kind = "docker".into();

        let error = plan_openfold_command(
            &model_backend(),
            &execution_target(),
            &profile,
            &run(json!({}).to_string(), execution_parameters().to_string()),
        )
        .expect_err("unsupported invocation kind should fail");

        assert!(
            error
                .to_string()
                .contains("only supports local_subprocess invocation profiles")
        );
    }
}
