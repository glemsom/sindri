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

## audit: every `scripts/audit-*.sh` the tree holds, each run to its own end —
## Rust modules at or under 800 lines, every module opening with its own `//!`
## paragraph, every crate root placing its modules on one line each, every
## module file under a tests tree named by a declaration, every test's name
## stating the behaviour it proves. A directory listing and not a list of
## scripts, so an audit added under `scripts/` is an audit the commit hook and
## the gate both run, and a tree that breaks two rules hears about both in one
## pass rather than one per round trip.
audit:
	@failed=0; \
	for audit in scripts/audit-*.sh; do \
		"$$audit" || failed=1; \
	done; \
	exit $$failed

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

## eval: ten scripted runs against recorded fixtures — offline, no key, no
## live provider. Prints one pass/fail line per eval and fails if any did.
## `make eval-live` asks your own provider instead; see the README.
eval:
	$(CARGO) build --quiet --package sindri-cli --bin sindri
	$(CARGO) run --quiet --package sindri-evals

## eval-live: the same ten evals against a real provider, with your own
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
## into dist/. The steps are the release image's, and so is the musl std they
## need: on a dev machine this fails, and the workflow is authoritative (it also
## verifies and publishes). See docs/install.md.
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
