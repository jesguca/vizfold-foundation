use std::{collections::BTreeMap, path::PathBuf};

use sea_orm::DbErr;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub current_dir: Option<PathBuf>,
    pub env: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[async_trait::async_trait]
pub trait CommandRunner {
    async fn run(&self, spec: CommandSpec) -> Result<CommandOutput, DbErr>;
}

#[cfg(test)]
#[derive(Clone, Debug)]
pub struct FakeCommandRunner {
    output: Result<CommandOutput, String>,
}

#[cfg(test)]
impl FakeCommandRunner {
    pub fn succeeds(output: CommandOutput) -> Self {
        Self { output: Ok(output) }
    }

    pub fn fails(message: impl Into<String>) -> Self {
        Self {
            output: Err(message.into()),
        }
    }
}

#[cfg(test)]
#[async_trait::async_trait]
impl CommandRunner for FakeCommandRunner {
    async fn run(&self, _spec: CommandSpec) -> Result<CommandOutput, DbErr> {
        self.output
            .clone()
            .map_err(|message| DbErr::Custom(message))
    }
}

#[cfg(test)]
mod tests {
    use super::{CommandOutput, CommandRunner, CommandSpec, FakeCommandRunner};

    #[test]
    fn command_spec_captures_program_args_dir_and_env() {
        let mut spec = CommandSpec {
            program: "openfold".into(),
            args: vec!["--fasta".into(), "input.fasta".into()],
            current_dir: Some("runs/run-1".into()),
            ..Default::default()
        };
        spec.env.insert("CUDA_VISIBLE_DEVICES".into(), "0".into());

        assert_eq!(spec.program, "openfold");
        assert_eq!(spec.args, vec!["--fasta", "input.fasta"]);
        assert_eq!(spec.current_dir, Some("runs/run-1".into()));
        assert_eq!(spec.env["CUDA_VISIBLE_DEVICES"], "0");
    }

    #[tokio::test]
    async fn fake_command_runner_returns_configured_success() {
        let runner = FakeCommandRunner::succeeds(CommandOutput {
            exit_code: 0,
            stdout: "done".into(),
            stderr: String::new(),
        });

        let output = runner
            .run(CommandSpec {
                program: "ignored".into(),
                ..Default::default()
            })
            .await
            .expect("fake runner should succeed");

        assert_eq!(output.exit_code, 0);
        assert_eq!(output.stdout, "done");
        assert_eq!(output.stderr, "");
    }

    #[tokio::test]
    async fn fake_command_runner_returns_configured_failure() {
        let runner = FakeCommandRunner::fails("command failed");

        let error = runner
            .run(CommandSpec {
                program: "ignored".into(),
                ..Default::default()
            })
            .await
            .expect_err("fake runner should fail");

        assert!(error.to_string().contains("command failed"));
    }
}
