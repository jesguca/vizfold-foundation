use clap::{Args, Parser, Subcommand};
use sea_orm::DbErr;

use crate::core::{
    db,
    repositories::{execution_targets, model_backends, model_invocation_profiles},
    seed::seed_defaults,
    services::runs,
};

#[derive(Debug, Parser)]
#[command(name = "vizfold", about = "VizFold executor administration CLI")]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Seed the default executor records.
    Seed,
    /// List executor records.
    List(ListArgs),
    /// Show one executor record.
    Show(ShowArgs),
}

#[derive(Debug, Args)]
struct ListArgs {
    #[command(subcommand)]
    resource: ListResource,
}

#[derive(Debug, Subcommand)]
enum ListResource {
    /// List model backends.
    Models,
    /// List execution targets.
    Targets,
    /// List model invocation profiles.
    Profiles,
    /// List runs.
    Runs {
        /// Restrict results to runs with this status.
        #[arg(long)]
        status: Option<String>,
    },
}

#[derive(Debug, Args)]
struct ShowArgs {
    #[command(subcommand)]
    resource: ShowResource,
}

#[derive(Debug, Subcommand)]
enum ShowResource {
    /// Show a run and its artifacts.
    Run { run_id: i32 },
}

pub async fn run() -> Result<(), DbErr> {
    let cli = Cli::parse();
    let database = db::connect_and_migrate().await?;

    match cli.command {
        Command::Seed => {
            seed_defaults(&database).await?;
            println!("Seeded default executor records.");
        }
        Command::List(list) => match list.resource {
            ListResource::Models => list_models(&database).await?,
            ListResource::Targets => list_targets(&database).await?,
            ListResource::Profiles => list_profiles(&database).await?,
            ListResource::Runs { status } => list_runs(&database, status.as_deref()).await?,
        },
        Command::Show(show) => match show.resource {
            ShowResource::Run { run_id } => show_run(&database, run_id).await?,
        },
    }

    Ok(())
}

async fn list_models(database: &sea_orm::DatabaseConnection) -> Result<(), DbErr> {
    let models = model_backends::list(database).await?;
    print_table(
        &["ID", "SLUG", "LABEL", "VERSION"],
        models
            .iter()
            .map(|model| {
                vec![
                    model.id.to_string(),
                    model.slug.clone(),
                    model.label.clone(),
                    model.version.clone().unwrap_or_else(|| "-".into()),
                ]
            })
            .collect(),
    );
    Ok(())
}

async fn list_targets(database: &sea_orm::DatabaseConnection) -> Result<(), DbErr> {
    let targets = execution_targets::list(database).await?;
    print_table(
        &["ID", "SLUG", "TYPE", "DESCRIPTION"],
        targets
            .iter()
            .map(|target| {
                vec![
                    target.id.to_string(),
                    target.slug.clone(),
                    target.target_type.clone(),
                    target.description.clone().unwrap_or_else(|| "-".into()),
                ]
            })
            .collect(),
    );
    Ok(())
}

async fn list_profiles(database: &sea_orm::DatabaseConnection) -> Result<(), DbErr> {
    let profiles = model_invocation_profiles::list(database).await?;
    print_table(
        &["ID", "MODEL ID", "TARGET ID", "INVOCATION KIND"],
        profiles
            .iter()
            .map(|profile| {
                vec![
                    profile.id.to_string(),
                    profile.model_backend_id.to_string(),
                    profile.execution_target_id.to_string(),
                    profile.invocation_kind.clone(),
                ]
            })
            .collect(),
    );
    Ok(())
}

async fn list_runs(
    database: &sea_orm::DatabaseConnection,
    status: Option<&str>,
) -> Result<(), DbErr> {
    let runs = runs::list_runs(database).await?;
    print_table(
        &[
            "ID",
            "STATUS",
            "MODEL ID",
            "TARGET ID",
            "PROFILE ID",
            "INPUT ID",
            "SUBMITTED AT",
        ],
        runs.iter()
            .filter(|run| status.is_none_or(|value| run.status == value))
            .map(|run| {
                vec![
                    run.id.to_string(),
                    run.status.clone(),
                    run.model_backend_id.to_string(),
                    run.execution_target_id.to_string(),
                    run.invocation_profile_id.to_string(),
                    run.input_id.clone(),
                    run.submitted_at.to_rfc3339(),
                ]
            })
            .collect(),
    );
    Ok(())
}

async fn show_run(database: &sea_orm::DatabaseConnection, run_id: i32) -> Result<(), DbErr> {
    let Some(result) = runs::get_run_with_artifacts(database, run_id).await? else {
        return Err(DbErr::Custom(format!("run {run_id} does not exist")));
    };
    let run = result.run;

    println!("Run {}", run.id);
    println!("status: {}", run.status);
    println!("input_id: {}", run.input_id);
    println!("model_backend_id: {}", run.model_backend_id);
    println!("execution_target_id: {}", run.execution_target_id);
    println!("invocation_profile_id: {}", run.invocation_profile_id);
    println!("submitted_at: {}", run.submitted_at.to_rfc3339());
    println!("started_at: {}", format_time(run.started_at));
    println!("completed_at: {}", format_time(run.completed_at));
    if let Some(error_message) = run.error_message {
        println!("error_message: {error_message}");
    }

    println!("artifacts:");
    print_table(
        &["ID", "TYPE ID", "FORMAT", "STORAGE URI"],
        result
            .artifacts
            .iter()
            .map(|artifact| {
                vec![
                    artifact.id.to_string(),
                    artifact.artifact_type_id.to_string(),
                    artifact.format.clone(),
                    artifact.storage_uri.clone(),
                ]
            })
            .collect(),
    );
    Ok(())
}

fn format_time(value: Option<chrono::DateTime<chrono::Utc>>) -> String {
    value
        .map(|time| time.to_rfc3339())
        .unwrap_or_else(|| "-".into())
}

fn print_table(headers: &[&str], rows: Vec<Vec<String>>) {
    let mut widths: Vec<usize> = headers.iter().map(|header| header.len()).collect();
    for row in &rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(cell.len());
        }
    }

    print_row(headers.iter().copied(), &widths);
    let separator = widths
        .iter()
        .map(|width| "-".repeat(*width))
        .collect::<Vec<_>>();
    print_row(separator.iter().map(String::as_str), &widths);
    for row in rows {
        print_row(row.iter().map(String::as_str), &widths);
    }
}

fn print_row<'a>(cells: impl IntoIterator<Item = &'a str>, widths: &[usize]) {
    let rendered = cells
        .into_iter()
        .zip(widths)
        .map(|(cell, width)| format!("{cell:<width$}", width = width))
        .collect::<Vec<_>>()
        .join("  ");
    println!("{rendered}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_list_runs_with_status_filter() {
        let cli = Cli::try_parse_from(["vizfold", "list", "runs", "--status", "failed"])
            .expect("list runs command should parse");

        assert!(matches!(
            cli.command,
            Command::List(ListArgs {
                resource: ListResource::Runs { status: Some(status) }
            }) if status == "failed"
        ));
    }

    #[test]
    fn parses_show_run() {
        let cli = Cli::try_parse_from(["vizfold", "show", "run", "1"])
            .expect("show run command should parse");

        assert!(matches!(
            cli.command,
            Command::Show(ShowArgs {
                resource: ShowResource::Run { run_id: 1 }
            })
        ));
    }
}
