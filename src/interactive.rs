use std::io::{self, BufRead, Write};

use crate::{config::AppState, restore, storage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlowMode {
    Inspect,
    Delete,
    Restore,
}

pub fn interactive_list<R, W>(
    config: &AppState,
    input: &mut R,
    output: &mut W,
) -> Result<(), String>
where
    R: BufRead,
    W: Write,
{
    let backups = storage::list_backups_for_listing(config).map_err(stringify_error)?;
    if backups.is_empty() {
        show_no_backups(output)?;
        return Ok(());
    }

    let mut detail_cache = vec![None; backups.len()];

    loop {
        write_line(output, &storage::render_summary(&backups))?;
        let Some(index) = prompt_for_backup_index(input, output, backups.len(), FlowMode::Inspect)?
        else {
            return Ok(());
        };

        if detail_cache[index].is_none() {
            let detail =
                storage::load_backup(config, &backups[index].id).map_err(stringify_error)?;
            detail_cache[index] = Some(detail);
        }

        let detail = detail_cache[index]
            .as_ref()
            .expect("interactive list detail cache should be populated");

        write_line(output, &storage::render_interactive_detail(detail))?;
    }
}

pub fn interactive_delete<R, W>(
    config: &AppState,
    input: &mut R,
    output: &mut W,
) -> Result<(), String>
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
) -> Result<(), String>
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
) -> Result<(), String>
where
    R: BufRead,
    W: Write,
{
    loop {
        let backups = storage::list_backups(config).map_err(stringify_error)?;
        if backups.is_empty() {
            show_no_backups(output)?;
            return Ok(());
        }

        write_line(output, &storage::render_summary(&backups))?;
        let Some(index) = prompt_for_backup_index(input, output, backups.len(), mode)? else {
            return Ok(());
        };

        let selected_backup = backups[index].id.clone();
        let detail = storage::load_backup(config, &selected_backup).map_err(stringify_error)?;
        write_line(output, &storage::render_detail(&detail))?;

        match mode {
            FlowMode::Inspect => continue,
            FlowMode::Delete => {
                if !prompt_for_confirmation(
                    input,
                    output,
                    &format!("remux> Delete backup {selected_backup}? [yes|no] "),
                )? {
                    continue;
                }

                storage::delete_backup(config, &selected_backup).map_err(stringify_error)?;
                write_line(output, &format!("Backup {selected_backup} was deleted"))?;
            }
            FlowMode::Restore => {
                if !prompt_for_confirmation(
                    input,
                    output,
                    &format!("remux> restore {selected_backup}? [yes|no] "),
                )? {
                    continue;
                }

                restore::restore_from_config(config, Some(&selected_backup))
                    .map_err(stringify_error)?;
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
) -> Result<Option<usize>, String>
where
    R: BufRead,
    W: Write,
{
    loop {
        prompt(output, "remux> Please give backup No. (press q to exit): ")?;

        let Some(line) = read_line(input)? else {
            return match mode {
                FlowMode::Inspect => Ok(None),
                FlowMode::Delete | FlowMode::Restore => {
                    Err("end of input while reading backup selection".to_string())
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
            write_line(output, &format!("Invalid index: {trimmed}"))?;
            continue;
        };

        if !(1..=backup_count).contains(&index) {
            write_line(output, &format!("Invalid index: {trimmed}"))?;
            continue;
        }

        return Ok(Some(index - 1));
    }
}

fn prompt_for_confirmation<R, W>(
    input: &mut R,
    output: &mut W,
    prompt_text: &str,
) -> Result<bool, String>
where
    R: BufRead,
    W: Write,
{
    loop {
        prompt(output, prompt_text)?;

        let Some(line) = read_line(input)? else {
            return Err("end of input while reading confirmation".to_string());
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

fn read_line<R>(input: &mut R) -> Result<Option<String>, String>
where
    R: BufRead,
{
    let mut line = String::new();
    let bytes_read = input.read_line(&mut line).map_err(io_error)?;
    if bytes_read == 0 {
        return Ok(None);
    }

    Ok(Some(line))
}

fn io_error(error: io::Error) -> String {
    format!("interactive I/O failed: {error}")
}

fn stringify_error(error: impl ToString) -> String {
    error.to_string()
}

fn show_no_backups<W>(output: &mut W) -> Result<(), String>
where
    W: Write,
{
    write_line(output, storage::no_backups_message())?;
    output.flush().map_err(io_error)
}

fn write_line<W>(output: &mut W, message: &str) -> Result<(), String>
where
    W: Write,
{
    writeln!(output, "{message}").map_err(io_error)
}

fn prompt<W>(output: &mut W, message: &str) -> Result<(), String>
where
    W: Write,
{
    write!(output, "{message}").map_err(io_error)?;
    output.flush().map_err(io_error)
}
