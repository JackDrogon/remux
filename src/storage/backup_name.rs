use std::fmt;
use std::path::Path;

use chrono::NaiveDateTime;

use crate::{Catalog as CatalogError, Result};

pub fn normalize_backup_name(raw: &str) -> Result<String> {
    let normalized = raw.trim();
    if normalized.is_empty() {
        return Err(CatalogError::BackupNameEmpty.into());
    }

    if normalized == ".." {
        return Err(CatalogError::BackupNameParentTraversal {
            raw: raw.to_string(),
        }
        .into());
    }

    if normalized.starts_with('\\') || Path::new(normalized).is_absolute() {
        return Err(CatalogError::BackupNameAbsoluteLike {
            raw: raw.to_string(),
        }
        .into());
    }

    if normalized.contains('/') || normalized.contains('\\') {
        return Err(CatalogError::BackupNamePathSeparator {
            raw: raw.to_string(),
        }
        .into());
    }

    Ok(normalized.to_string())
}

const AUTOMATIC_BACKUP_ID_FORMAT: &str = "%Y%m%d_%H%M%S";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupId(String);

impl BackupId {
    pub fn parse_custom(raw: &str) -> Result<Self> {
        Ok(Self(normalize_backup_name(raw)?))
    }

    pub fn automatic_at(local_time: NaiveDateTime) -> Self {
        Self(local_time.format(AUTOMATIC_BACKUP_ID_FORMAT).to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }

    pub fn is_automatic(&self) -> bool {
        is_automatic_backup_id(self.as_str())
    }
}

impl AsRef<str> for BackupId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for BackupId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

pub fn is_automatic_backup_id(name: &str) -> bool {
    name.len() == 15 && NaiveDateTime::parse_from_str(name, AUTOMATIC_BACKUP_ID_FORMAT).is_ok()
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

        let generated = super::BackupId::automatic_at(
            chrono::NaiveDate::from_ymd_opt(2024, 1, 2)
                .expect("test date should be valid")
                .and_hms_opt(3, 4, 5)
                .expect("test time should be valid"),
        );
        assert_eq!(generated.as_str(), "20240102_030405");
        assert!(generated.is_automatic());
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
