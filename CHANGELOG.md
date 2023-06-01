# Changelog

This is a manually written changelog, and only tracks front-facing changes since version [`v0.5.0`](#v050)

## next

- `download`: change editors to run with inherited STDIO
- `download`: add ability to play the current element in the edit stage
- `download`: add ability to go back elements in the edit stage
- ffmpeg: fix error on invalid utf8 sequence
- change Termination requests to not be based on time anymore
- some more internal refactoring

## v0.7.0

- `download`: truncate filenames to be below 255 bytes (most filesystems only support filenames up to 255 bytes)
- `completions`: add command to generate shell completions (bash, zsh, elvish, fish, powershell)
- `archive search`: add command to search the archive (if present) for the given queries
- `archive import`: actually support importing from SQLite archive (previous was a `todo!`)
- fix compile for windows
- some internal refactoring
- update various dependencies
- remove unused dependencies

## v0.6.0

- `rethumbnail`: better handle `mp4` for rethumbnailing
- `rethumbnail`:add special case for rethumbnailing `mkv` files (because attachments get changed to video streams instead of attachments)
- `download`: only re-write metadata if a editor has been run
- `download`: only write recovery file if there are elements to be written
- `download`: find and remove old youtube-dl archives where the pid's are not alive anymore
- `download`: handle `youtube-dl: error:` lines
- `download`: add youtube-dl commandline option `--convert-thumbnails` (`webp>jpg`)
- `download`: add option to embedd subtitles if available via `--sub-langs` (or env `YTDL_SUB_LANGS`), see [yt-dlp `--sub-langs`](https://github.com/yt-dlp/yt-dlp#subtitle-options) on how to define languages to add
- `download`: fix possible replace of invalid character boundary for truncation
- `download`: add printing of how many urls have been done and how many there are
- `download`: add printing of which url has been started
- `download`: add info of playlist count instead of always `??`
- `download`: reset download information on url change (to match playlist count)
- `download`: consolidate all `--force*archive*` arguments into `--archive-mode`
- update various dependencies

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
