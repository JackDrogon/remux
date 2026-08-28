//! Replay a persisted snapshot back into a live tmux server.
//!
//! The restore path validates pane assets before mutating tmux on purpose.
//! Snapshot reads are cheap to repeat, but tmux mutations are not transactional,
//! so this module keeps the "validate first, replay second" boundary explicit
//! to preserve fail-fast behavior and predictable recovery semantics.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use xerror::Context;

use crate::config::AppState;
use crate::error::{Restore as RestoreError, Result};
use crate::model::{Pane, Session, Tmux, Window};
use crate::storage::{
    LoadedSnapshot, PaneAsset, SnapshotDirectory, resolve_restore_target_in_root,
};
use crate::tmux_adapter::{TmuxClient, TmuxRuntimeOptions};

const DEFAULT_SESSION_SIZE: (u32, u32) = (10, 10);
const DUMMY_SESSION_SIZE: (u32, u32) = (10, 10);
const BASE_INDEX_OPTION: &str = "base-index";

pub fn restore_from_config(config: &AppState, requested_backup: Option<&str>) -> Result<String> {
    tracing::info!(
        requested_backup = requested_backup.unwrap_or("latest"),
        socket_name = config.socket_name().unwrap_or("default"),
        backup_root = %config.active_backup_path().display(),
        "starting restore"
    );
    let adapter = TmuxRuntimeOptions::new(&config.config().tmux.binary)
        .socket_name(config.socket_name())
        .content_with_escape(config.config().capture.with_escape)
        .build_adapter();
    let active_backup_path = config.active_backup_path();
    let backup_name = resolve_backup_name(&active_backup_path, requested_backup)?;
    restore_from_path_with_adapter(&active_backup_path, &adapter, &backup_name)?;
    tracing::info!(backup_name, "restore completed");
    Ok(backup_name)
}

pub fn resolve_backup_name(
    active_backup_path: &Path,
    requested_backup: Option<&str>,
) -> Result<String> {
    resolve_restore_target_in_root(active_backup_path, requested_backup)
}

pub fn restore_from_path_with_adapter(
    active_backup_path: &Path,
    adapter: &impl TmuxClient,
    backup_name: &str,
) -> Result<()> {
    let backup_dir = backup_dir_path(active_backup_path, backup_name);
    let snapshot = SnapshotDirectory::new(&backup_dir)
        .read_full()
        .with_context(|| format!("failed to load snapshot {}", backup_dir.display()))?;

    let mut engine = RestoreEngine::new(adapter);
    let restore_result = engine.restore_snapshot(&snapshot, &backup_dir);
    let cleanup_result = engine.cleanup_dummy_session();
    finish_restore(restore_result, cleanup_result)
}

fn finish_restore(restore_result: Result<()>, cleanup_result: Result<()>) -> Result<()> {
    match (restore_result, cleanup_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(error), Err(cleanup_error)) => {
            tracing::error!(
                error = %cleanup_error,
                debug_error = ?cleanup_error,
                "dummy session cleanup failed after restore failure"
            );
            Err(crate::error::attach_context(
                error,
                format!("dummy session cleanup also failed: {cleanup_error}"),
            ))
        }
    }
}

fn backup_dir_path(active_backup_path: &Path, backup_name: &str) -> PathBuf {
    active_backup_path.join(backup_name)
}

struct RestoreEngine<'a, T: TmuxClient + ?Sized> {
    adapter: &'a T,
    window_base_index: Option<usize>,
    dummy_session: Option<String>,
}

#[derive(Debug, Default)]
struct VerifiedPaneAssets {
    content_paths: BTreeMap<String, PathBuf>,
}

impl VerifiedPaneAssets {
    fn insert(&mut self, pane_id: String, content_path: PathBuf) {
        self.content_paths.insert(pane_id, content_path);
    }

    fn content_path(&self, pane_id: &str) -> Result<&Path> {
        let content_path = self
            .content_paths
            .get(pane_id)
            .map(PathBuf::as_path)
            .ok_or_else(|| RestoreError::MissingPaneAsset {
                pane_id: pane_id.to_string(),
            })?;
        Ok(content_path)
    }
}

impl<'a, T: TmuxClient + ?Sized> RestoreEngine<'a, T> {
    fn new(adapter: &'a T) -> Self {
        Self {
            adapter,
            window_base_index: None,
            dummy_session: None,
        }
    }

    fn restore_snapshot(&mut self, snapshot: &LoadedSnapshot, backup_dir: &Path) -> Result<()> {
        tracing::info!(
            backup_dir = %backup_dir.display(),
            session_count = snapshot.tmux.sessions.len(),
            pane_asset_count = snapshot.pane_assets.len(),
            "restoring snapshot into tmux"
        );
        let sessions_to_restore = self.collect_restorable_sessions(&snapshot.tmux)?;
        let verified_panes =
            self.validate_sessions(&sessions_to_restore, &snapshot.pane_assets, backup_dir)?;
        self.ensure_base_index_ready()?;

        tracing::info!(
            session_count = sessions_to_restore.len(),
            pane_asset_count = verified_panes.content_paths.len(),
            "validated restore inputs"
        );

        for session in sessions_to_restore {
            self.restore_session(session, &verified_panes)?;
        }

        Ok(())
    }

    fn collect_restorable_sessions<'b>(&self, tmux: &'b Tmux) -> Result<Vec<&'b Session>> {
        let has_server = self.adapter.has_server()?;
        let mut sessions_to_restore = Vec::new();

        for session in &tmux.sessions {
            if has_server && self.adapter.has_session(&session.name)? {
                tracing::debug!(session_name = %session.name, "skipping existing tmux session");
                continue;
            }

            sessions_to_restore.push(session);
        }

        Ok(sessions_to_restore)
    }

    fn cleanup_dummy_session(&mut self) -> Result<()> {
        if let Some(dummy_session) = self.dummy_session.take() {
            self.adapter.kill_session(&dummy_session)?;
        }

        Ok(())
    }

    fn ensure_base_index_ready(&mut self) -> Result<usize> {
        if let Some(window_base_index) = self.window_base_index {
            return Ok(window_base_index);
        }

        if !self.adapter.has_server()? {
            let dummy_session = generate_dummy_session_name();
            self.adapter.create_session(
                &dummy_session,
                DUMMY_SESSION_SIZE.0,
                DUMMY_SESSION_SIZE.1,
            )?;
            self.dummy_session = Some(dummy_session);
        }

        let raw = self.adapter.show_option(BASE_INDEX_OPTION)?;
        let window_base_index = raw
            .trim()
            .parse::<usize>()
            .map_err(|source| RestoreError::InvalidBaseIndex { raw, source })?;
        self.window_base_index = Some(window_base_index);
        Ok(window_base_index)
    }

    fn restore_session(
        &mut self,
        session: &Session,
        verified_panes: &VerifiedPaneAssets,
    ) -> Result<()> {
        let (width, height) = session.size.as_tuple().unwrap_or(DEFAULT_SESSION_SIZE);
        self.adapter.create_session(&session.name, width, height)?;

        let windows = session.windows_in_restore_order();
        for window in windows.iter().take(windows.len().saturating_sub(1)) {
            self.restore_window(window, verified_panes)?;
            self.adapter
                .create_empty_window(&session.name, self.ensure_base_index_ready()?)?;
        }

        if let Some(last_window) = windows.last() {
            self.restore_window(last_window, verified_panes)?;
        }

        Ok(())
    }

    fn restore_window(
        &mut self,
        window: &Window,
        verified_panes: &VerifiedPaneAssets,
    ) -> Result<()> {
        let window_base_index = self.ensure_base_index_ready()?;
        let window_id = window_id(window);

        self.restore_window_identity(window, window_base_index, window_id)?;
        self.restore_window_panes(window, window_id, verified_panes)?;
        self.adapter
            .select_layout(&window.session_name, window_id, &window.layout)?;
        Ok(())
    }

    fn restore_window_identity(
        &self,
        window: &Window,
        window_base_index: usize,
        window_id: usize,
    ) -> Result<()> {
        if window_base_index != window_id {
            self.adapter
                .renumber_window(&window.session_name, window_base_index, window_id)?;
        }

        self.adapter
            .rename_window(&window.session_name, window_id, &window.name)?;

        if window.active {
            self.adapter
                .select_window(&window.session_name, window_id)?;
        }

        Ok(())
    }

    fn restore_window_panes(
        &self,
        window: &Window,
        window_id: usize,
        verified_panes: &VerifiedPaneAssets,
    ) -> Result<()> {
        self.expand_window_panes(window, window_id)?;

        for pane in &window.panes {
            self.restore_pane(pane, verified_panes)?;
        }

        Ok(())
    }

    fn expand_window_panes(&self, window: &Window, window_id: usize) -> Result<()> {
        if window.panes.len() <= 1 {
            return Ok(());
        }

        let pane_min_id = pane_min_id(window);
        for _ in 0..window.panes.len() - 1 {
            self.adapter
                .split_window(&window.session_name, window_id, pane_min_id)?;
        }

        Ok(())
    }

    fn restore_pane(&self, pane: &Pane, verified_panes: &VerifiedPaneAssets) -> Result<()> {
        let pane_id = pane.pane_target();
        self.adapter
            .set_pane_path(pane_id.as_str(), Path::new(&pane.path))?;

        let content_path = verified_panes.content_path(pane_id.as_str())?;

        self.adapter
            .restore_pane_content(pane_id.as_str(), content_path)?;
        Ok(())
    }

    fn validate_sessions(
        &self,
        sessions: &[&Session],
        pane_assets: &BTreeMap<String, PaneAsset>,
        backup_dir: &Path,
    ) -> Result<VerifiedPaneAssets> {
        let mut verified = VerifiedPaneAssets::default();

        for session in sessions {
            for window in &session.windows {
                for pane in &window.panes {
                    let pane_id = pane.pane_target().into_string();
                    let content_path =
                        self.validated_pane_content_path(&pane_id, pane_assets, backup_dir)?;
                    verified.insert(pane_id, content_path);
                }
            }
        }

        Ok(verified)
    }

    fn validated_pane_content_path(
        &self,
        pane_id: &str,
        pane_assets: &BTreeMap<String, PaneAsset>,
        backup_dir: &Path,
    ) -> Result<PathBuf> {
        let asset = pane_assets
            .get(pane_id)
            .ok_or_else(|| RestoreError::MissingPaneAsset {
                pane_id: pane_id.to_string(),
            })?;
        SnapshotDirectory::new(backup_dir).validate_asset(pane_id, asset)
    }
}

fn window_id(window: &Window) -> usize {
    usize::try_from(window.window_id)
        .expect("u32 window ids should always fit into usize on supported targets")
}

fn pane_min_id(window: &Window) -> usize {
    let pane_min_id = window
        .min_pane_id()
        .expect("multi-pane windows must expose a minimum pane id");
    usize::try_from(pane_min_id)
        .expect("u32 pane ids should always fit into usize on supported targets")
}

fn generate_dummy_session_name() -> String {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("remux_dummy_{}_{}", std::process::id(), stamp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Code;

    fn restore_error(pane_id: &str) -> crate::Error {
        RestoreError::MissingPaneAsset {
            pane_id: pane_id.to_string(),
        }
        .into()
    }

    #[test]
    fn finish_restore_keeps_restore_error_when_cleanup_also_fails() {
        let error = finish_restore(
            Err(restore_error("restore-pane")),
            Err(restore_error("cleanup-pane")),
        )
        .expect_err("dual failure should surface the restore error");

        assert!(matches!(
            error.code(),
            Code::Restore(RestoreError::MissingPaneAsset { pane_id }) if pane_id == "restore-pane"
        ));
        assert_eq!(
            error.contexts().collect::<Vec<_>>(),
            ["dummy session cleanup also failed: missing pane metadata for cleanup-pane"]
        );
    }

    #[test]
    fn finish_restore_reports_cleanup_error_when_restore_succeeded() {
        let error = finish_restore(Ok(()), Err(restore_error("cleanup-pane")))
            .expect_err("cleanup failure must not be dropped");

        assert!(matches!(
            error.code(),
            Code::Restore(RestoreError::MissingPaneAsset { pane_id }) if pane_id == "cleanup-pane"
        ));
    }
}
