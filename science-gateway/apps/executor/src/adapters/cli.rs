use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use chrono::Utc;
use sea_orm::DbErr;
use serde_json::{Map, Value, json};

use crate::core::{
    commands::{CommandSpec, LocalCommandRunner},
    entities::{execution_targets, model_backends, model_invocation_profiles, runs as run_entity},
    execution::{ExecutionCore, ExecutionWorkflowResult, execute_command_workflow},
    model_runners::openfold::{
        OpenFoldPreflightRunner, fasta_files_in_directory, plan_openfold_command,
    },
    output_locations::resolve_output_location,
    preflight::{PreflightReport, PreflightRunner, PreflightStatus},
    seed,
    services::runs::{self, SubmitRunInput, UpdateRunStatusInput},
};

pub const USAGE: &str = "\
usage: executor run [options]

Submits a run against the seeded OpenFold backend, plans the command from the
seeded invocation profile, preflights it, then executes it locally.

Every option falls back to the matching VIZFOLD_OPENFOLD_* environment variable,
so --data-dir reads VIZFOLD_OPENFOLD_DATA_DIR when it is not passed.

  --input-id <id>          FASTA record to fold                       (required)
  --fasta-dir <dir>        directory holding exactly one FASTA        (required)
  --data-dir <dir>         OpenFold template/MSA dataset root         (required)
  --alignment-dir <dir>    reuse precomputed alignments from here
  --model-device <device>  cpu or cuda:0
  --cpus <n>               CPUs handed to the OpenFold process
  --residue-idx <n>        residue to emit attention maps for
  --config-preset <name>   OpenFold config preset
  --save-outputs           keep OpenFold's intermediate outputs
  --demo-attn              emit the attention-map demo artifacts
  --dry-run                plan and preflight only, do not execute
  --help                   show this message
";

const TAKES_VALUE: bool = true;
const FLAG: bool = false;

const OPTIONS: [(&str, bool); 12] = [
    ("input-id", TAKES_VALUE),
    ("fasta-dir", TAKES_VALUE),
    ("data-dir", TAKES_VALUE),
    ("alignment-dir", TAKES_VALUE),
    ("model-device", TAKES_VALUE),
    ("cpus", TAKES_VALUE),
    ("residue-idx", TAKES_VALUE),
    ("config-preset", TAKES_VALUE),
    ("save-outputs", FLAG),
    ("demo-attn", FLAG),
    ("dry-run", FLAG),
    ("help", FLAG),
];

pub async fn main(argv: impl Iterator<Item = String>) -> ! {
    let options = match parse_args(argv) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("error: {message}\n\n{USAGE}");
            std::process::exit(2);
        }
    };

    if options.contains_key("help") {
        println!("{USAGE}");
        std::process::exit(0);
    }

    match execute(&options).await {
        Ok(exit_code) => std::process::exit(exit_code),
        Err(error) => {
            eprintln!("error: {error}");
            std::process::exit(1);
        }
    }
}

fn parse_args(mut argv: impl Iterator<Item = String>) -> Result<BTreeMap<String, String>, String> {
    let mut options = BTreeMap::new();

    while let Some(argument) = argv.next() {
        let name = argument
            .strip_prefix("--")
            .ok_or_else(|| format!("unexpected argument '{argument}'"))?;
        let (_, takes_value) = OPTIONS
            .iter()
            .find(|(option, _)| *option == name)
            .ok_or_else(|| format!("unknown option '--{name}'"))?;

        let value = if *takes_value {
            argv.next()
                .ok_or_else(|| format!("--{name} requires a value"))?
        } else {
            "true".into()
        };
        options.insert(name.to_owned(), value);
    }

    Ok(options)
}

/// Reuses the `VIZFOLD_OPENFOLD_*` names the demo examples already use.
fn option(options: &BTreeMap<String, String>, name: &str) -> Option<String> {
    options
        .get(name)
        .cloned()
        .or_else(|| {
            std::env::var(format!(
                "VIZFOLD_OPENFOLD_{}",
                name.replace('-', "_").to_uppercase()
            ))
            .ok()
        })
        .filter(|value| !value.trim().is_empty())
}

fn required(options: &BTreeMap<String, String>, name: &str) -> Result<String, DbErr> {
    option(options, name).ok_or_else(|| DbErr::Custom(format!("--{name} is required")))
}

/// Run-shaping flags only: `--help` and `--dry-run` describe the invocation.
fn flag(options: &BTreeMap<String, String>, name: &str) -> bool {
    option(options, name).is_some_and(|value| !matches!(value.as_str(), "false" | "0"))
}

fn integer(options: &BTreeMap<String, String>, name: &str) -> Result<Option<i64>, DbErr> {
    option(options, name)
        .map(|value| {
            value
                .parse()
                .map_err(|_| DbErr::Custom(format!("--{name} must be an integer, got '{value}'")))
        })
        .transpose()
}

async fn execute(options: &BTreeMap<String, String>) -> Result<i32, DbErr> {
    let core = ExecutionCore::bootstrap().await?;
    let db = core.db();

    let (backend, target, profile) = seed::seeded_openfold(db).await?;

    let fasta_dir = required(options, "fasta-dir")?;
    let run = runs::submit_run(
        db,
        SubmitRunInput {
            model_backend_id: backend.id,
            execution_target_id: target.id,
            invocation_profile_id: profile.id,
            status: "submitted".into(),
            input_id: required(options, "input-id")?,
            input_sequence: read_fasta_sequence(&fasta_dir)?,
            model_parameters_json: Value::Object(model_parameters(options)).to_string(),
            execution_parameters_json: Value::Object(execution_parameters(options, &fasta_dir)?)
                .to_string(),
        },
    )
    .await?;

    // Stays "submitted": nothing executed, so no terminal status would be true.
    if options.contains_key("dry-run") {
        let (command, _) = plan(&backend, &target, &profile, &run)?;
        let report = OpenFoldPreflightRunner {
            command: &command,
            invocation_profile: &profile,
            run: &run,
        }
        .run_preflight()?;
        print_report(&report);

        return Ok(i32::from(report.has_failures()));
    }

    // The row exists now, so every exit below must land a status on it.
    let (exit_code, failure) = match run_openfold(&backend, &target, &profile, &run).await {
        Ok(outcome) => outcome,
        Err(error) => (1, Some(error.to_string())),
    };

    runs::update_run_status(
        db,
        run.id,
        UpdateRunStatusInput {
            status: if failure.is_some() {
                "failed"
            } else {
                "completed"
            }
            .into(),
            completed_at: Some(Some(Utc::now())),
            error_message: Some(failure),
            ..Default::default()
        },
    )
    .await?;

    Ok(exit_code)
}

fn plan(
    backend: &model_backends::Model,
    target: &execution_targets::Model,
    profile: &model_invocation_profiles::Model,
    run: &run_entity::Model,
) -> Result<(CommandSpec, PathBuf), DbErr> {
    let command = plan_openfold_command(backend, target, profile, run)?;
    let output_dir = resolve_output_location(profile, run)?;
    println!("run {} -> {}", run.id, output_dir.display());
    println!("$ {} {}\n", command.program, command.args.join(" "));

    Ok((command, output_dir))
}

/// Returns the process exit code and, when the run did not succeed, why.
async fn run_openfold(
    backend: &model_backends::Model,
    target: &execution_targets::Model,
    profile: &model_invocation_profiles::Model,
    run: &run_entity::Model,
) -> Result<(i32, Option<String>), DbErr> {
    let (command, output_dir) = plan(backend, target, profile, run)?;
    let ExecutionWorkflowResult {
        preflight_report,
        command_output,
        skipped_execution_reason,
    } = execute_command_workflow(
        &command,
        &LocalCommandRunner,
        Some(&OpenFoldPreflightRunner {
            command: &command,
            invocation_profile: profile,
            run,
        }),
    )
    .await?;

    if let Some(report) = &preflight_report {
        print_report(report);
    }

    let Some(output) = command_output else {
        let reason = skipped_execution_reason.unwrap_or_else(|| "execution skipped".into());
        println!("execution skipped: {reason}");
        return Ok((1, Some(reason)));
    };

    print!("{}", output.stdout);
    eprint!("{}", output.stderr);
    println!("\nexit {} -> {}", output.exit_code, output_dir.display());

    Ok((
        output.exit_code,
        (output.exit_code != 0)
            .then(|| format!("OpenFold exited with status {}", output.exit_code)),
    ))
}

fn execution_parameters(
    options: &BTreeMap<String, String>,
    fasta_dir: &str,
) -> Result<Map<String, Value>, DbErr> {
    let mut parameters = Map::new();
    parameters.insert("fasta_dir".into(), json!(fasta_dir));
    parameters.insert("data_dir".into(), json!(required(options, "data-dir")?));

    if let Some(model_device) = option(options, "model-device") {
        parameters.insert("model_device".into(), json!(model_device));
    }

    // Supplying the directory is the only reason to reuse alignments.
    if let Some(alignment_dir) = option(options, "alignment-dir") {
        parameters.insert("alignment_dir".into(), json!(alignment_dir));
        parameters.insert("use_precomputed_alignments".into(), json!(true));
    }

    for (name, key) in [("cpus", "cpus"), ("residue-idx", "residue_idx")] {
        if let Some(value) = integer(options, name)? {
            parameters.insert(key.into(), json!(value));
        }
    }

    Ok(parameters)
}

fn model_parameters(options: &BTreeMap<String, String>) -> Map<String, Value> {
    let mut parameters = Map::new();

    if let Some(config_preset) = option(options, "config-preset") {
        parameters.insert("config_preset".into(), json!(config_preset));
    }

    for (name, key) in [("save-outputs", "save_outputs"), ("demo-attn", "demo_attn")] {
        if flag(options, name) {
            parameters.insert(key.into(), json!(true));
        }
    }

    parameters
}

/// Preflight already requires exactly one FASTA matching `input_id`.
fn read_fasta_sequence(fasta_dir: &str) -> Result<String, DbErr> {
    let path = fasta_files_in_directory(Path::new(fasta_dir))
        .map_err(|error| DbErr::Custom(format!("cannot read fasta_dir '{fasta_dir}': {error}")))?
        .into_iter()
        .next()
        .ok_or_else(|| DbErr::Custom(format!("no FASTA file in '{fasta_dir}'")))?;
    let contents = std::fs::read_to_string(&path)
        .map_err(|error| DbErr::Custom(format!("cannot read '{}': {error}", path.display())))?;

    Ok(contents.lines().skip(1).map(str::trim).collect())
}

fn print_report(report: &PreflightReport) {
    for check in &report.checks {
        let label = match check.status {
            PreflightStatus::Passed => "pass",
            PreflightStatus::Warning => "warn",
            PreflightStatus::Failed => "FAIL",
        };
        println!(
            "[{label}] {}: {}",
            check.name,
            check.message.as_deref().unwrap_or("")
        );
    }
    println!();
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{parse_args, read_fasta_sequence};

    fn parse(argv: &[&str]) -> Result<BTreeMap<String, String>, String> {
        parse_args(argv.iter().map(|value| (*value).to_owned()))
    }

    #[test]
    fn parses_valued_options_and_value_less_flags() {
        let options = parse(&["--input-id", "6KWC_1", "--demo-attn", "--cpus", "8"])
            .expect("arguments should parse");

        assert_eq!(options.get("input-id").map(String::as_str), Some("6KWC_1"));
        assert_eq!(options.get("cpus").map(String::as_str), Some("8"));
        assert_eq!(options.get("demo-attn").map(String::as_str), Some("true"));
    }

    #[test]
    fn rejects_malformed_arguments() {
        for (argv, expected) in [
            (
                &["--demo-attn", "--input-id"][..],
                "--input-id requires a value",
            ),
            (&["run"][..], "unexpected argument 'run'"),
            (
                &["--model-devices", "cpu"][..],
                "unknown option '--model-devices'",
            ),
        ] {
            let error = parse(argv).expect_err("malformed arguments should fail");
            assert!(error.contains(expected), "got '{error}', want '{expected}'");
        }
    }

    #[test]
    fn reads_the_sequence_without_the_fasta_header() {
        let dir = std::env::temp_dir().join("executor-cli-fasta-test");
        std::fs::create_dir_all(&dir).expect("temp dir should be created");
        std::fs::write(
            dir.join("input.fasta"),
            ">6KWC_1 some description\nMSTN\nPKPQ\n",
        )
        .expect("fasta should be written");

        let sequence = read_fasta_sequence(dir.to_str().expect("utf-8 path"))
            .expect("sequence should be read");

        assert_eq!(sequence, "MSTNPKPQ");
        std::fs::remove_dir_all(&dir).ok();
    }
}
