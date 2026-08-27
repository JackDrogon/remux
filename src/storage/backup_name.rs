use std::path::Path;

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BackupNameError {
    #[error("invalid backup name: backup name cannot be empty or whitespace-only")]
    Empty,
    #[error("invalid backup name {raw:?}: backup name cannot be '..'")]
    ParentTraversal { raw: String },
    #[error("invalid backup name {raw:?}: backup name cannot contain path separators")]
    PathSeparator { raw: String },
    #[error("invalid backup name {raw:?}: backup name cannot be absolute-like")]
    AbsoluteLike { raw: String },
}

pub fn normalize_backup_name(raw: &str) -> Result<String, BackupNameError> {
    let normalized = raw.trim();
    if normalized.is_empty() {
        return Err(BackupNameError::Empty);
    }

    if normalized == ".." {
        return Err(BackupNameError::ParentTraversal {
            raw: raw.to_string(),
        });
    }

    if normalized.starts_with('\\') || Path::new(normalized).is_absolute() {
        return Err(BackupNameError::AbsoluteLike {
            raw: raw.to_string(),
        });
    }

    if normalized.contains('/') || normalized.contains('\\') {
        return Err(BackupNameError::PathSeparator {
            raw: raw.to_string(),
        });
    }

    Ok(normalized.to_string())
}

const AUTOMATIC_BACKUP_ID_FORMAT: &str = "%Y%m%d_%H%M%S";

pub fn is_automatic_backup_id(name: &str) -> bool {
    name.len() == 15
        && chrono::NaiveDateTime::parse_from_str(name, AUTOMATIC_BACKUP_ID_FORMAT).is_ok()
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
    fn recognizes_automatic_timestamp_backup_ids() {
        assert!(super::is_automatic_backup_id("20240101_120000"));
        assert!(!super::is_automatic_backup_id("backup_20240101_120000"));
        assert!(!super::is_automatic_backup_id("20241301_120000"));
        assert!(!super::is_automatic_backup_id("sprint_demo"));
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
