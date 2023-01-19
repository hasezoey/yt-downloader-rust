# YT-Downloader RUST

## Requirements

- Linux / Mac - build with POSIX system paths in mind (Windows *might* work)
- youtube-dl or yt-dlp installed and be accessable via the command `youtube-dl`
- ffmpeg is installed and be accessable via the command `ffmpeg`
- rust stable 1.58 or higher

## Usage

### Global Options

Signature: `ytdlr [OPTIONS] <SUBCOMMAND>`  

(Options for main command, must be set before the subcommands)

| Short |    Long     | Environment Variable |         Default          |        Type         | Description                                                   |
| :---: | :---------: | :------------------: | :----------------------: | :-----------------: | :------------------------------------------------------------ |
|  -h   |   --help    |                      |                          |        flag         | Print Help Information                                        |
|       |  --archive  |     YTDL_ARCHIVE     |                          |        OsStr        | The Archive Path to use for a Archive                         |
|       |   --color   |                      |                          |        flag         | Enable Color Output (Currently unused)                        |
|       | --debugger  |                      |                          |        flag         | Request a VSCode CodeLLDB Debugger before continuing          |
|       |    --tmp    |       YTDL_TMP       | tmpdir + `ytdl_rust_tmp` |        OsStr        | The Temporary Directory to use for storing intermediate Files |
|  -v   | --verbosity |    YTDL_VERBOSITY    |            0             | occurences / number | Set the logging verbosity (same as `RUST_LOG`)                |
|  -V   |  --version  |                      |                          |        flag         | Print the Version                                             |

Notes:

- Currently `color` is unused.
- `debugger` only works in a target with `debug_assertions` enabled.
- `verbosity` is counted by occurences in the command (like `-vv` equals `2`) or a number in the environment variable.
- `archive` is only used when a path is set.

### `download`

Command to download 1 or more URLS with youtube-dl / yt-dlp with extra archive support and edit functionality

Signature: `download [OPTIONS] [URLS]...`  
Aliases: `download`

| Positional Name | Short |            Long            |      Environment Variable      |          Default          |  Type  | Description                                                                                                            |
| :-------------: | :---: | :------------------------: | :----------------------------: | :-----------------------: | :----: | :--------------------------------------------------------------------------------------------------------------------- |
|                 |  -h   |           --help           |                                |                           |  flag  | Print Help Information                                                                                                 |
|                 |  -a   |        --audio-only        |                                |                           |  flag  | Set that the Output will only be audio-only (mp3)                                                                      |
|                 |       |       --audio-editor       |       YTDL_AUDIO_EDITOR        |                           | OsStr  | Audio Editor Command / Path to Command                                                                                 |
|                 |       |       --video-editor       |       YTDL_VIDEO_EDITOR        |                           | OsStr  | Video Editor Command / Path to Command                                                                                 |
|                 |       |          --picard          |          YTDL_PICARD           |                           | OsStr  | Picard Command / Path to Command                                                                                       |
|                 |       |      --editor-stdout       |                                |                           |  flag  | Enable Output of the Editor command stdout to be printed to the log                                                    |
|                 |       |     --youtubedl-stdout     |                                |                           |  flag  | Enable Output of the youtube-dl command stdout to be printed to the log                                                |
|                 |       |   --no-reapply-thumbnail   | YTDL_DISABLE_REAPPLY_THUMBNAIL |           false           |  bool  | Disable re-applying the thumbnail after a editor has run                                                               |
|                 |  -o   |       --output-path        |            YTDL_OUT            | DownloadDir + `ytdlr-out` | OsStr  | Output path to place all finished files in                                                                             |
|                 |       |   --force-genarchive-all   |                                |                           |  flag  | Force the archive to be completely dumped in the youtube-dl archive                                                    |
|                 |       | --force-genarchive-by-date |                                |                           |  flag  | Force the archive to use the by-date generation for the youtube-dl archive                                             |
|                 |       |     --force-no-archive     |                                |                           |  flag  | Force to not use any ytdl archive (include all entries), but still add media to ytdlr archive (if not exist already)   |
|                 |       |    --no-check-recovery     |                                |                           |  flag  | Disables allowing 0 URL's to just check the recovery                                                                   |
|                 |       |        open-tagger         |                                |                           |  flag  | Set to automatically open the tagger in the end. also overwrites the default option of moving for non-interactive mode |
|      URLS       |       |                            |                                |                           | string | The URLS (one or more) to be downloaded                                                                                |

Notes:

- This command will store all intermediate downloaded files (until moved) in [`tmp`](#global-options).
- If `force-genarchive-all` or others are set, `force-genarchive-all` will take priority.
- Files will not be moved to `output-path` when Picard is chosen (enable "Move Files" in Picard).
- `*-stdout` flags enable stdout to be printed to the logs, but to view these `RUST_LOG` must at least be at `trace`.
- 0 URLs means to only check for recovery
- in non-interactive mode the default for finishing media is to move files (`m` in interactive mode), can be changed with `--open-tagger`

### `rethumbnail`

Command to re-apply a image onto a media file as a thumbnail  
Input images that are not JPG will be transformed into JPG (most thumbnail-able formats only accept jpg)

Signature: `re-thumbnail [OPTIONS] --image <INPUT_IMAGE_PATH> --media <INPUT_MEDIA_PATH>`  
Aliases: `re-thumbnail`, `rethumbnail`

| Short |  Long   | Environment Variable |      Default      | Type  | Description            |
| :---: | :-----: | :------------------: | :---------------: | :---: | :--------------------- |
|  -h   | --help  |                      |                   | flag  | Print Help Information |
|  -i   | --image |                      |                   | OsStr | Input Image File       |
|  -m   | --media |                      |                   | OsStr | Input Media File       |
|  -o   |  --out  |                      | Same as `--media` | OsStr | Output Media File      |

### `archive import`

Command to import a archive into the currently set one  
Will Error if [Archive Path](#global-options) is unset

Signature: `archive import <FILE_PATH>`  
Aliases: `import`

| Positional Name | Short |  Long  | Environment Variable | Default | Type  | Description            |
| :-------------: | :---: | :----: | :------------------: | :-----: | :---: | :--------------------- |
|                 |  -h   | --help |                      |         | flag  | Print Help Information |
|    FILE_PATH    |       |        |                      |         | OsStr | File to Import         |

Currently supported formats that can be imported:

- JSON Archive (from previous versions)
- youtube-dl (provider, id) Archive
- SQLite Archive

## Extra

This Project is mainly a personal project, so it is currently tailored to my use-cases, but issues / requests will still be reviewed.

## Project TODO

- [ ] Rework 2022
  - [x] Move Archive to SQL (SQLite by default) instead of big json
    - [x] re-implement `archive import` for sql
    - [x] re-implement main download for sql
  - [x] re-implement main download
  - [x] add QOL command `re-thumbnail`
  - [ ] add QOL command `archive search` to search through the archive by any column
  - [ ] add QOL command `completions` to generate shell completions (bash, zsh, etc)
  - [x] re-implement `archive import`
  - ~~[ ] add command `archive migrate` (to check and migrate the archive to new versions)~~
  - [x] completely seperate `libytdlr(lib)` and `ytdlr(bin)`
  - [x] make binary interface more pleasant
  - [ ] add ability to start a play (like mpv) before choosing to edit
- [x] Picard Integration (generic)
- ~~[ ] Sponsorblock integration via yt-dlp?~~
