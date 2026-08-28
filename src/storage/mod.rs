mod backup_name;
mod fs_ops;
mod snapshot_contract;

pub mod catalog;
pub mod config_loader;
pub mod fingerprint;
pub mod snapshot;

pub use backup_name::{BackupId, is_automatic_backup_id, normalize_backup_name};
pub use catalog::{
    BackupEntry, delete_backup, latest_backup, list_backups, list_backups_for_listing, load_backup,
    load_newest_backups, resolve_restore_target, resolve_restore_target_in_root,
};
pub use config_loader::{DEFAULT_CONFIG_TEMPLATE, load_or_init_app_config};
pub use fingerprint::CompactFingerprint;
pub use snapshot::{
    LoadedSnapshot, MANIFEST_FILE_NAME, PaneAsset, SUMMARY_FILE_NAME, SnapshotDirectory,
    read_schema_version, read_snapshot_dir, read_snapshot_summary_dir, validate_pane_asset,
    validate_pane_assets, write_snapshot_dir,
};
pub use snapshot_contract::PaneEncoding;
