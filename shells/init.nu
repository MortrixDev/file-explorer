def --env fex [...args] {
    let tmp = (mktemp -t fex-XXXXXX)
    ^"@BIN@" ...$args --cwd-file $tmp
    let dir = (open --raw $tmp | str trim)
    rm -f $tmp
    if ($dir | is-not-empty) and ($dir | path exists) {
        cd $dir
    }
}
