# ESVG command line

`esvg` optimizes SVG files from a terminal or a script. It runs the same
optimizer as the desktop app, with the same defaults, and it is easy for both
people and agents to drive.

## Install

Every release ships the CLI next to the desktop app.

### macOS

The CLI is inside the app bundle at `ESVG.app/Contents/MacOS/esvg-cli`.
Link it onto your PATH as `esvg` once:

```sh
sudo mkdir -p /usr/local/bin
sudo ln -sf "/Applications/ESVG.app/Contents/MacOS/esvg-cli" /usr/local/bin/esvg
esvg --version
```

If you keep the app somewhere other than `/Applications`, use that path instead.

The app is not signed yet. If macOS refuses to run the app or `esvg` because it
is from an unidentified developer, clear the download quarantine once:

```sh
xattr -dr com.apple.quarantine /Applications/ESVG.app
```

### Windows

Run the installer (`esvg-<version>-windows-x86_64-setup.exe`). It installs the
CLI to `%LOCALAPPDATA%\Programs\ESVG\cli\esvg.exe` and, unless you untick
**Add the esvg command to PATH**, adds that folder to your user PATH. Open a new
terminal afterwards and run `esvg --version`.

The installer only edits your user PATH, not the system PATH, so no
administrator rights are needed. Uninstalling removes the entry again.

If you use the zip instead, put the `cli` folder from it on your PATH yourself.

### Linux

There are two ways to get the CLI:

- **Standalone file.** Download `esvg-cli-<version>-linux-x86_64` from the
  release, next to the AppImage, and install it as `esvg`:

  ```sh
  install -Dm755 esvg-cli-<version>-linux-x86_64 ~/.local/bin/esvg
  esvg --version
  ```

  Make sure `~/.local/bin` is on your PATH.

- **Tarball.** `esvg-<version>-linux-x86_64.tar.gz` contains `cli/esvg`, which
  you can copy to `~/.local/bin`.

An AppImage cannot add anything to your PATH, so the AppImage itself does not
install the CLI. The file next to it is the CLI.

### From source

```sh
cargo build --release -p esvg-cli
install -m755 target/release/esvg-cli ~/.local/bin/esvg
```

The Cargo binary is named `esvg-cli` so it does not clash with the desktop app's
`esvg` in the shared build folder. Installing it as `esvg` gives the name used
everywhere in this document.

## Quick start

```sh
# Print the optimized SVG. The report goes to stderr.
esvg optimize logo.svg > logo.min.svg

# Write the result to a file
esvg optimize logo.svg -o logo.min.svg

# Optimize many files into a folder, keeping their names
esvg optimize "icons/*.svg" -d dist/icons

# Optimize every SVG in a folder tree, overwriting the originals
esvg optimize assets -r -i

# See what would change without writing anything
esvg optimize "icons/*.svg" -n

# Read from stdin, write to stdout
cat logo.svg | esvg optimize - > logo.min.svg
```

Quote glob patterns such as `"icons/*.svg"`. Then the CLI expands them, which
works the same in every shell and on Windows. Unquoted patterns also work, but
the shell expands them first.

## Choosing plugins

Every plugin is on by default. Plugin ids are listed by `esvg plugins`:

| id | What it does |
| --- | --- |
| `apply-transforms` | Bakes transforms into shapes and paths, unless that would change the rendering |
| `collapse-groups` | Removes groups that add nothing, moving their properties onto their children |
| `combine-paths` | Merges neighbouring paths when that cannot change the rendering |
| `css-to-attributes` | Moves inline style properties into plain attributes |
| `mangle-ids` | Shortens ids and class names, updating every reference to them |
| `number-precision` | Rounds numbers to fewer decimal places |
| `optimize-colors` | Rewrites colour values in a shorter form, with the same colour |
| `remove-empty-text` | Removes text elements that have nothing to draw |
| `remove-unnecessary-attrs` | Removes attributes that never affect rendering |
| `remove-unnecessary-clip-paths` | Removes clip paths that do not change the rendering |
| `shape-to-path` | Converts rectangles, circles and other basic shapes into paths |
| `simplify-paths` | Removes redundant points and segments from paths |
| `sort-attrs` | Sorts attributes by name |

```sh
# Turn plugins off. Comma separated or repeated. --disable wins over --enable
esvg optimize logo.svg --disable shape-to-path,mangle-ids

# Run only these plugins
esvg optimize logo.svg --only simplify-paths,number-precision

# Turn a plugin on in addition to the defaults
esvg optimize logo.svg --enable sort-attrs
```

`--only` cannot be combined with `--enable` or `--disable`.

## Options

| Option | Meaning |
| --- | --- |
| `-o, --output FILE` | Write one file. `-` means stdout (the default for a single input) |
| `-d, --out-dir DIR` | Write each result into DIR, keeping its file name. Creates DIR |
| `-i, --in-place` | Overwrite the inputs |
| `-r, --recursive` | Include SVG files in folders, at any depth |
| `-n, --dry-run` | Report sizes without writing anything |
| `--json` | Print the report as JSON |
| `-q, --quiet` | Hide the text report when everything succeeds. Errors and `--json` output are still printed |
| `-j, --jobs N` | Optimize N files at once. Defaults to the number of CPU cores |

Several inputs need `-d` or `-i`. A single input without either writes to stdout.
Two inputs with the same file name cannot share an `-d` folder, so the CLI stops
before writing anything. Names that differ only in letter case count as the
same, because they are the same file on macOS and Windows.

Files are written through a temporary file, so an interrupted run never leaves
a half-written SVG behind. Writing to a symbolic link updates the file it points
to and keeps the link.

## For scripts and agents

- **Exit codes.** `0` means every file was optimized. `1` means at least one
  file failed, and the other files were still processed. That includes a file
  that crashes the optimizer. `2` means the command
  was not valid. Nothing was written in that case.
- **Streams.** When the optimized SVG is written to stdout, the report goes to
  stderr, so the SVG can be piped. Otherwise the report goes to stdout.
- **No prompts.** The CLI never asks a question, so it can run unattended.
- **JSON.** `--json` prints one object:

  ```json
  {
    "dry_run": false,
    "files": [
      {
        "input": "icons/a.svg",
        "output": "dist/a.svg",
        "status": "ok",
        "input_bytes": 1200,
        "output_bytes": 800,
        "saved_bytes": 400,
        "saved_percent": 33.33,
        "error": null
      },
      {
        "input": "icons/b.svg",
        "output": null,
        "status": "error",
        "input_bytes": 40,
        "output_bytes": null,
        "saved_bytes": null,
        "saved_percent": null,
        "error": "Malformed XML. 1:1 Unexpected end of stream"
      }
    ],
    "summary": {
      "files": 2,
      "optimized": 1,
      "failed": 1,
      "input_bytes": 1200,
      "output_bytes": 800,
      "saved_percent": 33.33
    }
  }
  ```

  `output` is `-` when the result went to stdout, and `null` when nothing was
  written. `summary` covers the files that were optimized. `saved_bytes` and
  `saved_percent` are negative when the output is larger.

- **Plugin list.** `esvg plugins --json` prints `[{"id", "name", "default"}, ...]`.

## Limitations

- Optimizing is not repeatable at the byte level yet. The optimizer writes
  attributes in an arbitrary order that changes between runs, so the same
  input can produce different bytes with the same content. The CLI does not
  change that.
- Running the optimizer on its own output can change the file again.
