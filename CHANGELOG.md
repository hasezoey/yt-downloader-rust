# Changelog

This is a manually written changelog, and only tracks front-facing changes since version [`v0.5.0`](#v050)

## v0.5.0

- add a `LICENSE` file
- completely seperate the project into a library (`libytdlr`) and a binary (`ytdlr`)
- move from json archive to be a sqlite archive (migration & imports available)
- add a way to wait for the vscode debugger (only in debug target (`debug_assertions`))
- update to clap 4
- add non-tty way of usage (automated)
- no panic exits for normal errors, instead directly printing the errors
- add recovery file and recover from no recovery file
- add ctrlc (plus some other signals) handler
- update logger to be better than 0.4.0
- update indicatif to 0.17 (which fixes lines disapperaing because of the progressbar, like logs)
- test that ytdl(-p) and ffmpeg are installed
- adjust progressbar text based on terminal size (minimal recommended width is 50)
- add option to run in various archive modes (`--force-genarchive-by-date`, `--force-genarchive-all`, `--force-no-archive`)
- add a way to start a tagger directly after editing files
- add more information to `--version`
