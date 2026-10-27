use cli_command_kit::Mode;
use engine_cli::{CancellationToken, cli, execute};

#[tokio::test]
async fn native_shell_scripts_include_actual_interface_without_database_access() {
    std::fs::create_dir_all(".tmp").unwrap();
    for shell in ["bash", "fish", "powershell", "zsh"] {
        for no_descriptions in [false, true] {
            let mut args = vec!["engine-cli", "completion", shell];
            if no_descriptions {
                args.push("--no-descriptions");
            }
            let matches = cli::command(Mode::Agent)
                .try_get_matches_from(args)
                .unwrap();
            let mut output = Vec::new();
            execute(
                &matches,
                Mode::Agent,
                &mut output,
                &mut Vec::new(),
                &CancellationToken::new(),
            )
            .await
            .unwrap();
            let script = String::from_utf8(output).unwrap();
            assert!(script.contains("engine-cli"));
            assert!(script.contains("duration-unit"));
            assert!(script.contains("playlist"));
            assert!(script.contains("relocate"));
            if no_descriptions {
                assert!(
                    !script.contains("Skip the safety snapshot before mutations"),
                    "{shell}"
                );
            } else if shell != "bash" {
                assert!(
                    script.contains("Skip the safety snapshot before mutations"),
                    "{shell}"
                );
            }
            if matches!(shell, "bash" | "zsh") {
                let directory = tempfile::tempdir_in(".tmp").unwrap();
                let path = directory.path().join("completion");
                std::fs::write(&path, &script).unwrap();
                let checked = std::process::Command::new(shell)
                    .arg("-n")
                    .arg(path)
                    .output()
                    .unwrap();
                assert!(
                    checked.status.success(),
                    "{shell}: {}",
                    String::from_utf8_lossy(&checked.stderr)
                );
            }
        }
    }
}

#[tokio::test]
async fn completion_output_failures_are_returned() {
    struct Broken;
    impl std::io::Write for Broken {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "closed completion output",
            ))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    for shell in ["bash", "fish", "powershell", "zsh"] {
        let matches = cli::command(Mode::Agent)
            .try_get_matches_from(["engine-cli", "completion", shell])
            .unwrap();
        let error = execute(
            &matches,
            Mode::Agent,
            &mut Broken,
            &mut Vec::new(),
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(error, engine_cli::Error::Io(ref error) if error.kind() == std::io::ErrorKind::BrokenPipe)
        );
    }
}
