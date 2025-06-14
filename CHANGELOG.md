# Changelog

This is a manually written changelog, and only tracks front-facing changes since version [`v0.5.0`](#v050)

## unreleased

## v0.12.0

- bump msrv to `1.85`
- remove ability to import old ytdlr JSON Archives. They were upgrade from since [`v0.5.0`](#v050), which released in January 2023.
- try releasing on crates.io

## v0.11.0

- `download`: add `opus`, `m4a` and `flac` as possible extension to be found during recovery
- `download`: add `matroska` & `flac` to the list of containers that use global metadata
- `download`: ignore files that have `.temp` before the extension (youtube-dl files before post-processing)
- `download`: ignore entries that are not a file or are size 0
- `download`: fix not parsing `LineType::ArchiveSkip` correctly
- `download`: reset progressbar again when actually moving file. (otherwise the time will count since the last reset, which was before asking what to do)
- `rethumbnail`: support ogg container (via lofty)
- `rethumbnail`: use lofty for mp3 rethumbnailing
- bump msrv to `1.75`
- add ability to choose what output container to use for audio (`--audio-format`) / video (`--video-format`), for rules see [yt-dlp Post-Processing Options `--remux-video`](- `rethumbnail`: support ogg container (via lofty))
- set default audio format to `best` (will use whatever is downloaded if possible or try to convert losslessly, otherwise `mp3`)

## v0.10.0

- `download`: enable steady-tick for the progress-bar (progressbar will get printed, even if "stuck")
- `download`: rename option `youtubedl-stdout` to `youtubedl-log`
- `download`: add option to save the youtube-dl log to file `youtubedl-logfile`
- `download`: (debug only): add subcommand `unicode-test` to debug string display widths
- `download`: decrease current counter on error
- `download`: add option `edit-action` to apply a single action to all media in edit-media stage
- `download`: some internal refactors on the state handling
- `download`: set default count estimate (1) if not set by the time the first download starts
- add feature `workaround_fe0f` (enabled by default) to workaround some terminals seeing `FE0F`(or previous emoji) as double-space (because `unicode-width` reports it is only 1 length)
- bump msrv to `1.70`
- disable feature `multithread` on `sysinfo` to not have unnecessary empty threads hanging around
- various dependency updates

## v0.9.0

- `download`: handle files with the same name at the end by automatically adding a `-X` (where `X` is a number up to 30)
- `download`: print yt-dlp error lines with `Debug` trait instead of `Display` so that escape-sequences from the line are escaped
- update various dependencies

## v0.8.0

- `download`: change editors to run with inherited STDIO
- `download`: add ability to play the current element in the edit stage
- `download`: add ability to go back elements in the edit stage
- `download`: change to use command `yt-dlp` instead of `youtube-dl`
- `download`: try to add entries from the recovery to the archive after move
- `download`: add option `--extra-ytdl-args` to provide extra youtube-dl arguments
- `download`: does not immediately error anymore when a "ERROR:" line in encountered (like a private video in a playlist)
- `download`: reflect skipped and errored media in the count
- `download`: add warning when used yt-dlp version is below the minimal recommended one
- ffmpeg: fix error on invalid utf8 sequence
- add backtraces to errors that do not panic (only when `RUST_BACKTRACE=true` is set)
- change Termination requests to not be based on time anymore
- some more internal refactoring
- build requirements section has been added
- requirements have been updated

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
