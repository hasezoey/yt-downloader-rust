# YT-Downloader RUST

**The Project is currently being restructured, and the README is not up-to-date**

## Requirements

- Linux / Mac - build with POSIX system paths in mind (Windows *might* work)
- youtube-dl is installed and be accessable via the command `youtube-dl`
- ffmpeg is installed and be accessable via the command `ffmpeg`
- rust stable 1.50 or higher

## Usage

### Basic Usage

`yt-downloader <URL>` (replace `<URL>` with the URL)

Parameters:

| Short |    Long    | Environment Variable |            Default            | Description                                                |
| :---: | :--------: | :------------------: | :---------------------------: | :--------------------------------------------------------- |
|  -a   |            |                      |                               | Output files will be audio-only                            |
|  -h   |   --help   |                      |                               | List the help (basically this table)                       |
|  -d   |            |                      |                               | Enable Command Verbose output (youtube-dl, ffmpeg)         |
|  -c   |            |                      |                               | Disable Cleanup after successful run                       |
|  -t   |            |                      |                               | Disable re-applying the thumbnail after running the editor |
|       |   --out    |       YTDL_OUT       |    `~/Downloads/ytdl-out`     | Set the Output Directory                                   |
|       |   --tmp    |       YTDL_TMP       |       `/tmp/ytdl-rust`        | Set the Temporary Directory to use                         |
|       | --archive  |     YTDL_ARCHIVE     | `~/.config/ytdl_archive.json` | Set the Archive file path                                  |
|       | --askedit  |     YTDL_ASKEDIT     |            `true`             | Ask for edit or directly move to Output Directory          |
|       |  --editor  |     YTDL_EDITOR      |                               | Set what editor to use on an file                          |
|       | --debugger |                      |            `false`            | Request to start the CodeLLDB Debugger in vscode           |
|       |            |                      |                               | URL to download                                            |
|       |     --     |                      |                               | Extra youtube-dl parameters                                |

### Import youtube-dl archive

An existing Youtube-DL archive can also be imported by using the subcommand `import`

Example: `yt-downloader import ./archive`

This subcommand will use the out-archive location of [`--archive`](#basic-usage)

---

Please note this project is still in development (so not finished) and im still new to rust

## Project TODO

- [ ] Rework 2022
  - [ ] Move Archive to SQL (SQLite by default) instead of big json
  - [ ] re-implement main download
  - [x] add QOL command `re-thumbnail`
  - [ ] add QOL command `archive search` to search through the archive by any column
  - [ ] add QOL command `completions` to generate shell completions (bash, zsh, etc)
  - [x] re-implement `archive import`
  - [ ] add command `archive migrate` (to check and migrate the archive to new versions)
  - [ ] completely seperate `libytdlr(lib)` and `ytdlr(bin)`
  - [ ] make binary interface more pleasant
- [ ] Picard Integration (generic)
- [ ] Sponsorblock integration via yt-dlp?
