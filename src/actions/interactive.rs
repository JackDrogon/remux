use std::io::{self, BufRead, Write};

use thiserror::Error;

use crate::actions::restore;
use crate::cli::{catalog_render, ui};
use crate::config::AppState;
use crate::storage;

#[derive(Debug, Error)]
pub enum InteractiveError {
    #[error("interactive I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("end of input while reading {context}")]
    EndOfInput { context: &'static str },
    #[error(transparent)]
    Catalog(#[from] storage::CatalogError),
    #[error(transparent)]
    Restore(#[from] restore::RestoreError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlowMode {
    Inspect,
    Delete,
    Restore,
}

impl FlowMode {
    fn ui_mode(self) -> ui::InteractiveMode {
        match self {
            Self::Inspect => ui::InteractiveMode::Inspect,
            Self::Delete => ui::InteractiveMode::Delete,
            Self::Restore => ui::InteractiveMode::Restore,
        }
    }
}

pub fn interactive_list<R, W>(
    config: &AppState,
    input: &mut R,
    output: &mut W,
) -> Result<(), InteractiveError>
where
    R: BufRead,
    W: Write,
{
    loop {
        let backups = load_listing_backups(config)?;
        if backups.is_empty() {
            show_no_backups(output)?;
            return Ok(());
        }

        render_interactive_header(output, FlowMode::Inspect, backups.len())?;
        render_backup_summary(output, &backups)?;
        let Some(index) = prompt_for_backup_index(input, output, backups.len(), FlowMode::Inspect)?
        else {
            return Ok(());
        };

        let detail = load_backup_detail(config, &backups[index].backup_id)?;

        render_interactive_backup_detail(output, &detail)?;
    }
}

pub fn interactive_delete<R, W>(
    config: &AppState,
    input: &mut R,
    output: &mut W,
) -> Result<(), InteractiveError>
where
    R: BufRead,
    W: Write,
{
    run_flow(config, input, output, FlowMode::Delete)
}

pub fn interactive_restore<R, W>(
    config: &AppState,
    input: &mut R,
    output: &mut W,
) -> Result<(), InteractiveError>
where
    R: BufRead,
    W: Write,
{
    run_flow(config, input, output, FlowMode::Restore)
}

fn run_flow<R, W>(
    config: &AppState,
    input: &mut R,
    output: &mut W,
    mode: FlowMode,
) -> Result<(), InteractiveError>
where
    R: BufRead,
    W: Write,
{
    loop {
        let backups = load_backups(config)?;
        if backups.is_empty() {
            show_no_backups(output)?;
            return Ok(());
        }

        render_interactive_header(output, mode, backups.len())?;
        render_backup_summary(output, &backups)?;
        let Some(index) = prompt_for_backup_index(input, output, backups.len(), mode)? else {
            return Ok(());
        };

        let selected_backup = backups[index].backup_id.clone();
        let detail = load_backup_detail(config, &selected_backup)?;
        render_backup_detail(output, &detail)?;

        match mode {
            FlowMode::Inspect => continue,
            FlowMode::Delete => {
                if !prompt_for_confirmation(
                    input,
                    output,
                    &ui::confirmation_prompt(mode.ui_mode(), &selected_backup),
                )? {
                    continue;
                }

                delete_backup(config, &selected_backup)?;
                write_line(output, &format!("Backup {selected_backup} was deleted"))?;
            }
            FlowMode::Restore => {
                if !prompt_for_confirmation(
                    input,
                    output,
                    &ui::confirmation_prompt(mode.ui_mode(), &selected_backup),
                )? {
                    continue;
                }

                restore::restore_from_config(config, Some(&selected_backup))?;
                write_line(output, &format!("Backup {selected_backup} was restored"))?;
                return Ok(());
            }
        }
    }
}

fn prompt_for_backup_index<R, W>(
    input: &mut R,
    output: &mut W,
    backup_count: usize,
    mode: FlowMode,
) -> Result<Option<usize>, InteractiveError>
where
    R: BufRead,
    W: Write,
{
    loop {
        let prompt_text = ui::selection_prompt(mode.ui_mode());
        prompt(output, &prompt_text)?;

        let Some(line) = read_line(input)? else {
            return match mode {
                FlowMode::Inspect => Ok(None),
                FlowMode::Delete | FlowMode::Restore => Err(InteractiveError::EndOfInput {
                    context: "backup selection",
                }),
            };
        };
        let trimmed = line.trim();

        if trimmed.eq_ignore_ascii_case("q") {
            return Ok(None);
        }

        if trimmed.is_empty() {
            write_line(output, "Invalid index: (empty)")?;
            continue;
        }

        let Ok(index) = trimmed.parse::<usize>() else {
            write_line(
                output,
                &format!("Invalid index: {trimmed} (expected 1..={backup_count})"),
            )?;
            continue;
        };

        if !(1..=backup_count).contains(&index) {
            write_line(
                output,
                &format!("Invalid index: {trimmed} (expected 1..={backup_count})"),
            )?;
            continue;
        }

        return Ok(Some(index - 1));
    }
}

fn prompt_for_confirmation<R, W>(
    input: &mut R,
    output: &mut W,
    prompt_text: &str,
) -> Result<bool, InteractiveError>
where
    R: BufRead,
    W: Write,
{
    loop {
        prompt(output, prompt_text)?;

        let Some(line) = read_line(input)? else {
            return Err(InteractiveError::EndOfInput {
                context: "confirmation",
            });
        };
        let trimmed = line.trim();

        if trimmed.eq_ignore_ascii_case("yes") {
            return Ok(true);
        }

        if trimmed.eq_ignore_ascii_case("no") {
            return Ok(false);
        }

        if trimmed.is_empty() {
            write_line(output, "Invalid confirmation: (empty)")?;
        } else {
            write_line(output, &format!("Invalid confirmation: {trimmed}"))?;
        }
    }
}

fn read_line<R>(input: &mut R) -> Result<Option<String>, InteractiveError>
where
    R: BufRead,
{
    let mut line = String::new();
    let bytes_read = input.read_line(&mut line)?;
    if bytes_read == 0 {
        return Ok(None);
    }

    Ok(Some(line))
}

fn load_listing_backups(config: &AppState) -> Result<Vec<storage::BackupEntry>, InteractiveError> {
    storage::list_backups_for_listing(config).map_err(Into::into)
}

fn load_backups(config: &AppState) -> Result<Vec<storage::BackupEntry>, InteractiveError> {
    storage::list_backups(config).map_err(Into::into)
}

fn load_backup_detail(
    config: &AppState,
    backup_id: &str,
) -> Result<storage::BackupEntry, InteractiveError> {
    storage::load_backup(config, backup_id).map_err(Into::into)
}

fn delete_backup(config: &AppState, backup_id: &str) -> Result<(), InteractiveError> {
    storage::delete_backup(config, backup_id).map_err(Into::into)
}

fn render_backup_summary<W>(
    output: &mut W,
    backups: &[storage::BackupEntry],
) -> Result<(), InteractiveError>
where
    W: Write,
{
    write_line(output, &catalog_render::render_summary(backups))
}

fn render_backup_detail<W>(
    output: &mut W,
    detail: &storage::BackupEntry,
) -> Result<(), InteractiveError>
where
    W: Write,
{
    write_line(output, &catalog_render::render_detail(detail))
}

fn render_interactive_backup_detail<W>(
    output: &mut W,
    detail: &storage::BackupEntry,
) -> Result<(), InteractiveError>
where
    W: Write,
{
    write_line(output, &catalog_render::render_interactive_detail(detail))
}

fn render_interactive_header<W>(
    output: &mut W,
    mode: FlowMode,
    backup_count: usize,
) -> Result<(), InteractiveError>
where
    W: Write,
{
    write_line(
        output,
        &ui::interactive_header(mode.ui_mode(), backup_count),
    )
}

fn show_no_backups<W>(output: &mut W) -> Result<(), InteractiveError>
where
    W: Write,
{
    write_line(output, catalog_render::no_backups_message())?;
    output.flush()?;
    Ok(())
}

fn write_line<W>(output: &mut W, message: &str) -> Result<(), InteractiveError>
where
    W: Write,
{
    writeln!(output, "{message}")?;
    Ok(())
}

fn prompt<W>(output: &mut W, message: &str) -> Result<(), InteractiveError>
where
    W: Write,
{
    write!(output, "{message}")?;
    output.flush()?;
    Ok(())
}
