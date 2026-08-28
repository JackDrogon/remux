use std::process::Command;

use clap::error::ErrorKind;
use remux::cli::{CliCommand, parse_cli_args};

#[test]
fn socket_can_appear_before_or_after_action() {
    let before = parse_cli_args(["remux", "-L", "sockA", "backup", "backup_20240101_120000"])
        .expect("socket before action should parse");
    let after = parse_cli_args(["remux", "backup", "backup_20240101_120000", "-L", "sockA"])
        .expect("socket after action should parse");

    assert_eq!(before.socket_name.as_deref(), Some("sockA"));
    assert_eq!(before.verbose_log_level, 0);
    assert_eq!(
        before.command,
        CliCommand::Backup {
            name: Some("backup_20240101_120000".to_string())
        }
    );
    assert_eq!(before, after);
}

#[test]
fn subcommands_map_cleanly_to_internal_actions() {
    let parsed = parse_cli_args(["remux", "-L", "sockA", "backup", "named_backup"])
        .expect("backup command should parse");

    assert_eq!(parsed.socket_name.as_deref(), Some("sockA"));
    assert_eq!(parsed.verbose_log_level, 0);
    assert_eq!(
        parsed.command,
        CliCommand::Backup {
            name: Some("named_backup".to_string())
        }
    );

    let interactive_restore = parse_cli_args(["remux", "restore", "--interactive"])
        .expect("interactive restore should parse");
    assert_eq!(interactive_restore.command, CliCommand::InteractiveRestore);

    let named_restore = parse_cli_args(["remux", "restore", "backup_20240101_120000"])
        .expect("named restore should parse");
    assert_eq!(
        named_restore.command,
        CliCommand::Restore {
            name: Some("backup_20240101_120000".to_string())
        }
    );

    let verbose = parse_cli_args(["remux", "-v", "backup", "named_backup"])
        .expect("tmux verbose should parse globally");
    assert_eq!(verbose.verbose_log_level, 1);
    assert_eq!(
        verbose.command,
        CliCommand::Backup {
            name: Some("named_backup".to_string())
        }
    );

    let verbose_twice = parse_cli_args(["remux", "-vv", "backup", "named_backup"])
        .expect("multiple tmux verbose flags should parse globally");
    assert_eq!(verbose_twice.verbose_log_level, 2);
    assert_eq!(
        verbose_twice.command,
        CliCommand::Backup {
            name: Some("named_backup".to_string())
        }
    );

    let compact =
        parse_cli_args(["remux", "-L", "sockA", "compact"]).expect("compact command should parse");
    assert_eq!(compact.command, CliCommand::Compact);
    assert_eq!(compact.socket_name.as_deref(), Some("sockA"));

    let backup_compact = parse_cli_args(["remux", "backup", "--compact"]).unwrap_err();
    assert_eq!(backup_compact.kind(), ErrorKind::UnknownArgument);
    assert!(
        backup_compact
            .to_string()
            .contains("unexpected argument '--compact' found"),
        "backup must not grow a --compact flag, got: {backup_compact}"
    );
}

#[test]
fn clap_errors_cover_missing_values_and_unexpected_arguments() {
    let missing_socket = parse_cli_args(["remux", "-L"]).unwrap_err();
    assert_eq!(missing_socket.kind(), ErrorKind::InvalidValue);
    assert!(
        missing_socket
            .to_string()
            .contains("a value is required for '-L <socket-name>'"),
        "expected clap missing-value message, got: {missing_socket}"
    );

    let unknown_action = parse_cli_args(["remux", "--wat"]).unwrap_err();
    assert_eq!(unknown_action.kind(), ErrorKind::UnknownArgument);
    assert!(
        unknown_action
            .to_string()
            .contains("unexpected argument '--wat' found"),
        "expected clap unknown-argument message, got: {unknown_action}"
    );

    let legacy_socket =
        parse_cli_args(["remux", "--socket", "sockA", "backup", "demo"]).unwrap_err();
    assert_eq!(legacy_socket.kind(), ErrorKind::UnknownArgument);
    assert!(
        legacy_socket
            .to_string()
            .contains("unexpected argument '--socket' found"),
        "expected legacy --socket rejection, got: {legacy_socket}"
    );

    let extra_arg = parse_cli_args(["remux", "backup", "backup", "extra"]).unwrap_err();
    assert_eq!(extra_arg.kind(), ErrorKind::UnknownArgument);
    assert!(
        extra_arg
            .to_string()
            .contains("unexpected argument 'extra' found"),
        "expected clap extra-argument message, got: {extra_arg}"
    );
}

#[test]
fn no_action_is_reported_as_missing_required_input() {
    let error = parse_cli_args(["remux"]).unwrap_err();
    assert_eq!(
        error.kind(),
        ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand,
    );
    assert!(
        error
            .to_string()
            .contains("Usage: remux [OPTIONS] <COMMAND>"),
        "expected clap help-on-missing-command output, got: {error}"
    );
}

#[test]
fn invalid_argument_shapes_exit_nonzero() {
    let binary = env!("CARGO_BIN_EXE_remux");
    let cases: Vec<Vec<&str>> = vec![
        vec!["-L"],
        vec!["--socket", "sockA", "backup", "demo"],
        vec!["backup", "backup", "extra"],
        vec!["--wat"],
        vec!["restore", "named_backup", "--interactive"],
    ];

    for args in cases {
        let output = Command::new(binary)
            .args(&args)
            .output()
            .expect("binary invocation should succeed");

        assert!(
            !output.status.success(),
            "expected nonzero exit for args {:?}",
            args
        );
        assert!(
            !String::from_utf8_lossy(&output.stderr).trim().is_empty(),
            "expected stderr output for args {:?}",
            args
        );
    }
}

#[test]
fn clap_stderr_is_clean_and_brief() {
    let binary = env!("CARGO_BIN_EXE_remux");
    let output = Command::new(binary)
        .args(["--wat"])
        .output()
        .expect("binary invocation should succeed");

    assert!(!output.status.success(), "unknown action should fail");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("error: unexpected argument '--wat' found"),
        "expected clap parse error text, stderr was: {stderr}"
    );
    assert!(
        stderr.contains("Usage: remux [OPTIONS] <COMMAND>"),
        "expected clap usage text for parse errors, stderr was: {stderr}"
    );
    assert_stable_stderr(
        &stderr,
        "parse errors should not render diagnostic report frames",
    );
}

fn assert_stable_stderr(stderr: &str, context: &str) {
    assert!(
        !stderr.contains("Location:") && !stderr.contains("Backtrace omitted."),
        "{context}, stderr was: {stderr}"
    );
}
