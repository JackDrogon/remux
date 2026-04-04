use std::path::Path;

use super::command::{pane_path_keys, window_target};
use super::error::SubprocessError;

pub trait TmuxClient {
    fn has_server(&self) -> Result<bool, SubprocessError>;
    fn list_sessions(&self) -> Result<Vec<String>, SubprocessError>;
    fn list_windows(&self, session_name: &str) -> Result<Vec<String>, SubprocessError>;
    fn list_panes(
        &self,
        session_name: &str,
        window_index: usize,
    ) -> Result<Vec<String>, SubprocessError>;
    fn create_session(
        &self,
        session_name: &str,
        width: u32,
        height: u32,
    ) -> Result<(), SubprocessError>;
    fn kill_session(&self, session_name: &str) -> Result<bool, SubprocessError>;
    fn capture_pane(&self, pane_id: &str) -> Result<String, SubprocessError>;
    fn capture_pane_bytes(&self, pane_id: &str) -> Result<Vec<u8>, SubprocessError>;
    fn show_option(&self, option: &str) -> Result<String, SubprocessError>;
    fn has_session(&self, session_name: &str) -> Result<bool, SubprocessError>;
    fn clear_pane(&self, pane_id: &str) -> Result<(), SubprocessError>;
    fn send_keys(&self, target: &str, keys: &str) -> Result<(), SubprocessError>;
    fn set_pane_path(&self, pane_id: &str, path: &Path) -> Result<(), SubprocessError> {
        self.clear_pane(pane_id)?;
        self.send_keys(pane_id, &pane_path_keys(path))?;
        self.clear_pane(pane_id)?;
        Ok(())
    }
    fn create_empty_window(
        &self,
        session_name: &str,
        base_index: usize,
    ) -> Result<(), SubprocessError>;
    fn move_window(&self, source: &str, target: &str) -> Result<(), SubprocessError>;
    fn renumber_window(
        &self,
        session_name: &str,
        from_window_id: usize,
        to_window_id: usize,
    ) -> Result<(), SubprocessError> {
        self.move_window(
            &window_target(session_name, from_window_id),
            &window_target(session_name, to_window_id),
        )
    }
    fn rename_window(
        &self,
        session_name: &str,
        window_id: usize,
        name: &str,
    ) -> Result<(), SubprocessError>;
    fn select_window(&self, session_name: &str, window_id: usize) -> Result<(), SubprocessError>;
    fn split_window(
        &self,
        session_name: &str,
        window_id: usize,
        pane_min_id: usize,
    ) -> Result<(), SubprocessError>;
    fn select_layout(
        &self,
        session_name: &str,
        window_id: usize,
        layout: &str,
    ) -> Result<(), SubprocessError>;
    fn restore_pane_content(&self, pane_id: &str, filename: &Path) -> Result<(), SubprocessError>;
}
