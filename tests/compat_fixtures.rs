use std::fs;
use std::path::{Path, PathBuf};

use retmux::serde_legacy;

const FIXTURES_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/legacy");
const REQUIRED_JSON_MARKERS: [&str; 2] = ["\"__class__\"", "\"__module__\""];

#[derive(Debug)]
struct FixtureSpec {
    root_name: &'static str,
    backup_id: &'static str,
    pane_files: &'static [&'static str],
}

impl FixtureSpec {
    fn root_path(&self, fixtures_root: &Path) -> PathBuf {
        fixtures_root.join(self.root_name)
    }

    fn backup_dir(&self, fixtures_root: &Path) -> PathBuf {
        self.root_path(fixtures_root).join(self.backup_id)
    }

    fn json_path(&self, fixtures_root: &Path) -> PathBuf {
        self.backup_dir(fixtures_root)
            .join(format!("{}.json", self.backup_id))
    }
}

fn fixture_specs() -> [FixtureSpec; 2] {
    [
        FixtureSpec {
            root_name: "default_socket",
            backup_id: "backup_20240101_120000",
            pane_files: &["work:1.0", "work:1.1"],
        },
        FixtureSpec {
            root_name: "named_socket_sockA",
            backup_id: "backup_20240102_120000",
            pane_files: &["ops:2.0"],
        },
    ]
}

fn validate_fixture_tree(fixtures_root: &Path, spec: &FixtureSpec) -> Result<(), String> {
    let root_path = spec.root_path(fixtures_root);
    if !root_path.is_dir() {
        return Err(format!("fixture root is missing: {}", root_path.display()));
    }

    let backup_dir = spec.backup_dir(fixtures_root);
    if !backup_dir.is_dir() {
        return Err(format!(
            "backup directory is missing: {}",
            backup_dir.display()
        ));
    }

    let json_path = spec.json_path(fixtures_root);
    if !json_path.is_file() {
        return Err(format!("backup json is missing: {}", json_path.display()));
    }

    let json_content = fs::read_to_string(&json_path)
        .map_err(|error| format!("failed to read {}: {error}", json_path.display()))?;

    for marker in REQUIRED_JSON_MARKERS {
        if !json_content.contains(marker) {
            return Err(format!(
                "backup json {} is missing required marker {marker}",
                json_path.display()
            ));
        }
    }

    for pane_file in spec.pane_files {
        let pane_path = backup_dir.join(pane_file);
        if !pane_path.is_file() {
            return Err(format!(
                "pane content file is missing: {}",
                pane_path.display()
            ));
        }

        let pane_content = fs::read_to_string(&pane_path)
            .map_err(|error| format!("failed to read {}: {error}", pane_path.display()))?;
        if pane_content.trim().is_empty() {
            return Err(format!(
                "pane content file is empty: {}",
                pane_path.display()
            ));
        }
    }

    let mut entries = fs::read_dir(&backup_dir)
        .map_err(|error| {
            format!(
                "failed to read fixture directory {}: {error}",
                backup_dir.display()
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            format!(
                "failed to enumerate fixture directory {}: {error}",
                backup_dir.display()
            )
        })?;
    entries.sort_by_key(|entry| entry.file_name());

    let actual_names = entries
        .iter()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    let mut expected_names = spec
        .pane_files
        .iter()
        .map(|name| (*name).to_string())
        .collect::<Vec<_>>();
    expected_names.push(format!("{}.json", spec.backup_id));
    expected_names.sort();

    if actual_names != expected_names {
        return Err(format!(
            "fixture directory {} has unexpected shape: expected {:?}, got {:?}",
            backup_dir.display(),
            expected_names,
            actual_names
        ));
    }

    Ok(())
}

#[test]
fn legacy_fixture_assets_are_complete_and_well_formed() {
    let fixtures_root = Path::new(FIXTURES_ROOT);
    for spec in fixture_specs() {
        validate_fixture_tree(fixtures_root, &spec).unwrap_or_else(|message| {
            panic!("fixture '{}' is invalid: {message}", spec.root_name);
        });
    }
}

#[test]
fn rust_decodes_python_snapshot_fixture() {
    let fixtures_root = Path::new(FIXTURES_ROOT);

    for spec in fixture_specs() {
        let snapshot = serde_legacy::read_snapshot_file(spec.json_path(fixtures_root))
            .unwrap_or_else(|error| {
                panic!("fixture '{}' failed to decode: {error}", spec.root_name)
            });

        assert_eq!(snapshot.tid, spec.backup_id);
        assert_eq!(snapshot.sessions.len(), 1);

        let pane_files = snapshot
            .sessions
            .iter()
            .flat_map(|session| session.windows.iter())
            .flat_map(|window| window.panes.iter().map(|pane| pane.idstr()))
            .collect::<Vec<_>>();

        assert_eq!(pane_files, spec.pane_files);
    }
}

#[test]
fn corrupt_fixture_shape_is_rejected() {
    let temp_root = std::env::temp_dir().join(format!(
        "retmux-corrupt-fixture-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("compat")
    ));
    if temp_root.exists() {
        fs::remove_dir_all(&temp_root).expect("failed to clear stale temp fixture root");
    }

    let spec = FixtureSpec {
        root_name: "broken_fixture",
        backup_id: "backup_20990101_000000",
        pane_files: &["broken:1.0"],
    };
    let broken_root = temp_root.join(spec.root_name).join(spec.backup_id);
    fs::create_dir_all(&broken_root).expect("failed to create broken fixture tree");
    fs::write(
        broken_root.join(format!("{}.json", spec.backup_id)),
        "{\n  \"__class__\": \"Tmux\"\n}\n",
    )
    .expect("failed to write broken fixture json");

    let validation_error = validate_fixture_tree(
        &temp_root,
        &FixtureSpec {
            root_name: spec.root_name,
            backup_id: spec.backup_id,
            pane_files: spec.pane_files,
        },
    )
    .unwrap_err();

    assert!(
        validation_error.contains("fixture root is missing")
            || validation_error.contains("missing required marker")
            || validation_error.contains("pane content file is missing"),
        "unexpected validation error: {validation_error}"
    );

    fs::remove_dir_all(&temp_root).expect("failed to clean broken fixture tree");
}
