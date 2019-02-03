use os_pipe::pipe;
use std::io;
use std::io::prelude::*;
use std::process::ExitStatus;
use std::process::{Command, Output, Stdio};

type CommandResult = io::Result<CommandOutput>;

#[derive(Debug)]
pub struct CommandOutput {
    pub output: String,
    pub status: ExitStatus,
}

pub fn run_command_windows(cmd: &str, args: &Vec<String>) -> CommandResult {
    let stdout = Stdio::piped();
    let stderr = Stdio::piped();

    let h = Command::new(cmd)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .args(args)
        .spawn();
    h?.wait_with_output().map(|output: Output| CommandOutput {
        output: format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
        status: output.status,
    })
}

pub fn run_command(cmd: &str, args: &Vec<String>) -> io::Result<CommandOutput> {
    if cfg!(target_os = "linux") {
        run_command_unix(cmd, args)
    } else {
        // windows support is currently untested and probably not good
        run_command_windows(cmd, args)
    }
}

pub fn run_command_unix(cmd: &str, args: &Vec<String>) -> io::Result<CommandOutput> {
    let (mut reader, writer) = pipe().unwrap();
    let writer_clone = writer.try_clone().unwrap();

    let mut cmd = Command::new(cmd);
    let mut h = cmd
        .stdin(Stdio::null())
        .stdout(writer)
        .stderr(writer_clone)
        .args(args)
        .spawn()
        .unwrap();
    drop(cmd);
    let mut output = String::new();
    reader.read_to_string(&mut output).unwrap();
    let rc = h.wait()?;
    Ok(CommandOutput {
        output: output,
        status: rc,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn should_success() {
        let mut args = Vec::new();
        args.push("1122331".to_string());
        let output = run_command_unix("tests/run_command.sh", &args);
        assert!(output.unwrap().status.success());
    }

    #[test]
    fn should_combine_stderr() {
        let mut args = Vec::new();
        args.push("1122331".to_string());
        let output = run_command_unix("tests/run_command.sh", &args);
        assert!(output.unwrap().output.contains("should capture stderr as well"));
    }

    #[test]
    fn should_fail_err_1() {
        let mut args = Vec::new();
        args.push("some-id-error".to_string());
        let output = run_command_unix("tests/run_command.sh", &args);
        let status = output.unwrap().status;
        assert!(!status.success());
        assert_eq!(status.code(), Some(1));
    }
}
