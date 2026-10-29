# The installer as a user meets it: no Rust, one command, a checksum verified
# before anything is written, and a last line that sends them to `doctor`.

# The release base URL is a `file://` directory in the test's own temp, so no
# test needs the network (see run_installer in lib.sh).

test_installs_the_asset_and_ends_by_pointing_at_doctor() {
    make_release "$PWD/releases" 0.1.0 x86_64-unknown-linux-musl >/dev/null
    run_installer --version 0.1.0 --dir "$PWD/bin"
    assert_status 0 "$STATUS"
    assert_executable "$PWD/bin/sindri"
    assert_contains "$OUT" "sindri 0.1.0"
    # The last line is the one a hurried user reads.
    [ "$(printf '%s\n' "$OUT" | tail -1 | grep -c 'sindri doctor')" = 1 ] \
        || fail "last line does not point at doctor: $OUT"
}

test_verifies_the_checksum_before_installing() {
    asset=$(make_release "$PWD/releases" 0.1.0 x86_64-unknown-linux-musl)
    printf 'tampered\n' >>"$asset"
    run_installer --version 0.1.0 --dir "$PWD/bin"
    [ "$STATUS" -ne 0 ] || fail "a bad checksum installed anyway"
    assert_contains "$ERR" "checksum"
    assert_no_file "$PWD/bin/sindri"
}

test_picks_the_architecture_from_the_machine() {
    make_release "$PWD/releases" 0.1.0 x86_64-unknown-linux-musl >/dev/null
    make_release "$PWD/releases" 0.1.0 aarch64-unknown-linux-musl >/dev/null
    SINDRI_UNAME_M=aarch64 run_installer --version 0.1.0 --dir "$PWD/bin"
    assert_status 0 "$STATUS"
    grep -q 'aarch64-unknown-linux-musl' "$PWD/bin/sindri" \
        || fail "installed the wrong architecture"
}

test_refuses_a_machine_it_has_no_binary_for() {
    make_release "$PWD/releases" 0.1.0 x86_64-unknown-linux-musl >/dev/null
    SINDRI_UNAME_M=riscv64 run_installer --version 0.1.0 --dir "$PWD/bin"
    assert_status 2 "$STATUS"
    assert_contains "$ERR" "riscv64"
    assert_no_file "$PWD/bin/sindri"
}

test_refuses_a_kernel_that_is_not_linux() {
    make_release "$PWD/releases" 0.1.0 x86_64-unknown-linux-musl >/dev/null
    SINDRI_UNAME_S=Darwin run_installer --version 0.1.0 --dir "$PWD/bin"
    assert_status 2 "$STATUS"
    assert_contains "$ERR" "Linux"
}

test_resolves_the_latest_release_when_no_version_is_given() {
    make_release "$PWD/releases" 0.2.0 x86_64-unknown-linux-musl >/dev/null
    printf '{"tag_name": "v0.2.0"}\n' >"$PWD/latest.json"
    SINDRI_LATEST_URL="file://$PWD/latest.json" run_installer --dir "$PWD/bin"
    assert_status 0 "$STATUS"
    assert_contains "$OUT" "sindri 0.2.0"
}

test_installs_where_the_environment_says() {
    make_release "$PWD/releases" 0.1.0 x86_64-unknown-linux-musl >/dev/null
    SINDRI_INSTALL_DIR="$PWD/local/bin" SINDRI_VERSION=0.1.0 run_installer
    assert_status 0 "$STATUS"
    assert_executable "$PWD/local/bin/sindri"
}

test_reports_the_version_the_installed_binary_says() {
    # The release's name is a claim; the binary's own `--version` is evidence,
    # and that is what the user is told they now have.
    asset=$(make_release "$PWD/releases" 0.1.0 x86_64-unknown-linux-musl)
    printf '#!/bin/sh\necho "sindri 9.9.9 (2025-10-29)"\n' >"$asset"
    (cd "$PWD/releases/v0.1.0" && sha256sum ./*sindri-* | sed 's#\./##' >SHA256SUMS)
    run_installer --version 0.1.0 --dir "$PWD/bin"
    assert_status 0 "$STATUS"
    assert_contains "$OUT" "sindri 9.9.9"
}

test_refuses_a_release_with_no_checksum_file() {
    make_release "$PWD/releases" 0.1.0 x86_64-unknown-linux-musl >/dev/null
    rm "$PWD/releases/v0.1.0/SHA256SUMS"
    run_installer --version 0.1.0 --dir "$PWD/bin"
    [ "$STATUS" -ne 0 ] || fail "installed without a checksum file"
    assert_no_file "$PWD/bin/sindri"
}
