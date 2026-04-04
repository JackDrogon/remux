use crate::BINARY_NAME;

const ASCII_LOGO: &str = concat!(
    "    ____  ________  ______  __\n",
    "   / __ \\/ ____/  |/  / / / /\n",
    "  / /_/ / __/ / /|_/ / / / / \n",
    " / _, _/ /___/ /  / / /_/ /  \n",
    "/_/ |_/_____/_/  /_/\\____/   "
);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractiveMode {
    Inspect,
    Delete,
    Restore,
}

impl InteractiveMode {
    fn title(self) -> &'static str {
        match self {
            Self::Inspect => "Browse backups",
            Self::Delete => "Delete backups",
            Self::Restore => "Restore backups",
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            Self::Inspect => "Inspect a saved snapshot without changing anything.",
            Self::Delete => "Review a snapshot before permanently removing it.",
            Self::Restore => "Review a snapshot before replaying it into tmux.",
        }
    }

    fn prompt_label(self) -> &'static str {
        match self {
            Self::Inspect => "list",
            Self::Delete => "delete",
            Self::Restore => "restore",
        }
    }
}

pub fn about_line() -> String {
    format!("{BINARY_NAME} {}", env!("CARGO_PKG_VERSION"))
}

pub fn brand_block() -> String {
    format!(
        "{ASCII_LOGO}\n{}\ntmux session backup and restore with interactive recovery",
        about_line()
    )
}

pub fn interactive_header(mode: InteractiveMode, backup_count: usize) -> String {
    let noun = if backup_count == 1 {
        "backup"
    } else {
        "backups"
    };
    format!(
        "{}\n\n{}\n{}\nAvailable: {} {}\nHint: latest backup is marked with (*) and q exits this screen.",
        brand_block(),
        mode.title(),
        mode.subtitle(),
        backup_count,
        noun
    )
}

pub fn selection_prompt(mode: InteractiveMode) -> String {
    format!(
        "remux[{}]> Select backup number (q to exit): ",
        mode.prompt_label()
    )
}

pub fn confirmation_prompt(mode: InteractiveMode, backup_name: &str) -> String {
    match mode {
        InteractiveMode::Inspect => {
            format!("remux[list]> open backup {backup_name}? [yes|no] ")
        }
        InteractiveMode::Delete => {
            format!("remux[delete]> delete backup {backup_name}? [yes|no] ")
        }
        InteractiveMode::Restore => {
            format!("remux[restore]> restore backup {backup_name}? [yes|no] ")
        }
    }
}

pub fn detail_header(backup_name: &str, interactive: bool) -> String {
    let marker = if interactive { '>' } else { '=' };
    let border = marker.to_string().repeat(72);
    format!("{border}\nBackup: {backup_name}\n{border}")
}
