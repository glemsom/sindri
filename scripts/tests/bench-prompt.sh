# The system-prompt bench's baseline and its variant build, driven without a
# build and without a bill: what is asserted is that the baseline is read out of
# the one file the tree ships its stance in, that v00 patches nothing, and that a
# build — finished or failed — puts that file back byte for byte. The sweep, the
# median and the runs are `bench-system-prompt/self-test.sh`'s job, and that one
# is manual.

BENCH="$SINDRI_ROOT/scripts/bench-system-prompt"

# One question asked of `variants.py`, in the tree the gate is running against,
# and printed back. `with_variants CODE` runs a longer piece with the module
# already imported, its second argument the path a test may patch.
ask() { # python expression over the module
    with_variants "print($1)"
}

with_variants() { # python code, then an optional path
    python3 - "$BENCH" "${2:-}" "$1" <<'PY'
import pathlib, sys
sys.path.insert(0, sys.argv[1])
import variants
path = sys.argv[2] or None
exec(sys.argv[3])
PY
}

# The files in the tree that ship the stance, one path per line: found by the
# marker the bench itself reads, so a test cannot search for a string the bench
# stopped looking for.
shipping() {
    grep -rlF --include='*.rs' "$(ask 'variants.MARK')" "$SINDRI_ROOT/crates" | sort
}

# The shipped file, copied where a test may patch it, and the copy's digest
# before anything is written. `PROMPT` is how the build script is pointed at the
# copy: the gate never holds the tree's own file open.
copy_shipped() {
    cp "$(ask 'variants.SHIPPED')" source.rs
    before=$(sha256sum <source.rs)
}

test_the_baseline_is_read_out_of_the_one_file_that_ships_the_stance() {
    files=$(shipping)
    [ "$(printf '%s\n' "$files" | grep -c .)" -eq 1 ] \
        || fail "the stance is shipped in more than one file: $files"
    [ "$(ask 'variants.SHIPPED')" = "$files" ] \
        || fail "the bench reads $(ask 'variants.SHIPPED') and the tree ships $files"
    base=$(ask 'variants.BASE')
    case "$base" in
        "You are Sindri"*) ;;
        *) fail "the baseline is not the stance the tree ships: $base" ;;
    esac
}

test_v00_is_the_shipped_stance_and_patches_nothing() {
    copy_shipped
    patched=$(with_variants 'print(variants.patch("v00", pathlib.Path(path)))' source.rs)
    [ "$(sha256sum <source.rs)" = "$before" ] || fail "v00 patched the source"
    [ "$patched" = "$(ask 'variants.BASE')" ] || fail "v00 is not the stance the tree ships"
}

# The rewritten preamble is a measured wording, so it is written down rather than
# read out of the file: a sweep of `v15` names the text it measured, and the file
# moving under it cannot re-label that sweep. The prompt the tree ships is always one
# of the bench's own texts, which is what makes the wording a sweep was taken with
# findable after the file moves — `v15` today, and a new variant beside it on the day
# a ticket rewrites the stance, never `v15` overwritten.
test_the_prompt_the_tree_ships_is_a_variant_the_bench_holds() {
    with_variants 'print(any(text == variants.BASE for _, text in variants.VARIANTS.values()))' >found
    [ "$(cat found)" = True ] \
        || fail "the stance the tree ships is no variant the bench holds: pin it as a \
new variant beside v15 (which stays the record of #416's wording) and measure that"
    with_variants 'print(variants.VARIANTS["v15"][1])' >pinned
    [ -s pinned ] || fail "the bench holds no pinned variant of the rewritten preamble"
}

test_the_pinned_wording_is_a_literal_a_sweep_can_be_built_from() {
    # A `v15` computed from `shipped()` would follow the file and record nothing, so
    # the literal is the point of the entry — and it still has to be patchable.
    [ "$(grep -c '^REWRITTEN = (' "$BENCH/variants.py")" -eq 1 ] \
        || fail "the rewritten preamble is not a literal in variants.py"
    copy_shipped
    with_variants 'variants.patch("v15", pathlib.Path(path))
assert (variants.shipped(path) == variants.VARIANTS["v15"][1]
        == variants.REWRITTEN), "v15 does not patch the stance"' source.rs
}

test_a_variant_is_written_into_the_stance_literal() {
    copy_shipped
    with_variants 'variants.patch("v13", pathlib.Path(path))
assert variants.shipped(path) == variants.VARIANTS["v13"][1], "the source does not read back as the variant"' source.rs
}

test_a_variant_the_bench_does_not_hold_is_refused() {
    copy_shipped
    set +e
    with_variants 'variants.patch("v99", pathlib.Path(path))' source.rs >/dev/null 2>err
    status=$?
    set -e
    ERR=$(cat err)
    [ "$status" -ne 0 ] || fail "a variant the bench does not hold was written in"
    assert_contains "$ERR" "v99"
    [ "$(sha256sum <source.rs)" = "$before" ] || fail "a refused variant still patched the source"
}

test_a_build_that_fails_leaves_the_source_untouched() {
    copy_shipped
    # The build in the middle is a stand-in that reads the stance it was handed
    # and fails: what it saw is the variant, and what the tree is left with is
    # the source it started from.
    cat >stub <<STUB
#!/bin/sh
cp "\$PROMPT" "$PWD/seen.rs"
exit 1
STUB
    chmod +x stub
    set +e
    PROMPT="$PWD/source.rs" DEVBOX="$PWD/stub" "$BENCH/build_variant.sh" v02 >/dev/null 2>&1
    status=$?
    set -e
    [ "$status" -ne 0 ] || fail "a build that failed was reported as a build"
    # What the build saw is the variant's own text, read from the bench rather
    # than copied into this test.
    with_variants 'print(variants.VARIANTS["v02"][1])' >expected
    assert_contains "$(cat seen.rs)" "$(cat expected)"
    [ "$(sha256sum <source.rs)" = "$before" ] || fail "a failed build left the stance patched"
}
