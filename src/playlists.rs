use crate::{Context, Result, output::Output};
use cli_command_kit::{Mode, clap::ArgMatches};
use engine_library::{
    Database, Error, Playlist, Track,
    collection::{self, PlaylistNode},
    model::DurationUnit,
};
use tokio_util::sync::CancellationToken;

fn playlist(db: &Database, query: &str) -> engine_library::Result<Playlist> {
    db.find_playlist(query)?
        .ok_or_else(|| Error::NotFound(format!("playlist {query:?}")))
}
fn track(db: &Database, query: &str) -> engine_library::Result<Track> {
    db.find_track(query)?
        .ok_or_else(|| Error::NotFound(format!("track {query:?}")))
}
fn parent(db: &Database, query: Option<&String>) -> engine_library::Result<i64> {
    match query {
        None => Ok(0),
        Some(query) if query == "0" || query.eq_ignore_ascii_case("Collection") => Ok(0),
        Some(query) => Ok(playlist(db, query)?.id),
    }
}
fn text<'a>(args: &'a ArgMatches, name: &str) -> &'a str {
    args.get_one::<String>(name).expect("required argument")
}
pub(super) fn run(
    context: &Context,
    path: &[&str],
    args: &ArgMatches,
    out: &mut Output<'_>,
    cancel: &CancellationToken,
) -> Result<()> {
    let db = context.database(matches!(path[0], "list" | "inspect"))?;
    match path[0] {
        "list" => {
            let nodes = collection::hierarchy(&db)?;
            if nodes.is_empty() {
                out.line(if out.mode == Mode::Agent {
                    "status: empty"
                } else {
                    "No playlists found in database."
                })?;
            } else {
                out.line(if out.mode == Mode::Agent {
                    "- Collection"
                } else {
                    "Collection"
                })?;
                print_nodes(&nodes, "", out)?;
            }
        }
        "inspect" => {
            let details = collection::inspect_playlist(&db, text(args, "playlist"), context.unit)?;
            if details.tracks.is_empty() {
                out.line(if out.mode == Mode::Agent {
                    "status: empty"
                } else {
                    "No tracks in playlist."
                })?;
            }
            for track in details.tracks {
                out.line(format!(
                    "{}{}",
                    if out.mode == Mode::Agent { "- " } else { "" },
                    track_line(&track, context.unit)
                ))?;
            }
        }
        "create" => {
            let list = db.create_playlist(
                text(args, "name"),
                parent(&db, args.get_one::<String>("parent"))?,
                args.get_flag("folder"),
            )?;
            out.fields(&[
                ("status", "ok".into()),
                ("id", list.id.to_string()),
                ("title", list.title),
                ("parent_id", list.parent_list_id.to_string()),
                ("is_folder", list.is_folder.unwrap_or(false).to_string()),
            ])?;
        }
        "rm" => {
            let list = playlist(&db, text(args, "playlist"))?;
            db.delete_playlist(list.id, args.get_flag("force"))?;
            out.fields(&[
                ("status", "ok".into()),
                ("deleted_id", list.id.to_string()),
                ("deleted_title", list.title),
            ])?;
        }
        "move" => {
            let list = playlist(&db, text(args, "playlist"))?;
            let parent = parent(&db, args.get_one::<String>("to"))?;
            db.move_playlist(list.id, parent)?;
            out.fields(&[
                ("status", "ok".into()),
                ("id", list.id.to_string()),
                ("title", list.title),
                ("parent_id", parent.to_string()),
            ])?;
        }
        "track" => {
            let moving = path[1] == "move";
            let source = playlist(&db, text(args, if moving { "source" } else { "playlist" }))?;
            let destination = if moving {
                Some(playlist(&db, text(args, "destination"))?)
            } else {
                None
            };
            for query in args.get_many::<String>("tracks").expect("tracks required") {
                if cancel.is_cancelled() {
                    return Err(Error::Cancelled.into());
                }
                let track = track(&db, query)?;
                if let Some(destination) = &destination {
                    db.move_membership(source.id, destination.id, track.id)?;
                    out.fields(&[
                        ("status", "ok".into()),
                        ("src_playlist_id", source.id.to_string()),
                        ("dst_playlist_id", destination.id.to_string()),
                        ("track_id", track.id.to_string()),
                    ])?;
                } else {
                    if path[1] == "add" {
                        db.add_membership(source.id, track.id)?;
                    } else {
                        db.remove_membership(source.id, track.id)?;
                    }
                    out.fields(&[
                        ("status", "ok".into()),
                        ("playlist_id", source.id.to_string()),
                        (
                            if path[1] == "add" {
                                "track_id"
                            } else {
                                "removed_track_id"
                            },
                            track.id.to_string(),
                        ),
                    ])?;
                }
            }
        }
        _ => unreachable!("playlist leaves"),
    }
    Ok(())
}
fn track_line(track: &Track, unit: DurationUnit) -> String {
    let title = track
        .title
        .as_deref()
        .filter(|title| !title.is_empty())
        .or(track.filename.as_deref())
        .unwrap_or("");
    let name = track
        .artist
        .as_deref()
        .filter(|artist| !artist.is_empty())
        .map_or_else(|| title.to_string(), |artist| format!("{artist} - {title}"));
    let seconds = unit.seconds(track.length.unwrap_or(0).max(0));
    format!(
        "{name} [id:{} {}:{:02}]",
        track.id,
        seconds / 60,
        seconds % 60
    )
}
fn print_nodes(nodes: &[PlaylistNode], prefix: &str, out: &mut Output<'_>) -> Result<()> {
    for (index, node) in nodes.iter().enumerate() {
        let last = index + 1 == nodes.len();
        let branch = if out.mode == Mode::Agent {
            "  - "
        } else if last {
            "└── "
        } else {
            "├── "
        };
        out.line(format!(
            "{prefix}{branch}{} [id:{} tracks:{}]",
            node.title, node.id, node.track_count
        ))?;
        let next = if out.mode == Mode::Agent {
            "  "
        } else if last {
            "    "
        } else {
            "│   "
        };
        print_nodes(&node.children, &format!("{prefix}{next}"), out)?;
    }
    Ok(())
}
