use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use remux::config::ConfigPaths;

#[test]
fn help_shows_branding_commands_and_footer() {
    let temp_home = TempHome::new("help-output");
    let output = run_binary(temp_home.path(), ["--help"]);
    assert_success(&output, "help output should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in [
        "____  ________  ______  __",
        "remux 0.1.0",
        "Usage: remux [OPTIONS] <COMMAND>",
        "Commands:",
        "backup",
        "Capture current tmux sessions",
        "list",
        "Inspect backups",
        "delete",
        "Delete backups",
        "restore",
        "Restore tmux sessions from backup",
        "Options:",
        "-v, --tmux-verbose",
        "Increase tmux command verbosity",
        "-L <socket-name>",
        "-V, --version",
        "Examples:",
        "config file: $HOME/.remux/config.toml",
    ] {
        assert!(
            stdout.contains(expected),
            "expected help output to contain {expected:?}, got:\n{stdout}"
        );
    }
}

#[test]
fn no_args_now_show_help_like_a_clap_cli() {
    let temp_home = TempHome::new("help-no-args");
    let output = run_binary(temp_home.path(), []);
    assert_success(&output, "no-arg invocation should now show help");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Usage: remux [OPTIONS] <COMMAND>"),
        "expected help output when no args are supplied, got:\n{stdout}"
    );

    let config_file = ConfigPaths::from_home(temp_home.path()).config_file;
    assert!(
        !config_file.exists(),
        "help-on-missing-command should not need to bootstrap config at {}",
        config_file.display()
    );
}

#[test]
fn version_output_is_stable_and_does_not_require_config_bootstrap() {
    let temp_home = TempHome::new("version-output");
    let output = run_binary(temp_home.path(), ["--version"]);
    assert_success(&output, "version output should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout, format!("remux {}\n", env!("CARGO_PKG_VERSION")));

    let config_file = ConfigPaths::from_home(temp_home.path()).config_file;
    assert!(
        !config_file.exists(),
        "version should not need to bootstrap config at {}",
        config_file.display()
    );
}

#[test]
fn clap_parse_errors_do_not_require_config_bootstrap() {
    let temp_home = TempHome::new("parse-error-output");
    let output = run_binary(temp_home.path(), ["--wat"]);
    assert!(
        !output.status.success(),
        "unknown argument should fail: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unexpected argument '--wat' found"),
        "expected clap parse error, got:\n{stderr}"
    );

    let config_file = ConfigPaths::from_home(temp_home.path()).config_file;
    assert!(
        !config_file.exists(),
        "parse errors should not need to bootstrap config at {}",
        config_file.display()
    );
}

fn run_binary<const N: usize>(home_dir: &Path, args: [&str; N]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_remux"))
        .env("HOME", home_dir)
        .args(args)
        .output()
        .expect("remux binary invocation should succeed")
}

fn assert_success(output: &std::process::Output, context: &str) {
    assert!(
        output.status.success(),
        "{context}: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[derive(Debug)]
struct TempHome {
    path: PathBuf,
}

impl TempHome {
    fn new(label: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after UNIX_EPOCH")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "remux-help-output-{label}-{}-{unique}",
            std::process::id()
        ));

        if path.exists() {
            fs::remove_dir_all(&path).expect("should clear stale temp HOME");
        }
        fs::create_dir_all(&path).expect("should create temp HOME");

        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempHome {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
