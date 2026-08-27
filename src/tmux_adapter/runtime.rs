use super::TmuxAdapter;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmuxRuntimeOptions {
    binary: String,
    socket_name: Option<String>,
    content_with_escape: bool,
}

impl TmuxRuntimeOptions {
    pub fn new(binary: impl Into<String>) -> Self {
        Self {
            binary: binary.into(),
            socket_name: None,
            content_with_escape: true,
        }
    }

    pub fn socket_name(mut self, socket_name: Option<&str>) -> Self {
        self.socket_name = socket_name.map(ToOwned::to_owned);
        self
    }

    pub fn content_with_escape(mut self, content_with_escape: bool) -> Self {
        self.content_with_escape = content_with_escape;
        self
    }

    pub fn build_adapter(self) -> TmuxAdapter {
        TmuxAdapter::from_prefix(
            tmux_command_prefix(&self.binary, self.socket_name.as_deref()),
            self.content_with_escape,
        )
    }
}

pub fn tmux_command_prefix(binary: &str, socket_name: Option<&str>) -> Vec<String> {
    let mut prefix = vec![binary.to_string()];
    if let Some(socket_name) = socket_name {
        prefix.push("-L".to_string());
        prefix.push(socket_name.to_string());
    }
    prefix
}
