# `make check` is the single definition of done (spec §16): the stages below, in
# this order, each printing its own summary line, the first failure stopping the
# run with a non-zero exit and naming the stage that failed.
#
# Run it inside `devbox shell`: cargo, rustfmt, clippy and the musl target all
# come from devbox's init hook (ADR 0001), so nothing here names a toolchain
# path. Outside that shell the stages fail because cargo is not on PATH, which
# is the honest answer — a check that cannot run must not report success.
#
#     make check      every stage, in order (the definition of done)
#     make audit      only the four fast audits (spec §16; the commit hook's entry)
#     make <stage>    one stage alone — fmt, clippy, audit, test, toolchain
#
# A stage line reads `check: <stage> ... ok` or `... FAIL`, printed after the
# stage's own output so the verdict is the last thing the eye lands on. Nothing
# is printed for a stage that did not run. The last two stages are placeholders
# for work that has not landed yet (ticket #58): they print `... MISSING` and
# never `ok`, because a green line over nothing is the one lie this target must
# not tell. `make check` therefore exits 0 with those two gaps printed on its
# last two lines, where a reader cannot miss them.
#
# `make -j` changes nothing: the stages are a loop, not prerequisites, so they
# stay in order and stop at the first failure.

SHELL := /bin/bash
.SHELLFLAGS := -eu -o pipefail -c
.DEFAULT_GOAL := check

# The definition of done, in order: cheap and fast first, so the slow stages run
# only on a tree that already passes the quick ones.
STAGES := fmt clippy audit test toolchain evals no-relink

.PHONY: check $(STAGES)

check:
	@for stage in $(STAGES); do \
		$(MAKE) --no-print-directory $$stage || exit 1; \
	done

# stage: one stage that really runs. (1) the name it is reported under, (2) the
# command that proves it. The verdict is printed after the command, so it lands
# after whatever pages of output the command produced.
define stage
@if $(2); then printf 'check: $(1) ... ok\n'; else printf 'check: $(1) ... FAIL\n'; exit 1; fi
endef

# missing: a stage whose machinery does not exist yet. It says so plainly and
# prints no `ok`, so the gap is visible rather than counted as green. `make
# check` still exits 0 on a tree whose real stages pass: the gap is ticket #58's
# and tracking it is what that ticket is for. Landing one of these stages is a
# one-line change — swap `$(call missing,<name>,<note>)` for
# `$(call stage,<name>,<command>)` — and nothing else in this file moves.
define missing
@printf 'check: $(1) ... MISSING ($(2))\n'
endef

fmt:
	$(call stage,fmt,cargo fmt --all --check)

clippy:
	$(call stage,clippy,cargo clippy --all-targets -- -D warnings)

# The four audits of spec §16 print all four of their lines, always, and this
# stage adds the verdict: audit.sh exits non-zero if any rule found anything.
audit:
	$(call stage,audit,bash scripts/audit.sh .)

test:
	$(call stage,test,cargo test --workspace)

# Proves the devbox shell supplies the pinned Rust 1.99 and the musl target
# (ADR 0001, ticket #4 wired here by #5).
toolchain:
	$(call stage,toolchain,bash scripts/toolchain-probe.sh)

# Ticket #58 owns both of these. Placeholders until it lands.
evals:
	$(call missing,evals,evals are not yet present - ticket #58)

no-relink:
	$(call missing,no-relink,the no-relink proof is not yet present - ticket #58)
