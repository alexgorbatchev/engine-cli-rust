use crate::{
    Context, Result,
    output::{Output, expand},
};
use cli_command_kit::{Mode, clap::ArgMatches};
use engine_library::{artwork, backup, collection, music};
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;

pub(super) async fn run(
    context: &Context,
    path: &[&str],
    args: &ArgMatches,
    out: &mut Output<'_>,
    cancel: &CancellationToken,
) -> Result<()> {
    match path[0] {
        "backup" => {
            let destination = args
                .get_one::<PathBuf>("path")
                .or_else(|| args.get_one::<PathBuf>("out-dir"))
                .map(|p| expand(p));
            if args.get_flag("list") {
                let directory = destination
                    .map(Ok)
                    .unwrap_or_else(backup::default_directory)?;
                let backups = backup::list(&directory)?;
                out.fields(&[("count", backups.len().to_string())])?;
                for info in backups {
                    out.line(format!(
                        "backup: {} | timestamp: {} | size_bytes: {}",
                        cli_command_kit::display_path(&info.path),
                        info.timestamp.to_rfc3339(),
                        info.size_bytes
                    ))?;
                }
            } else {
                let info = backup::create(&context.library, destination.as_deref(), cancel)?;
                out.fields(&[
                    ("status", "success".into()),
                    ("path", cli_command_kit::display_path(&info.path)),
                    ("size_bytes", info.size_bytes.to_string()),
                    ("db_files", info.db_files.to_string()),
                    ("art_files", info.art_files.to_string()),
                ])?;
            }
        }
        "restore" => {
            let source = expand(args.get_one::<PathBuf>("path").expect("path required"));
            backup::restore(&source, &context.library, cancel)?;
            out.fields(&[
                ("status", "success".into()),
                ("restored_from", cli_command_kit::display_path(&source)),
                (
                    "target_dir",
                    cli_command_kit::display_path(&context.library),
                ),
            ])?;
        }
        "verify" => {
            let db = context.database(true)?;
            let report = collection::verify(&db, &context.library, cancel)?;
            if args.get_flag("json") {
                // Display paths in JSON as well; stored paths and filesystem calls remain unchanged.
                let mut report = report;
                for issue in &mut report.issues {
                    issue.path = issue.path.take().map(|p| crate::output::fold_text(&p));
                    issue.description = crate::output::fold_text(&issue.description);
                }
                out.line(serde_json::to_string_pretty(&report)?)?;
            } else {
                let status = if report.is_healthy {
                    "healthy"
                } else {
                    "issues_detected"
                };
                if out.mode == Mode::Agent {
                    out.fields(&[
                        ("total_tracks", report.total_tracks.to_string()),
                        ("missing_files", report.missing_on_disk_count.to_string()),
                        ("corrupt_sizes", report.bytes_mismatch_count.to_string()),
                        ("broken_entities", report.orphaned_count.to_string()),
                        ("missing_artwork", report.missing_art_count.to_string()),
                        ("empty_playlists", report.empty_playlist_count.to_string()),
                        ("status", status.into()),
                    ])?;
                } else {
                    out.line("Library Verification Report")?;
                    out.line(format!(
                        "Missing Audio Files: {}",
                        report.missing_on_disk_count
                    ))?;
                    out.fields(&[
                        ("Total Tracks Audited", report.total_tracks.to_string()),
                        ("Size Mismatches", report.bytes_mismatch_count.to_string()),
                        ("Broken Playlist Links", report.orphaned_count.to_string()),
                        ("Missing Album Covers", report.missing_art_count.to_string()),
                        ("Empty Playlists", report.empty_playlist_count.to_string()),
                        ("Overall Health Status", status.into()),
                    ])?;
                    for issue in report.issues {
                        out.line(issue.description)?;
                    }
                }
            }
        }
        "sync" => {
            let db = context.database(false)?;
            let report =
                music::synchronize(&db, &context.library, context.unit, &context.tools, cancel)
                    .await?;
            out.fields(&[
                ("status", "complete".into()),
                ("total_tracks", report.total_db_tracks.to_string()),
                ("updated_records", report.updated_count.to_string()),
                ("missing_files", report.missing_count.to_string()),
                ("unindexed_files", report.unindexed_count.to_string()),
            ])?;
            for path in report.unindexed_paths {
                out.fields(&[("unindexed", path)])?;
            }
            for warning in report.warnings {
                out.warning(&warning)?;
            }
        }
        "prune" => {
            let db = context.database(false)?;
            let report = artwork::prune(&db, &context.library.join("Artwork"), cancel)?;
            out.fields(&[
                ("status", "complete".into()),
                ("deleted_db_rows", report.deleted_db_rows.to_string()),
                ("deleted_disk_files", report.deleted_disk_files.to_string()),
            ])?;
        }
        _ => unreachable!("library leaves"),
    }
    Ok(())
}
