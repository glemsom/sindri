# The developer entry point. Every Rust step goes through devbox, which pins the
# toolchain; `make check` is the single definition of done.

DEVBOX ?= devbox
CARGO  ?= $(DEVBOX) run -- cargo

.PHONY: all check fmt clippy audit test relink eval eval-live scripts-check build release run install clean

all: check

## check: the gate. fmt, clippy with warnings denied, the 400-line audit, tests,
## the evals, proof that an unchanged tree does not relink, and the release
## scripts run as processes.
check: fmt clippy audit test relink eval scripts-check

fmt:
	$(CARGO) fmt --all --check

clippy:
	$(CARGO) clippy --workspace --all-targets -- -D warnings

## audit: source files stay at or under 400 lines, and every module opens
## with its own `//!` paragraph.
audit:
	@scripts/audit-line-length.sh
	@scripts/audit-module-docs.sh

test:
	$(CARGO) nextest run --workspace --no-fail-fast
	$(CARGO) test --doc --workspace

## relink: the build embeds `git describe` and the commit date, never the wall
## clock, so an unchanged tree must not relink.
relink:
	@$(DEVBOX) run -- scripts/check-relink.sh

## eval: eight scripted runs against recorded fixtures — offline, no key, no
## live provider. Prints one pass/fail line per eval and fails if any did.
## `make eval-live` asks your own provider instead; see the README.
eval:
	$(CARGO) build --quiet --package sindri-cli --bin sindri
	$(CARGO) run --quiet --package sindri-evals

## eval-live: the same eight evals against a real provider, with your own
## credential and at your own cost.
eval-live:
	$(CARGO) build --quiet --package sindri-cli --bin sindri
	$(CARGO) run --quiet --package sindri-evals -- --live

## scripts-check: the installer and the release scripts, driven through `sh` and
## `bash` with a `file://` release, a throwaway git repository and dry runs —
## offline, and the one place the artefact naming contract is proven.
scripts-check:
	@scripts/test-scripts.sh

build:
	$(CARGO) build --release

## release: both static musl binaries for TAG, plus SHA256SUMS and the changelog,
## into dist/. The steps are the release image's; the workflow is authoritative
## (it also verifies and publishes). See docs/install.md.
release:
	@rm -rf dist && mkdir -p dist
	@scripts/release-build.sh "$(TAG)" dist
	@scripts/release-checksums.sh dist
	@scripts/release-changelog.sh "$(TAG)" > dist/CHANGELOG.md

run:
	$(CARGO) run -- $(ARGS)

## install: install the working tree's binary.
install:
	$(CARGO) install --path crates/cli --force

clean:
	$(DEVBOX) run -- cargo clean
