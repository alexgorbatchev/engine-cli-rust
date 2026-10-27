use cli_command_kit::Mode;

#[test]
fn skill_tracks_every_live_command_and_option_and_all_help_paths_are_alerted() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let skill = std::fs::read_to_string(root.join("skills/engine-cli/SKILL.md")).unwrap();
    assert!(skill.contains("name: engine-cli\n"));
    let mut text = skill;
    for topic in [
        "setup",
        "library",
        "playlists",
        "tracks",
        "completion",
        "help",
    ] {
        text.push_str(
            &std::fs::read_to_string(root.join(format!("skills/engine-cli/references/{topic}.md")))
                .unwrap(),
        );
    }
    let mut command = engine_cli::cli::command(Mode::Agent);
    let missing =
        cli_command_kit::skill_omissions(&command, &engine_cli::cli::help_options(), &text);
    assert!(missing.is_empty(), "{missing:?}");
    command.build();
    fn visit(command: &cli_command_kit::clap::Command, path: Vec<String>) {
        let mut args = path.clone();
        if !path.iter().any(|segment| segment == "help") {
            args.push("--help".into());
        }
        let result = engine_cli::cli::command(Mode::Agent)
            .try_get_matches_from(args)
            .unwrap_err();
        assert!(
            result
                .to_string()
                .starts_with("ALERT: Agents must read `AGENT=1 engine-cli skill`"),
            "{path:?}: {result}"
        );
        for child in command.get_subcommands() {
            let mut next = path.clone();
            next.push(child.get_name().into());
            visit(child, next);
        }
    }
    visit(&command, vec!["engine-cli".into()]);
}
