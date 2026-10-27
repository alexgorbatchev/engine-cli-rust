use cli_command_kit::{
    HelpOptions, Mode,
    clap::{Arg, ArgAction, Command, builder::NonEmptyStringValueParser},
};
use std::path::PathBuf;

pub const SKILL_POINTER: &str = "Install the engine-cli skill using the Skills CLI:\n\n  npx skills add alexgorbatchev/engine-cli --skill engine-cli\n\nSkill:\n\n  https://github.com/alexgorbatchev/engine-cli/tree/main/skills/engine-cli\n";
pub fn help_options() -> HelpOptions {
    HelpOptions {
        skill: Some(SKILL_POINTER.into()),
        tree: cli_command_kit::TreeOptions {
            terminal_width: Some(match cli_command_kit::terminal_width() {
                0 => 80,
                width => width,
            }),
            ..Default::default()
        },
        ..Default::default()
    }
}
fn flag(name: &'static str, help: &'static str) -> Arg {
    Arg::new(name)
        .long(name)
        .help(help)
        .action(ArgAction::SetTrue)
}
fn text(name: &'static str, help: &'static str) -> Arg {
    Arg::new(name)
        .help(help)
        .value_parser(NonEmptyStringValueParser::new())
}
fn path(name: &'static str, help: &'static str) -> Arg {
    Arg::new(name)
        .help(help)
        .value_parser(cli_command_kit::clap::value_parser!(PathBuf))
}
fn group(name: &'static str, help: &'static str) -> Command {
    Command::new(name).about(help)
}
pub fn command(mode: Mode) -> Command {
    let library = group("library", "Audit, synchronize and back up your collection")
        .visible_aliases(["lib", "db"])
        .subcommand(
            group("verify", "Check files, artwork and playlist links")
                .arg(flag("json", "Print the complete report as JSON")),
        )
        .subcommand(group("sync", "Update collection tags from audio files"))
        .subcommand(
            group("backup", "Create or list collection snapshots")
                .arg(path("path", "Snapshot destination directory"))
                .arg(flag("list", "List snapshots without creating one"))
                .arg(
                    path(
                        "out-dir",
                        "Snapshot destination; defaults to the user data directory",
                    )
                    .long("out-dir")
                    .short('o'),
                ),
        )
        .subcommand(
            group("restore", "Restore a snapshot over the collection")
                .arg(path("path", "Snapshot directory").required(true)),
        )
        .subcommand(group(
            "prune",
            "Remove unreferenced artwork records and cache files",
        ));
    let membership = group("track", "Manage playlist track memberships")
        .subcommand(
            group("add", "Add tracks to a playlist")
                .arg(text("playlist", "Playlist name or ID").required(true))
                .arg(
                    text("tracks", "Track titles, IDs or paths")
                        .required(true)
                        .num_args(1..),
                ),
        )
        .subcommand(
            group("rm", "Remove tracks from a playlist")
                .arg(text("playlist", "Playlist name or ID").required(true))
                .arg(
                    text("tracks", "Track titles, IDs or paths")
                        .required(true)
                        .num_args(1..),
                ),
        )
        .subcommand(
            group("move", "Transfer tracks between playlists")
                .arg(text("source", "Source playlist name or ID").required(true))
                .arg(text("destination", "Destination playlist name or ID").required(true))
                .arg(
                    text("tracks", "Track titles, IDs or paths")
                        .required(true)
                        .num_args(1..),
                ),
        );
    let playlist = group("playlist", "Inspect and manage playlists and folders")
        .subcommand(group("list", "List playlists and folders"))
        .subcommand(
            group("inspect", "Show a playlist's tracks and durations")
                .arg(text("playlist", "Playlist name or ID").required(true)),
        )
        .subcommand(
            group("create", "Create a playlist or folder")
                .arg(text("name", "Playlist title").required(true))
                .arg(
                    text("parent", "Parent name or ID; defaults to Collection")
                        .long("parent")
                        .short('p'),
                )
                .arg(flag("folder", "Create a folder container")),
        )
        .subcommand(
            group("rm", "Delete a playlist or folder")
                .arg(text("playlist", "Playlist name or ID").required(true))
                .arg(flag("force", "Recursively delete child playlists").short('f')),
        )
        .subcommand(
            group("move", "Move a playlist under a different parent")
                .arg(text("playlist", "Playlist name or ID").required(true))
                .arg(
                    text("to", "Parent name, ID, or Collection")
                        .long("to")
                        .required(true),
                ),
        )
        .subcommand(membership);
    let track = group("track", "Manage collection tracks and audio files")
        .subcommand(
            group("add", "Import audio files and their metadata")
                .arg(
                    path("paths", "Audio file paths")
                        .required(true)
                        .num_args(1..),
                )
                .arg(
                    text(
                        "to-playlist",
                        "Assign to this playlist, creating it if absent",
                    )
                    .long("to-playlist"),
                )
                .arg(flag(
                    "move",
                    "Move files into the collection's Music directory",
                )),
        )
        .subcommand(
            group("rm", "Remove tracks from the collection and playlists")
                .arg(
                    text("tracks", "Track titles, IDs or paths")
                        .required(true)
                        .num_args(1..),
                )
                .arg(flag("delete-file", "Also delete the physical audio files")),
        )
        .subcommand(
            group("move", "Move an audio file and update its collection path")
                .arg(text("track", "Track title, ID or path").required(true))
                .arg(path("destination", "Destination file or directory").required(true)),
        )
        .subcommand(
            group("relocate", "Replace directory prefixes in stored paths")
                .arg(
                    text("from", "Old directory prefix")
                        .long("from")
                        .required(true),
                )
                .arg(text("to", "New directory prefix").long("to").required(true)),
        )
        .subcommand(
            group("artwork", "Manage embedded cover artwork").subcommand(group(
                "normalize",
                "Crop artwork to square covers without changing audio",
            )),
        );
    let root = group(
        "engine-cli",
        "Inspect, repair and back up Engine DJ collections",
    )
    .disable_version_flag(true)
    .version(env!("CARGO_PKG_VERSION"))
    .arg(flag("version", "Print the raw version"))
    .arg(
        path("lib-dir", "Engine Library directory")
            .long("lib-dir")
            .short('l')
            .default_value("~/Music/Engine Library")
            .global(true),
    )
    .arg(flag("no-backup", "Skip the safety snapshot before mutations").global(true))
    .arg(
        Arg::new("duration-unit")
            .long("duration-unit")
            .value_parser(["milliseconds", "seconds"])
            .default_value("milliseconds")
            .help("Stored duration units; use seconds only for a verified collection")
            .global(true),
    )
    .subcommand(library)
    .subcommand(playlist)
    .subcommand(track)
    .subcommand(["bash", "fish", "powershell", "zsh"].into_iter().fold(
        group("completion", "Generate shell completion scripts"),
        |command, shell| {
            command.subcommand(
                group(shell, "Print this shell's completion script")
                    .arg(flag("no-descriptions", "Omit completion descriptions")),
            )
        },
    ));
    cli_command_kit::setup(root, mode, &help_options()).expect("valid Engine command tree")
}
