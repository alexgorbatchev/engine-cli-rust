use crate::{
    Context, Result,
    output::{Output, expand},
};
use cli_command_kit::clap::ArgMatches;
use engine_library::{Error, artwork, music};
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;

pub(super) async fn run(
    context: &Context,
    path: &[&str],
    args: &ArgMatches,
    out: &mut Output<'_>,
    cancel: &CancellationToken,
) -> Result<()> {
    let db = context.database(false)?;
    match path[0] {
        "add" => {
            let options = music::ImportOptions {
                playlist: args.get_one::<String>("to-playlist").cloned(),
                move_file: args.get_flag("move"),
                duration_unit: context.unit,
            };
            for file in args.get_many::<PathBuf>("paths").expect("paths required") {
                let result = music::import(
                    &db,
                    &context.library,
                    &expand(file),
                    &options,
                    &context.tools,
                    cancel,
                )
                .await?;
                let track = result.track;
                out.fields(&[
                    ("status", "ok".into()),
                    ("track_id", track.id.to_string()),
                    ("title", title(&track)),
                    ("path", track.path.unwrap_or_default()),
                    ("moved_file", result.moved.to_string()),
                ])?;
                if let Some(list) = result.playlist {
                    out.fields(&[
                        ("playlist_id", list.id.to_string()),
                        ("playlist", list.title),
                    ])?;
                }
                if let Some(warning) = result.probe_warning {
                    out.warning(&warning)?;
                }
            }
        }
        "rm" => {
            let delete = args.get_flag("delete-file");
            for query in args.get_many::<String>("tracks").expect("tracks required") {
                let track = music::remove(&db, &context.library, query, delete, cancel)?;
                out.fields(&[
                    ("status", "ok".into()),
                    ("deleted_track_id", track.id.to_string()),
                    ("title", title(&track)),
                    ("deleted_file", delete.to_string()),
                ])?;
            }
        }
        "move" => {
            let track = music::move_file(
                &db,
                &context.library,
                args.get_one::<String>("track").expect("track required"),
                &expand(
                    args.get_one::<PathBuf>("destination")
                        .expect("destination required"),
                ),
                cancel,
            )?;
            out.fields(&[
                ("status", "ok".into()),
                ("track_id", track.id.to_string()),
                ("title", title(&track)),
                ("new_path", track.path.unwrap_or_default()),
            ])?;
        }
        "relocate" => {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled.into());
            }
            let from = args.get_one::<String>("from").expect("from required");
            let to = args.get_one::<String>("to").expect("to required");
            let count = db.relocate_tracks(from, to)?;
            out.fields(&[
                ("status", "ok".into()),
                ("relocated_count", count.to_string()),
                ("from_prefix", from.clone()),
                ("to_prefix", to.clone()),
            ])?;
        }
        "artwork" => {
            let report = artwork::normalize(
                &db,
                &context.library,
                &context.library.join("Artwork"),
                cancel,
            )?;
            out.fields(&[
                ("status", "complete".into()),
                ("total_tracks", report.total_tracks.to_string()),
                ("normalized", report.normalized_count.to_string()),
                ("already_square", report.already_square.to_string()),
                ("no_art", report.no_art_count.to_string()),
                ("errors", report.error_count.to_string()),
                (
                    "cache_normalized",
                    report.cache_normalized_count.to_string(),
                ),
            ])?;
            for warning in report.warnings {
                out.warning(&warning)?;
            }
        }
        _ => unreachable!("track leaves"),
    }
    Ok(())
}
fn title(track: &engine_library::Track) -> String {
    track
        .title
        .as_ref()
        .filter(|s| !s.is_empty())
        .or(track.filename.as_ref())
        .cloned()
        .unwrap_or_default()
}
