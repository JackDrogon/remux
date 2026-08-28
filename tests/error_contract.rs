use std::error::Error as StdError;
use std::io;
use std::path::PathBuf;

use remux::{Backup, Catalog, Category, Cli, Config, Error, Interactive, Restore, Snapshot, Tmux};
use xerror::Context;

#[test]
fn inner_catalogs_map_to_matching_categories() {
    let cases = [
        (
            Error::from(Cli::Stdout(io::Error::other("broken pipe"))),
            Category::Cli,
        ),
        (Error::from(Config::HomeDirNotFound), Category::Config),
        (Error::from(Catalog::BackupNameEmpty), Category::Catalog),
        (
            Error::from(Snapshot::InvalidRelativePath {
                relative_path: "pane".to_string(),
            }),
            Category::Snapshot,
        ),
        (
            Error::from(Backup::InvalidTmuxOutput {
                command: "list-sessions",
                line: "line".to_string(),
                detail: "detail".to_string(),
            }),
            Category::Backup,
        ),
        (
            Error::from(Restore::MissingPaneAsset {
                pane_id: "pane".to_string(),
            }),
            Category::Restore,
        ),
        (
            Error::from(Tmux::TmuxFailed {
                command: vec!["tmux".to_string()],
                status: Some(1),
                stdout: String::new(),
                stderr: String::new(),
            }),
            Category::Tmux,
        ),
        (
            Error::from(Interactive::EndOfInput { context: "input" }),
            Category::Interactive,
        ),
    ];

    for (error, category) in cases {
        assert_eq!(error.category(), category);
    }
}

#[test]
fn config_source_report_keeps_context_and_terminal_shell_semantics() {
    let result: remux::Result<()> = Err(Config::ReadFile {
        path: PathBuf::from("config.toml"),
        source: io::Error::other("disk exploded"),
    }
    .into());

    let outer = result
        .context("reading config")
        .context("loading remux")
        .unwrap_err();

    assert_eq!(
        outer.contexts().collect::<Vec<_>>(),
        ["reading config", "loading remux"]
    );
    assert_eq!(
        outer.to_string(),
        "loading remux\nreading config\nfailed to read config.toml\ncaused by: disk exploded"
    );
    assert!(StdError::source(&outer).is_none());
    assert_eq!(
        StdError::source(outer.code())
            .expect("config io source")
            .to_string(),
        "disk exploded"
    );
}

#[test]
fn interactive_io_exposes_os_error_as_source() {
    let outer = Error::from(Interactive::InteractiveIo(io::Error::other("pipe closed")));
    assert_eq!(
        outer.to_string(),
        "interactive I/O failed\ncaused by: pipe closed"
    );
    assert!(StdError::source(&outer).is_none());
    assert_eq!(
        StdError::source(outer.code())
            .expect("interactive io source")
            .to_string(),
        "pipe closed"
    );
}
