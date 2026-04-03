use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use retmux::config::ConfigPaths;

#[test]
fn help_lists_option_inventory_and_config_path() {
    let temp_home = TempHome::new("help-output");
    let output = run_binary(temp_home.path(), ["-h"]);
    assert_success(&output, "help output should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in [
        "-h                  print help message",
        "-v                  version",
        "-l [name]           list backup info",
        "with [name]:    show detailed backup info by name",
        "without [name]: show brief and detailed info interactively",
        "-d [name]           delete a backup",
        "-b [name]           backup current tmux sessions",
        "-r [name]           restore tmux sessions from backup",
        "-ri                 restore sessions interactively",
        "-L [socket-name]    use the given tmux socket name",
        "config file: $HOME/.retmux/retmux.conf",
    ] {
        assert!(
            stdout.contains(expected),
            "expected help output to contain {expected:?}, got:\n{stdout}"
        );
    }
}

#[test]
fn version_output_is_stable_and_bootstraps_config() {
    let temp_home = TempHome::new("version-output");
    let output = run_binary(temp_home.path(), ["-v"]);
    assert_success(&output, "version output should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout, format!("retmux {}\n", env!("CARGO_PKG_VERSION")));

    let config_file = ConfigPaths::from_home(temp_home.path()).config_file;
    assert!(
        config_file.is_file(),
        "version should still bootstrap config at {}",
        config_file.display()
    );
}

fn run_binary<const N: usize>(home_dir: &Path, args: [&str; N]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_retmux"))
        .env("HOME", home_dir)
        .args(args)
        .output()
        .expect("retmux binary invocation should succeed")
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
            "retmux-help-output-{label}-{}-{unique}",
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
