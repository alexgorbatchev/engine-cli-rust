pub mod cli;
mod completion;
mod library;
pub mod output;
mod playlists;
mod tracks;
use cli_command_kit::{Mode, clap::ArgMatches};
use engine_library::{Database, backup, model::DurationUnit};
use std::{io::Write, path::PathBuf};
pub use tokio_util::sync::CancellationToken;
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Library(#[from] engine_library::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
pub fn exit_status(error: &Error) -> i32 {
    match error {
        Error::Library(engine_library::Error::Cancelled) => 130,
        Error::Library(
            engine_library::Error::PartialRestore { .. }
            | engine_library::Error::CommittedCleanup { .. },
        ) => 6,
        _ => 1,
    }
}
struct Context {
    library: PathBuf,
    unit: DurationUnit,
    tools: audio_metadata::process::Tools,
}
impl Context {
    fn database(&self, readonly: bool) -> Result<Database> {
        Ok(Database::open(
            self.library.join("Database2/m.db"),
            readonly,
        )?)
    }
}
pub async fn execute(
    matches: &ArgMatches,
    mode: Mode,
    mut stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    cancel: &CancellationToken,
) -> Result<()> {
    if matches.get_flag("version") {
        stdout.write_all(concat!(env!("CARGO_PKG_VERSION"), "\n").as_bytes())?;
        return Ok(());
    }
    if cli_command_kit::write_skill(matches, &cli::help_options(), &mut stdout)? {
        return Ok(());
    }
    let mut path = Vec::new();
    let mut leaf = matches;
    while let Some((name, args)) = leaf.subcommand() {
        path.push(name);
        leaf = args;
    }
    if path.first() == Some(&"completion") && path.len() == 2 {
        return completion::write(path[1], leaf.get_flag("no-descriptions"), stdout);
    }
    let mut command = cli::command(mode);
    let mut selected = &mut command;
    for name in &path {
        selected = selected.find_subcommand_mut(name).expect("parsed command");
    }
    if selected.has_subcommands() || path.is_empty() {
        selected.write_help(&mut stdout)?;
        writeln!(stdout)?;
        return Ok(());
    }
    if cancel.is_cancelled() {
        return Err(engine_library::Error::Cancelled.into());
    }
    let library = output::expand(
        matches
            .get_one::<PathBuf>("lib-dir")
            .expect("library default"),
    );
    let unit = if matches
        .get_one::<String>("duration-unit")
        .expect("unit default")
        == "seconds"
    {
        DurationUnit::Seconds
    } else {
        DurationUnit::Milliseconds
    };
    let context = Context {
        library,
        unit,
        tools: audio_metadata::process::Tools::new(
            std::env::var_os("PATH").unwrap_or_default(),
            std::env::current_dir()?,
        ),
    };
    let mut output = output::Output {
        mode,
        out: stdout,
        diagnostics: stderr,
    };
    let readonly = matches!(
        path.as_slice(),
        ["library", "verify" | "backup"] | ["playlist", "list" | "inspect"]
    );
    if !readonly
        && let Some(info) =
            backup::automatic(&context.library, matches.get_flag("no-backup"), cancel)?
    {
        output.fields(&[(
            if path == ["library", "restore"] {
                "safety_backup"
            } else {
                "backup_snapshot"
            },
            cli_command_kit::display_path(&info.path),
        )])?;
    }
    match path[0] {
        "library" => library::run(&context, &path[1..], leaf, &mut output, cancel).await,
        "playlist" => playlists::run(&context, &path[1..], leaf, &mut output, cancel),
        "track" => tracks::run(&context, &path[1..], leaf, &mut output, cancel).await,
        _ => unreachable!("clap domain commands"),
    }
}
