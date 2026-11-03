# arms.sh — the three arms of the bench, and the one question each is settled by,
# stated once for the two scripts that build and run them. Sourced, not run: the
# caller sets `BENCH` first, because the list of arms and the module that moves
# the doctrine both live beside it.
#
# The arm is what the binary carries, never the word on the command line, so both
# scripts ask the binary two facts: the catalog `doctor` reports, and whether the
# text it ships carries the doctrine's trigger sentence. The list itself is
# `bench.py`'s — `ARMS`, read back through its `arms` subcommand — so a fourth arm
# is one line there and neither script can hold a stale set of names.

ARMS=$(python3 "$BENCH/bench.py" arms)

# arms_words — the arms as a reader says them: `with, without or no-trigger`.
arms_words() {
  local head=${ARMS% *} last=${ARMS##* }
  printf '%s or %s' "${head// /, }" "$last"
}

# arm_is_a_name ARM — 0 when the word is one the bench builds.
arm_is_a_name() {
  case " $ARMS " in *" $1 "*) return 0 ;; *) return 1 ;; esac
}

# arm_mismatch ARM BIN — the reason this binary is not this arm, printed on
# stdout, and 1; nothing and 0 when the binary is the arm it is labelled.
arm_mismatch() {
  local ships=no carries=no
  if "$2" doctor 2>/dev/null | grep -q '^  builtin '; then ships=yes; fi
  # A `without` binary ships no catalog, so the sentence it carries is not a fact
  # about that arm and is not asked of it: the arm is its absent built-in.
  if [ "$1" != without ] && python3 "$BENCH/doctrines.py" carries "$2"; then carries=yes; fi
  case "$1:$ships:$carries" in
    with:yes:yes|no-trigger:yes:no|without:no:*) return 0 ;;
  esac
  printf 'it ships %s built-in and carries the trigger sentence %s' "$ships" "$carries"
  return 1
}
