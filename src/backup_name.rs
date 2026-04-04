use std::fmt;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupNameError {
    raw: String,
    kind: BackupNameErrorKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum BackupNameErrorKind {
    Empty,
    ParentTraversal,
    PathSeparator,
    AbsoluteLike,
}

impl fmt::Display for BackupNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            BackupNameErrorKind::Empty => {
                write!(
                    f,
                    "invalid backup name: backup name cannot be empty or whitespace-only"
                )
            }
            BackupNameErrorKind::ParentTraversal => {
                write!(
                    f,
                    "invalid backup name {:?}: backup name cannot be '..'",
                    self.raw
                )
            }
            BackupNameErrorKind::PathSeparator => write!(
                f,
                "invalid backup name {:?}: backup name cannot contain path separators",
                self.raw,
            ),
            BackupNameErrorKind::AbsoluteLike => write!(
                f,
                "invalid backup name {:?}: backup name cannot be absolute-like",
                self.raw,
            ),
        }
    }
}

impl std::error::Error for BackupNameError {}

pub fn normalize_backup_name(raw: &str) -> Result<String, BackupNameError> {
    let normalized = raw.trim();
    if normalized.is_empty() {
        return Err(invalid_backup_name(raw, BackupNameErrorKind::Empty));
    }

    if normalized == ".." {
        return Err(invalid_backup_name(
            raw,
            BackupNameErrorKind::ParentTraversal,
        ));
    }

    if normalized.starts_with('\\') || Path::new(normalized).is_absolute() {
        return Err(invalid_backup_name(raw, BackupNameErrorKind::AbsoluteLike));
    }

    if normalized.contains('/') || normalized.contains('\\') {
        return Err(invalid_backup_name(raw, BackupNameErrorKind::PathSeparator));
    }

    Ok(normalized.to_string())
}

fn invalid_backup_name(raw: &str, kind: BackupNameErrorKind) -> BackupNameError {
    BackupNameError {
        raw: raw.to_string(),
        kind,
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_backup_name;

    #[test]
    fn trims_valid_backup_names() {
        assert_eq!(
            normalize_backup_name("  backup_20240101_120000  ")
                .expect("trimmed backup names should normalize"),
            "backup_20240101_120000"
        );
        assert_eq!(
            normalize_backup_name("team backup").expect("internal spaces should remain intact"),
            "team backup"
        );
    }

    #[test]
    fn rejects_invalid_backup_names() {
        let cases = [
            ("", "empty or whitespace-only"),
            ("   ", "empty or whitespace-only"),
            ("..", "cannot be '..'"),
            ("nested/name", "path separators"),
            ("nested\\name", "path separators"),
            ("/tmp/backup", "absolute-like"),
        ];

        for (raw, expected_detail) in cases {
            let error = normalize_backup_name(raw).expect_err("invalid backup name should fail");
            let message = error.to_string();
            assert!(
                message.contains(expected_detail),
                "expected {raw:?} to mention {expected_detail:?}, got: {message}"
            );
        }
    }
}
