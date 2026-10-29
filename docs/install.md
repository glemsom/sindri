# Installing, and shipping, Sindri

Sindri ships as two static musl binaries — `x86_64-unknown-linux-musl` and
`aarch64-unknown-linux-musl` — plus `SHA256SUMS` and a changelog, on a GitHub
Release tagged `vX.Y.Z`. No Rust toolchain, no libc to match, nothing to
compile on the machine that runs it.

## Install, in one command

```console
$ curl -fsSL https://raw.githubusercontent.com/glemsom/sindri/main/install.sh | sh
```

The installer:

1. refuses a kernel that is not Linux and a machine it has no binary for;
2. resolves the latest release, or the one you name;
3. downloads the binary **and** `SHA256SUMS`, and installs nothing unless the
   checksum matches;
4. installs to `~/.local/bin/sindri`, replacing any earlier copy atomically;
5. prints the version the installed binary reports, and ends by sending you to
   `sindri doctor`.

```console
$ curl -fsSL …/install.sh | sh -s -- --version 0.1.0 --dir /usr/local/bin
```

| Flag | Environment | Default |
| --- | --- | --- |
| `--version VERSION` | `SINDRI_VERSION` | the latest release |
| `--dir DIR` | `SINDRI_INSTALL_DIR` | `~/.local/bin` |

`SINDRI_RELEASE_BASE_URL` points the installer at a mirror (or a `file://`
directory); the tests use it so no test touches the network. Exit `2` is a
usage error, `1` anything else — a failed download, a checksum mismatch, an
unwritable directory.

The last line is not decoration: **run `sindri doctor` on a fresh install.** It
proves bubblewrap, the binary set and the mount plan on *your* machine, and
tells you one actionable line if your distribution blocks unprivileged user
namespaces. An agent failure caused by a sandbox that could never start is the
one bad first impression a static binary must not deliver.

## What a release carries

| File | What it is |
| --- | --- |
| `sindri-X.Y.Z-x86_64-unknown-linux-musl` | static binary for x86_64 |
| `sindri-X.Y.Z-aarch64-unknown-linux-musl` | static binary for aarch64 |
| `SHA256SUMS` | the two lines above, sorted, verified by the installer |
| release notes | `CHANGELOG.md`, the tickets closed since the previous tag |

The version reported by `--version` is `git describe` and the commit date,
injected at build time by `crates/types/build.rs` — never the wall clock, so an
unchanged tree does not relink (ADR-0009).

## Cutting a release (maintainer)

The whole of it is one tag; the workflow does the rest.

```console
$ git checkout main && git pull --ff-only
$ git tag -s v0.1.0 -m "sindri v0.1.0"
$ git push origin v0.1.0
```

`.github/workflows/release.yml` then:

1. generates a release image from the committed `devbox.json`
   (`devbox generate dockerfile`), so dev, CI and release share one toolchain
   definition (ADR-0009);
2. builds both targets inside that image with `cargo-zigbuild`
   (`scripts/release-build.sh`);
3. writes `SHA256SUMS` (`scripts/release-checksums.sh`) and the changelog
   derived from the merge commits since the previous tag
   (`scripts/release-changelog.sh`);
4. re-verifies every checksum and refuses to publish anything that is not
   statically linked;
5. creates the GitHub Release with both binaries, the checksums and the notes.

Locally the same steps, without Docker:

```console
$ make release TAG=v0.1.0   # dist/: both binaries, SHA256SUMS, CHANGELOG.md
$ make scripts-check        # the installer and release scripts, offline
```

`make scripts-check` is part of `make check`. It runs the installer through
`sh` against a `file://` release, so the artefact names the build produces are
proven to be the names the installer looks for.

### Where musl std comes from

devbox's `rustc` includes only the host's standard library, and it refuses the
upstream `rust-std` tarball of the same version (the metadata hashes differ:
`E0514`). There is also no rustup, deliberately — its shims shadow `cargo` and
break the pinned toolchain (ADR-0009).

So the release image installs a rustup toolchain **of devbox's own rustc
version**, with the two musl targets, and builds with `CARGO="cargo +<that
version>"`. It exists only inside the container; a developer's `PATH` is
untouched, and `make check` never sees it.
