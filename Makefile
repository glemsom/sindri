# The developer entry point. Every Rust step goes through devbox, which pins the
# toolchain; `make check` is the single definition of done.

DEVBOX ?= devbox
CARGO  ?= $(DEVBOX) run -- cargo

.PHONY: all check fmt clippy audit hooks test relink eval eval-live scripts-check build release run install clean

all: check

## check: the gate. fmt, clippy with warnings denied, the four audits — 800
## lines, module docs, the module map, test names — the tests, the evals, proof
## that an unchanged tree does not relink, and the release scripts run as
## processes.
check: fmt clippy audit test relink eval scripts-check

fmt:
	$(CARGO) fmt --all --check

clippy:
	$(CARGO) clippy --workspace --all-targets -- -D warnings

## audit: Rust modules stay at or under 800 lines, every module opens with its
## own `//!` paragraph, every crate root places its modules on one line each, and
## every test's name states the behaviour it proves.
audit:
	@scripts/audit-line-length.sh
	@scripts/audit-module-docs.sh
	@scripts/audit-module-map.sh
	@scripts/audit-test-names.sh

## hooks: put the fast audits in front of the commit rather than the push — a
## local `core.hooksPath`, opt-in, so a fresh clone commits freely until it asks
## for this. `make check` stays the gate; this only moves the cheap part earlier.
hooks:
	@git config core.hooksPath .githooks
	@echo "hooks: core.hooksPath is .githooks — every commit now runs make audit"

test:
	$(CARGO) nextest run --workspace --no-fail-fast
	$(CARGO) test --doc --workspace

## relink: the build embeds `git describe` and the commit date, never the wall
## clock, so an unchanged tree must not relink.
relink:
	@$(DEVBOX) run -- scripts/check-relink.sh

## eval: nine scripted runs against recorded fixtures — offline, no key, no
## live provider. Prints one pass/fail line per eval and fails if any did.
## `make eval-live` asks your own provider instead; see the README.
eval:
	$(CARGO) build --quiet --package sindri-cli --bin sindri
	$(CARGO) run --quiet --package sindri-evals

## eval-live: the same nine evals against a real provider, with your own
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
