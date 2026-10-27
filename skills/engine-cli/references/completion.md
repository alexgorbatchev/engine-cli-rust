---
created_on: 2026-10-14 17:01
last_modified: 2026-10-14 17:01
status: current
---

## Shell completion

`engine-cli completion` prints help. Each command below writes the corresponding shell's completion script to stdout without accessing the collection:

- `engine-cli completion bash`
- `engine-cli completion fish`
- `engine-cli completion powershell`
- `engine-cli completion zsh`

Every shell command accepts `--no-descriptions`, a bool flag defaulting to `false`, which removes command and option descriptions from the generated script. Bash's generator already omits descriptions. No positional arguments are accepted. Global flags remain available. Completion output is a shell script in both human and `AGENT=1` modes; save or load it with that shell's native completion facilities.
