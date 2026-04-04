use super::TMUX_BIN;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmuxCommandPrefixBuilder {
    binary: String,
    socket_name: Option<String>,
}

impl TmuxCommandPrefixBuilder {
    pub fn new(binary: impl Into<String>) -> Self {
        Self {
            binary: binary.into(),
            socket_name: None,
        }
    }

    pub fn system_default() -> Self {
        Self::new(TMUX_BIN)
    }

    pub fn socket_name(mut self, socket_name: Option<&str>) -> Self {
        self.socket_name = socket_name.map(ToOwned::to_owned);
        self
    }

    pub fn build(self) -> Vec<String> {
        let mut prefix = vec![self.binary];
        if let Some(socket_name) = self.socket_name {
            prefix.push("-L".to_string());
            prefix.push(socket_name);
        }
        prefix
    }
}

pub fn tmux_command_prefix(binary: &str, socket_name: Option<&str>) -> Vec<String> {
    TmuxCommandPrefixBuilder::new(binary)
        .socket_name(socket_name)
        .build()
}
