# file-explorer

A terminal file explorer with a preview pane: two-column listing, syntax-highlighted
text previews, inline images and PDF pages (Kitty graphics protocol), and file metadata.

## Install

With Cargo:

```sh
cargo install --path .
```

With Nix:

```sh
nix profile install .#file-explorer
```

Or run it without installing:

```sh
nix run .
```

## Shell init

The binary can print a `fex` shell function that changes your shell's directory to
wherever you left off. Add the matching line to your shell config:

```sh
eval "$(file-explorer init bash)"          # bash / zsh
file-explorer init fish | source           # fish
file-explorer init nu | save -f ~/.fex.nu  # nu, then `source ~/.fex.nu`
```

## Usage

```
file-explorer [DIR] [--cwd-file PATH]
file-explorer init <bash|zsh|fish|nu>
```

`DIR` is the directory to open, defaulting to the current one. `--cwd-file PATH`
writes the final directory to `PATH` on exit, which is how the `fex` function
follows your navigation.

### Keys

| Key | Action |
| --- | --- |
| `j` / `k`, arrows | move cursor |
| `d` / `u` | page down / up |
| `g` / `G` | first / last entry |
| `l`, `→`, `Enter` | enter directory |
| `h`, `←`, Backspace | go to parent |
| `.` | toggle hidden files |
| `space` | mark / unmark entry |
| `m` / `c` | move / copy marked entries here |
| `r` | rename |
| `o`, `Enter` | open file with `xdg-open` |
| `e` | open file in `$EDITOR` |
| `q`, `Esc` | quit, writing the current directory |
| `Q` | quit without changing directory |
