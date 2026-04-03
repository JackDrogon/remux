use std::io::{self, BufRead, Write};

use crate::{catalog, config::RuntimeConfig, restore};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlowMode {
    Inspect,
    Delete,
    Restore,
}

pub fn interactive_list<R, W>(
    config: &RuntimeConfig,
    input: &mut R,
    output: &mut W,
) -> Result<(), String>
where
    R: BufRead,
    W: Write,
{
    run_flow(config, input, output, FlowMode::Inspect)
}

pub fn interactive_delete<R, W>(
    config: &RuntimeConfig,
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
    config: &RuntimeConfig,
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
    config: &RuntimeConfig,
    input: &mut R,
    output: &mut W,
    mode: FlowMode,
) -> Result<(), String>
where
    R: BufRead,
    W: Write,
{
    loop {
        let backups = catalog::list_backups(config).map_err(|error| error.to_string())?;
        if backups.is_empty() {
            writeln!(output, "{}", catalog::no_backups_message()).map_err(io_error)?;
            output.flush().map_err(io_error)?;
            return Ok(());
        }

        writeln!(output, "{}", catalog::render_summary(&backups)).map_err(io_error)?;
        let Some(index) = prompt_for_backup_index(input, output, backups.len())? else {
            return Ok(());
        };

        let selected_backup = backups[index].id.clone();
        let detail =
            catalog::load_backup(config, &selected_backup).map_err(|error| error.to_string())?;
        writeln!(output, "{}", catalog::render_detail(&detail)).map_err(io_error)?;

        match mode {
            FlowMode::Inspect => continue,
            FlowMode::Delete => {
                if !prompt_for_confirmation(
                    input,
                    output,
                    &format!("retmux> Delete backup {selected_backup}? [yes|no] "),
                )? {
                    continue;
                }

                catalog::delete_backup(config, &selected_backup)
                    .map_err(|error| error.to_string())?;
                writeln!(output, "Backup {selected_backup} was deleted").map_err(io_error)?;
            }
            FlowMode::Restore => {
                if !prompt_for_confirmation(
                    input,
                    output,
                    &format!("retmux> restore {selected_backup}? [yes|no] "),
                )? {
                    continue;
                }

                restore::restore_from_config(config, Some(&selected_backup))
                    .map_err(|error| error.to_string())?;
                writeln!(output, "Backup {selected_backup} was restored").map_err(io_error)?;
                return Ok(());
            }
        }
    }
}

fn prompt_for_backup_index<R, W>(
    input: &mut R,
    output: &mut W,
    backup_count: usize,
) -> Result<Option<usize>, String>
where
    R: BufRead,
    W: Write,
{
    loop {
        write!(output, "retmux> Please give backup No. (press q to exit): ").map_err(io_error)?;
        output.flush().map_err(io_error)?;

        let Some(line) = read_line(input)? else {
            return Err("end of input while reading backup selection".to_string());
        };
        let trimmed = line.trim();

        if trimmed.eq_ignore_ascii_case("q") {
            return Ok(None);
        }

        if trimmed.is_empty() {
            writeln!(output, "Invalid index: (empty)").map_err(io_error)?;
            continue;
        }

        let Ok(index) = trimmed.parse::<usize>() else {
            writeln!(output, "Invalid index: {trimmed}").map_err(io_error)?;
            continue;
        };

        if !(1..=backup_count).contains(&index) {
            writeln!(output, "Invalid index: {trimmed}").map_err(io_error)?;
            continue;
        }

        return Ok(Some(index - 1));
    }
}

fn prompt_for_confirmation<R, W>(
    input: &mut R,
    output: &mut W,
    prompt: &str,
) -> Result<bool, String>
where
    R: BufRead,
    W: Write,
{
    loop {
        write!(output, "{prompt}").map_err(io_error)?;
        output.flush().map_err(io_error)?;

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
            writeln!(output, "Invalid confirmation: (empty)").map_err(io_error)?;
        } else {
            writeln!(output, "Invalid confirmation: {trimmed}").map_err(io_error)?;
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
