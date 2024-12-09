# Youtube-DL rust cli interface (ytdlr)

A CLI interface for `youtube-dl` (or `yt-dlp` available in PATH as `youtube-dl`) written in RUST.

Also contains some helper functions like [rethumbnailing](#rethumbnail).

## Requirements

- Linux / Mac - build with POSIX system paths in mind (Windows *might* work)
- [yt-dlp](https://github.com/yt-dlp/yt-dlp) above `2023.03.03`*1 and be accessable via the command `yt-dlp`
- ffmpeg is installed and be accessable via the command `ffmpeg`
- `libsqlite3-0`(ubuntu) or `core/sqlite`(arch) needs to be present

Notes:
- *1 it is recommended to use the latest version available for `yt-dlp`

### Building requirements

- rust stable 1.70 profile `minimal` or higher is needed
- `build-essentail`(ubuntu) or `base-devel`(arch) needs to be installed
- `libsqlite3-dev`(ubuntu) or `core/sqlite`(arch) needs to be installed
- `git` needs to be available (required by build-script)

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

| Positional Name | Short |          Long          |      Environment Variable      |          Default          |  Type  | Description                                                                                                                                      |
| :-------------: | :---: | :--------------------: | :----------------------------: | :-----------------------: | :----: | :----------------------------------------------------------------------------------------------------------------------------------------------- |
|                 |  -h   |         --help         |                                |                           |  flag  | Print Help Information                                                                                                                           |
|                 |  -a   |      --audio-only      |                                |                           |  flag  | Set that the Output will only be audio-only (mp3)                                                                                                |
|                 |       |     --audio-editor     |       YTDL_AUDIO_EDITOR        |                           | OsStr  | Audio Editor Command / Path to use (like `audacity`)                                                                                             |
|                 |       |     --video-editor     |       YTDL_VIDEO_EDITOR        |                           | OsStr  | Video Editor Command / Path to use (like `kdenlive`)                                                                                             |
|                 |       |        --tagger        |          YTDL_TAGGER           |                           | OsStr  | Tagger Command / Path to use (like `picard`)                                                                                                     |
|                 |       |        --player        |          YTDL_PLAYER           |                           | OsStr  | Media Player Command / Path to use (like `mpv`)                                                                                                  |
|                 |       |    --youtubedl-log     |                                |                           |  flag  | Enable Output of the youtube-dl command stdout to be printed to the log                                                                          |
|                 |       |  --youtubedl-logfile   |                                |                           |  flag  | Save Youtube-DL logs to a file. File will be in the temporary directory, named "yt-dl_PID.log" where the PID is the ytdlr's pid                  |
|                 |       | --no-reapply-thumbnail | YTDL_DISABLE_REAPPLY_THUMBNAIL |           false           |  bool  | Disable re-applying the thumbnail after a editor has run                                                                                         |
|                 |  -o   |     --output-path      |            YTDL_OUT            | DownloadDir + `ytdlr-out` | OsStr  | Output path to place all finished files in                                                                                                       |
|                 |       |     --archive-mode     |                                |         `default`         |  enum  | Set which entries should be output to the youtube-dl archive                                                                                     |
|                 |       |  --no-check-recovery   |                                |                           |  flag  | Disables allowing 0 URL's to just check the recovery                                                                                             |
|                 |       |     --open-tagger      |                                |                           |  flag  | Set to automatically open the tagger in the end. also overwrites the default option of moving for non-interactive mode                           |
|                 |       |     --edit-action      |                                |                           |  enum  | Apply a single action to all media in the edit stage                                                                                             |
|                 |       |      --sub-langs       |         YTDL_SUB_LANGS         |                           | String | Set which subtitles to download / embed, see [yt-dl(p) subtitle options](https://github.com/yt-dlp/yt-dlp#subtitle-options) for what is accepted |
|                 |       |     --video-format     |                                |           `mkv`           | String | Set the output video container remux rules                                                                                                       |
|                 |       |     --audio-format     |                                |           `mp3`           | String | Set the output audio container remux rules                                                                                                       |
|                 |       |   --extra-ytdl-args    |                                |                           | String | Add extra youtube-dl arguments                                                                                                                   |
|      URLS       |       |                        |                                |                           | String | The URLS (one or more) to be downloaded            (or 0 for error recovery)                                                                     |

Notes:

- This command will store all intermediate downloaded files (until moved) in the tempoarary path specified by [`--tmp`](#global-options).
- Files will not be moved to `output-path` when the Tagger option is chosen (enable "Move Files" in your Tagger).
- `*-stdout` flags enable stdout to be printed to the logs, but to view these `RUST_LOG` must at least be at `trace` (or `-vvv`).
- 0 URLs means to only check for recovery
- in non-interactive mode the default for finishing media is to move files (`m` in interactive mode), can be changed with `--open-tagger`
- if no "sub-langs" are specified, no subtitles will be downloaded and embedded
- the fist subtitle stream is set as "default"
- `--extra-ytdl-args` requires the use of `=`, otherwise clap interprets it as a ytldr arguments, like `--extra-ytdl-args="--max-downloads 10"`
- `--extra-ytdl-args` can be provided infinite times to add extra arguments
- `--extra-ytdl-args` needs to be used once for each extra arguments, like `--extra-ytdl-args="--max-downloads 10" --extra-ytdl-args="--another-option"`

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
- this command does not require `youtube-dl` to be present, but `ffmpeg` is required

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
- this command does not require `youtube-dl` or `ffmpeg` to be present

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

Notes:

- this command does not require `youtube-dl` or `ffmpeg` to be present

### `archive search`

Search the archive for given search parameters
Will Error if [Archive Path](#global-options) is unset

Signature: `archive search [OPTIONS] <QUERIES>...`  
Aliases: `import`

| Positional Name | Short |      Long       | Environment Variable | Default |      Type      | Description                                            |
| :-------------: | :---: | :-------------: | :------------------: | :-----: | :------------: | :----------------------------------------------------- |
|                 |  -l   |     --limit     |                      |   10    |     number     | Set the limit for returned values                      |
|                 |  -f   | --result-format |                      | normal  |      enum      | Set which format the resulting values are printed in   |
|     QUERIES     |       |                 |                      |         | String, String | Key-Value pairs of Queries where the key is the column |

Supported Columns are:

- `Provider`
- `Title`
- `MediaId`, `id`
- `InsertedAt`, `inserted`

column names are case-insensitive

Supported Output formats are:

- `Normal`: custom formatting `[provider:media_id] [inserted_at] title`
- `CSVC`: CSV, comma delimited `provider,media_id,inserted_at,title`
- `CSVT`: CSV, tab delimited `provider\tmedia_id\tinserted_at\ttitle`

Supported Date range operators (default: `=`): `> < >= <= =`

Examples:

```sh
ytdlr archive search title="Some Good Title"
ytdlr archive search title=sometitle
ytdlr archive search "title=Some Good Title"
ytdlr archive search inserted=">=2023-04"
ytdlr archive search provider=youtube title="bug"
```

## Notes

This Project is mainly a personal project, so it is currently tailored to my use-cases, but issues / requests will still be reviewed.

## Project TODO

Currently there is nothing to-do.
