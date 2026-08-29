function fex
    set -l tmp (mktemp)
    "@BIN@" $argv --cwd-file $tmp
    set -l dir (cat $tmp)
    rm -f $tmp
    if test -n "$dir" -a -d "$dir"
        cd $dir
    end
end
