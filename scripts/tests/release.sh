# The release scripts: the artefact names the build produces must be the names
# the installer looks for, and the checksum file must cover exactly the
# artefacts the release carries. All offline — dry runs, a `file://` release
# and a throwaway git repository.

make_artefact() { # dir name
    mkdir -p "$1"
    printf '#!/bin/sh\necho fake\n' >"$1/$2"
}

test_checksums_cover_every_artefact_and_nothing_else() {
    make_artefact dist sindri-0.1.0-aarch64-unknown-linux-musl
    make_artefact dist sindri-0.1.0-x86_64-unknown-linux-musl
    printf 'not a binary\n' >dist/notes.txt
    bash "$SINDRI_ROOT/scripts/release-checksums.sh" dist >/dev/null
    assert_file dist/SHA256SUMS
    names=$(awk '{ print $2 }' dist/SHA256SUMS)
    [ "$names" = "$(printf 'sindri-0.1.0-aarch64-unknown-linux-musl\nsindri-0.1.0-x86_64-unknown-linux-musl')" ] \
        || fail "SHA256SUMS covers the wrong files: $names"
    (cd dist && sha256sum -c --quiet SHA256SUMS) || fail "SHA256SUMS does not verify"
}

test_checksums_refuse_a_release_with_no_artefacts() {
    mkdir -p dist
    set +e
    bash "$SINDRI_ROOT/scripts/release-checksums.sh" dist >/dev/null 2>err
    status=$?
    set -e
    [ "$status" -ne 0 ] || fail "an empty release produced checksums"
    assert_contains "$(cat err)" "artefact"
}

# A throwaway repository: one tagged commit, then merge commits in the repo's
# own ticket style, which is where a release's changelog comes from.
make_history() {
    git init -q .
    git config user.email ship@example.invalid
    git config user.name ship
    git commit -q --allow-empty -m "T1: the first slice (#1)"
    git tag v0.0.1
    git checkout -q -b one
    git commit -q --allow-empty -m "T9: your own models, over the OpenAI-compatible wire (#11)"
    git commit -q --allow-empty -m "wip: not a ticket, not in the changelog"
    git checkout -q -
    git merge -q --no-ff one -m "T9: your own models, over the OpenAI-compatible wire (#11)"
    git tag v0.1.0
}

test_changelog_lists_the_tickets_merged_since_the_previous_tag() {
    make_history
    out=$(bash "$SINDRI_ROOT/scripts/release-changelog.sh" v0.1.0)
    assert_contains "$out" "## v0.1.0"
    assert_contains "$out" "- your own models, over the OpenAI-compatible wire (#11)"
    assert_contains_absent "$out" "the first slice"
    assert_contains_absent "$out" "not a ticket"
}

test_changelog_says_so_when_a_release_closed_nothing() {
    make_history
    git tag v0.1.1
    out=$(bash "$SINDRI_ROOT/scripts/release-changelog.sh" v0.1.1)
    assert_contains "$out" "No tickets closed in this release."
}

test_dry_run_names_both_targets_and_their_artefacts() {
    out=$(bash "$SINDRI_ROOT/scripts/release-build.sh" --dry-run v0.1.0 dist)
    assert_contains "$out" "--target x86_64-unknown-linux-musl"
    assert_contains "$out" "--target aarch64-unknown-linux-musl"
    assert_contains "$out" "dist/sindri-0.1.0-x86_64-unknown-linux-musl"
    assert_contains "$out" "dist/sindri-0.1.0-aarch64-unknown-linux-musl"
}

test_installer_takes_the_artefact_names_the_build_produces() {
    out=$(bash "$SINDRI_ROOT/scripts/release-build.sh" --dry-run v0.1.0 dist)
    for name in $(printf '%s\n' "$out" | grep -o 'sindri-0\.1\.0-[A-Za-z0-9_-]*musl'); do
        make_release "$PWD/releases" 0.1.0 "${name#sindri-0.1.0-}" >/dev/null
    done
    run_installer --version 0.1.0 --dir "$PWD/bin"
    assert_status 0 "$STATUS"
    assert_executable "$PWD/bin/sindri"
}

test_build_honours_the_cargo_command_it_is_given() {
    # The release image builds with a rustup toolchain of devbox's own rustc
    # version (the only way to get musl std); the script must not hard-code
    # `cargo`, or the container cannot use it.
    out=$(CARGO='cargo +1.99.0' bash "$SINDRI_ROOT/scripts/release-build.sh" --dry-run v0.1.0 dist)
    assert_contains "$out" "cargo +1.99.0 zigbuild"
}

test_workflow_publishes_both_targets_with_their_checksums() {
    # Guards the contract the three scripts and the workflow share: a tag
    # triggers it, both binaries are built, the checksums are re-verified, and
    # nothing is published that is not static.
    workflow="$SINDRI_ROOT/.github/workflows/release.yml"
    assert_file "$workflow"
    body=$(cat "$workflow")
    assert_contains "$body" "tags:"
    assert_contains "$body" "release-build.sh"
    assert_contains "$body" "release-checksums.sh"
    assert_contains "$body" "release-changelog.sh"
    assert_contains "$body" "sha256sum -c SHA256SUMS"
    assert_contains "$body" "statically linked"
    assert_contains "$body" "gh release create"
}
