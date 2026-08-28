use std::io::{BufRead, Write};

use crate::actions::restore;
use crate::cli::catalog_render;
use crate::cli::ui::{self, InteractiveMode};
use crate::config::AppState;
use crate::error::{Interactive as InteractiveError, Result};
use crate::storage;

pub fn interactive_list<R, W>(config: &AppState, input: &mut R, output: &mut W) -> Result<()>
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

        render_interactive_header(output, InteractiveMode::Inspect, backups.len())?;
        render_backup_summary(output, &backups)?;
        let Some(index) =
            prompt_for_backup_index(input, output, backups.len(), InteractiveMode::Inspect)?
        else {
            return Ok(());
        };

        let detail = load_backup_detail(config, &backups[index].backup_id)?;

        render_interactive_backup_detail(output, &detail)?;
    }
}

pub fn interactive_delete<R, W>(config: &AppState, input: &mut R, output: &mut W) -> Result<()>
where
    R: BufRead,
    W: Write,
{
    run_flow(config, input, output, InteractiveMode::Delete)
}

pub fn interactive_restore<R, W>(config: &AppState, input: &mut R, output: &mut W) -> Result<()>
where
    R: BufRead,
    W: Write,
{
    run_flow(config, input, output, InteractiveMode::Restore)
}

fn run_flow<R, W>(
    config: &AppState,
    input: &mut R,
    output: &mut W,
    mode: InteractiveMode,
) -> Result<()>
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
            InteractiveMode::Inspect => continue,
            InteractiveMode::Delete => {
                if !prompt_for_confirmation(
                    input,
                    output,
                    &ui::confirmation_prompt(mode, &selected_backup),
                )? {
                    continue;
                }

                delete_backup(config, &selected_backup)?;
                write_line(output, &format!("Backup {selected_backup} was deleted"))?;
            }
            InteractiveMode::Restore => {
                if !prompt_for_confirmation(
                    input,
                    output,
                    &ui::confirmation_prompt(mode, &selected_backup),
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
    mode: InteractiveMode,
) -> Result<Option<usize>>
where
    R: BufRead,
    W: Write,
{
    loop {
        let prompt_text = ui::selection_prompt(mode);
        prompt(output, &prompt_text)?;

        let Some(line) = read_line(input)? else {
            return match mode {
                InteractiveMode::Inspect => Ok(None),
                InteractiveMode::Delete | InteractiveMode::Restore => {
                    Err(InteractiveError::EndOfInput {
                        context: "backup selection",
                    }
                    .into())
                }
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

fn prompt_for_confirmation<R, W>(input: &mut R, output: &mut W, prompt_text: &str) -> Result<bool>
where
    R: BufRead,
    W: Write,
{
    loop {
        prompt(output, prompt_text)?;

        let Some(line) = read_line(input)? else {
            return Err(InteractiveError::EndOfInput {
                context: "confirmation",
            }
            .into());
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

fn read_line<R>(input: &mut R) -> Result<Option<String>>
where
    R: BufRead,
{
    let mut line = String::new();
    let bytes_read = input
        .read_line(&mut line)
        .map_err(InteractiveError::InteractiveIo)?;
    if bytes_read == 0 {
        return Ok(None);
    }

    Ok(Some(line))
}

fn load_listing_backups(config: &AppState) -> Result<Vec<storage::BackupEntry>> {
    storage::list_backups_for_listing(config)
}

fn load_backups(config: &AppState) -> Result<Vec<storage::BackupEntry>> {
    storage::list_backups(config)
}

fn load_backup_detail(config: &AppState, backup_id: &str) -> Result<storage::BackupEntry> {
    storage::load_backup(config, backup_id)
}

fn delete_backup(config: &AppState, backup_id: &str) -> Result<()> {
    storage::delete_backup(config, backup_id)
}

fn render_backup_summary<W>(output: &mut W, backups: &[storage::BackupEntry]) -> Result<()>
where
    W: Write,
{
    write_line(output, &catalog_render::render_summary(backups))
}

fn render_backup_detail<W>(output: &mut W, detail: &storage::BackupEntry) -> Result<()>
where
    W: Write,
{
    write_line(output, &catalog_render::render_detail(detail))
}

fn render_interactive_backup_detail<W>(output: &mut W, detail: &storage::BackupEntry) -> Result<()>
where
    W: Write,
{
    write_line(output, &catalog_render::render_interactive_detail(detail))
}

fn render_interactive_header<W>(
    output: &mut W,
    mode: InteractiveMode,
    backup_count: usize,
) -> Result<()>
where
    W: Write,
{
    write_line(output, &ui::interactive_header(mode, backup_count))
}

fn show_no_backups<W>(output: &mut W) -> Result<()>
where
    W: Write,
{
    write_line(output, catalog_render::no_backups_message())?;
    output.flush().map_err(InteractiveError::InteractiveIo)?;
    Ok(())
}

fn write_line<W>(output: &mut W, message: &str) -> Result<()>
where
    W: Write,
{
    // Interactive flow: BrokenPipe is still InteractiveIo, not CLI success.
    writeln!(output, "{message}").map_err(InteractiveError::InteractiveIo)?;
    Ok(())
}

fn prompt<W>(output: &mut W, message: &str) -> Result<()>
where
    W: Write,
{
    write!(output, "{message}").map_err(InteractiveError::InteractiveIo)?;
    output.flush().map_err(InteractiveError::InteractiveIo)?;
    Ok(())
}

#[cfg(test)]
mod io_contract_tests {
    use super::*;
    use crate::{Category, Code};
    use std::io::Cursor;

    #[test]
    fn read_line_eof_is_none_not_interactive_io() {
        let mut input = Cursor::new(Vec::<u8>::new());
        let line = read_line(&mut input).expect("EOF is not an I/O error");
        assert_eq!(line, None);
    }

    #[test]
    fn write_line_broken_pipe_is_interactive_io() {
        let (mut writer, reader) = std::os::unix::net::UnixStream::pair().expect("pipe");
        drop(reader);
        let err = write_line(&mut writer, "hello").expect_err("BrokenPipe stays Interactive");
        assert_eq!(err.category(), Category::Interactive);
        assert!(matches!(
            err.code(),
            Code::Interactive(InteractiveError::InteractiveIo(_))
        ));
    }
}
