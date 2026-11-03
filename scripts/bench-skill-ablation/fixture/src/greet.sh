# greet.sh: the greeting itself, and the log it keeps.

say_hello() {
    dir=$(state_dir)
    mkdir -p "$dir" || return 1
    echo "$1" >> "$dir/names"
    echo "Hello, $1!"
}
