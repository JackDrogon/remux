use std::process::Command;

use remux::cli::{Action, CliError, parse_cli_args};

#[test]
fn socket_can_appear_before_or_after_action() {
    let before = parse_cli_args(["remux", "-L", "sockA", "-b", "backup_20240101_120000"])
        .expect("socket before action should parse");
    let after = parse_cli_args(["remux", "-b", "backup_20240101_120000", "-L", "sockA"])
        .expect("socket after action should parse");

    assert_eq!(before.socket_name.as_deref(), Some("sockA"));
    assert_eq!(before.action, Action::Backup);
    assert_eq!(before.action_arg.as_deref(), Some("backup_20240101_120000"));
    assert_eq!(before, after);
}

#[test]
fn missing_socket_value_is_rejected() {
    let error = parse_cli_args(["remux", "-L"]).unwrap_err();

    assert_eq!(error, CliError::MissingSocketName);
}

#[test]
fn too_many_args_or_invalid_shape_are_rejected() {
    assert_eq!(
        parse_cli_args(["remux", "-b", "backup", "extra"]).unwrap_err(),
        CliError::TooManyArguments
    );
    assert_eq!(
        parse_cli_args(["remux", "--wat"]).unwrap_err(),
        CliError::UnknownAction("--wat".to_string())
    );
    assert_eq!(
        parse_cli_args(["remux"]).unwrap_err(),
        CliError::MissingAction
    );
}

#[test]
fn invalid_argument_shapes_exit_nonzero() {
    let binary = env!("CARGO_BIN_EXE_remux");
    let cases: Vec<Vec<&str>> = vec![vec!["-L"], vec!["-b", "backup", "extra"], vec!["--wat"]];

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
