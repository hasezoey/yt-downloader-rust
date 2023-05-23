# Youtube-DL rust cli interface (ytdlr)

A CLI interface for `youtube-dl` (or `yt-dlp` available in PATH as `youtube-dl`) written in RUST.

Also contains some helper functions like [rethumbnailing](#rethumbnail).

## Requirements

- Linux / Mac - build with POSIX system paths in mind (Windows *might* work)
- youtube-dl or yt-dlp installed and be accessable via the command `youtube-dl`
- ffmpeg is installed and be accessable via the command `ffmpeg`
- rust stable 1.65 or higher

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

- `debugger` only works in a target with `debug_assertions` enabled.
- `verbosity` is counted by occurences in the command (like `-vv` equals `2`) or a number in the environment variable. (`0 - WARN`, `1 - INFO`, `2 - DEBUG`, `3 - TRACE`)
- `archive` is only used when a path is set.

### `download`

Command to download 1 or more URLS with youtube-dl / yt-dlp with extra archive support and edit functionality

Signature: `download [OPTIONS] [URLS]...`  
Aliases: `download`

| Positional Name | Short |          Long          |      Environment Variable      |          Default          |                             Type                             | Description                                                                                                                                      |
| :-------------: | :---: | :--------------------: | :----------------------------: | :-----------------------: | :----------------------------------------------------------: | :----------------------------------------------------------------------------------------------------------------------------------------------- |
|                 |  -h   |         --help         |                                |                           |                             flag                             | Print Help Information                                                                                                                           |
|                 |  -a   |      --audio-only      |                                |                           |                             flag                             | Set that the Output will only be audio-only (mp3)                                                                                                |
|                 |       |     --audio-editor     |       YTDL_AUDIO_EDITOR        |                           |                            OsStr                             | Audio Editor Command / Path to use (like `audacity`)                                                                                             |
|                 |       |     --video-editor     |       YTDL_VIDEO_EDITOR        |                           |                            OsStr                             | Video Editor Command / Path to use (like `kdenlive`)                                                                                             |
|                 |       |        --tagger        |          YTDL_TAGGER           |                           |                            OsStr                             | Tagger Command / Path to use (like `picard`)                                                                                                     |
|                 |       |    --editor-stdout     |                                |                           |                             flag                             | Enable Output of the Editor command stdout to be printed to the log                                                                              |
|                 |       |   --youtubedl-stdout   |                                |                           |                             flag                             | Enable Output of the youtube-dl command stdout to be printed to the log                                                                          |
|                 |       | --no-reapply-thumbnail | YTDL_DISABLE_REAPPLY_THUMBNAIL |           false           |                             bool                             | Disable re-applying the thumbnail after a editor has run                                                                                         |
|                 |  -o   |     --output-path      |            YTDL_OUT            | DownloadDir + `ytdlr-out` |                            OsStr                             | Output path to place all finished files in                                                                                                       |
|                 |       |     --archive-mode     |                                |         `default`         | Set which entries should be output to the youtube-dl archive |
|                 |       |  --no-check-recovery   |                                |                           |                             flag                             | Disables allowing 0 URL's to just check the recovery                                                                                             |
|                 |       |      open-tagger       |                                |                           |                             flag                             | Set to automatically open the tagger in the end. also overwrites the default option of moving for non-interactive mode                           |
|                 |       |      --sub-langs       |         YTDL_SUB_LANGS         |                           |                            string                            | Set which subtitles to download / embed, see [yt-dl(p) subtitle options](https://github.com/yt-dlp/yt-dlp#subtitle-options) for what is accepted |
|      URLS       |       |                        |                                |                           |                            string                            | The URLS (one or more) to be downloaded            (or 0 for error recovery)                                                                     |

Notes:

- This command will store all intermediate downloaded files (until moved) in the tempoarary path specified by [`--tmp`](#global-options).
- Files will not be moved to `output-path` when the Tagger option is chosen (enable "Move Files" in your Tagger).
- `*-stdout` flags enable stdout to be printed to the logs, but to view these `RUST_LOG` must at least be at `trace` (or `-vvv`).
- 0 URLs means to only check for recovery
- in non-interactive mode the default for finishing media is to move files (`m` in interactive mode), can be changed with `--open-tagger`
- if no "sub-langs" are specified, no subtitles will be downloaded and embedded
- the fist subtitle stream is set as "default"

### archive-mode

The download option `--archive-mode` sets which archive entries are output for the youtube-dl archive from the SQLite archive.  
This option does not have any effect when no archive is provided.  
This option does not affect which entries are added to the SQLite archive (only the generated youtube-dl archive)

Possible values are:

- `default`: Use the default Archive-Mode, currently corresponds to "all"
- `all`: Dump the full SQLite archive as a youtube-dl archive
- `byDate1000`: Output the newest 1000 media elements from the archive
- `none`: Dont add any entries from the SQLite archive to the youtube-dl archive

Note: none of the options affect the creation of a youtube-dl archive, only which entries are added before the youtube-dl command is run.

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

Notes:

- if no `--out` is specified, by default it will overwrite the input `--media` path

### `completions`

Command to generate shell completions.
Supported shells are all that [`clap_complete`](https://docs.rs/clap_complete/latest/clap_complete/shells/enum.Shell.html) support, which currently are (also lowercased):

- Bash
- Elvish
- Fish
- PowerShell
- Zsh

Signature: `completions --shell <SHELL_NAME> [--out <PATH>]`  
Aliases: `re-thumbnail`, `rethumbnail`

| Short |  Long   | Environment Variable | Default |  Type  | Description                       |
| :---: | :-----: | :------------------: | :-----: | :----: | :-------------------------------- |
|  -s   | --shell |                      |         | string | Shell to generate completions for |
|  -o   |  --out  |                      |         | OsStr  | Path to output the completions to |

Notes:

- if no output path (`--out`) is provided, it will be output to STDOUT

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

## Notes

This Project is mainly a personal project, so it is currently tailored to my use-cases, but issues / requests will still be reviewed.

## Project TODO

- [ ] add QOL command `archive search` to search through the archive by any column
- [ ] add ability to start a play (like mpv) before choosing to edit
- [ ] add ability to go back in the edit list
