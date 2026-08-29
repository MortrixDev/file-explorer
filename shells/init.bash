fex() {
  local tmp
  tmp="$(mktemp)" || return 1
  "@BIN@" "$@" --cwd-file "$tmp"
  local dir
  dir="$(cat -- "$tmp")"
  rm -f -- "$tmp"
  [ -n "$dir" ] && [ -d "$dir" ] && cd -- "$dir"
}
