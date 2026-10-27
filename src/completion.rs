use crate::Result;
use clap_complete::aot::{Generator, Shell};
use cli_command_kit::{Mode, clap::Command};
use std::io::Write;

pub(super) fn write(shell: &str, no_descriptions: bool, output: &mut dyn Write) -> Result<()> {
    let generator = match shell {
        "bash" => Shell::Bash,
        "fish" => Shell::Fish,
        "powershell" => Shell::PowerShell,
        "zsh" => Shell::Zsh,
        _ => unreachable!("parsed completion shell"),
    };
    let mut command = crate::cli::command(Mode::Human);
    if no_descriptions {
        remove_descriptions(&mut command);
    }
    command.set_bin_name("engine-cli");
    command.build();
    // Fish's generator still expects successful writes internally in 4.6.11.
    // Generate into memory, then propagate errors from the caller's actual sink.
    let mut script = Vec::new();
    generator.try_generate(&command, &mut script)?;
    output.write_all(&script)?;
    Ok(())
}

fn remove_descriptions(command: &mut Command) {
    *command = command
        .clone()
        .about("")
        .long_about("")
        .mut_args(|arg| arg.help("").long_help(""));
    for child in command.get_subcommands_mut() {
        remove_descriptions(child);
    }
}
