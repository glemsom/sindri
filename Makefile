# `make check` is the definition of done: the stages below, in this order, the first
# failure stopping the run and naming the stage that failed.

# Run it inside `devbox shell`: cargo, rustfmt, clippy and the musl target come from
# devbox's init hook, so nothing here names a toolchain path, and a check that cannot
# run fails instead of reporting success.

#     make check                       every stage, in order
#     make audit                       only the audits
#     make sandbox                     only the sandbox tier, and its contract test

#     make check STAGES='fmt clippy'   a subset, in order
#     make <stage>                     one stage alone, e.g. frames or test
#     make frame F=boot-120x40         print one frame as the code draws it now

#     make frame-update F=boot-120x40  bless a deliberate change to that frame
#     SINDRI_BLESS=1 make evals        rewrite the evals' goldens from the tree
#                                      itself; doctor and mermaid have none

# A stage line reads `check: <stage> ... ok` or `... FAIL`, printed after the stage's
# own output so the verdict is the last thing the eye lands on.

# 77 is the status that asks for a skip, with the reason on the line above, and only a
# skip keeps exit 0 — a stage that ran and failed stops the run. `make -j` changes
# nothing: the stages are a loop, not prerequisites, so they stay in order.

SHELL := /bin/bash
.SHELLFLAGS := -eu -o pipefail -c
.DEFAULT_GOAL := check

# Cheap and fast first, so a slow stage runs only on a tree that already passes the quick
# ones; `release` last, being the only stage that builds for a target nobody else does.
STAGES := fmt clippy audit test frames bench gate toolchain sandbox evals no-relink release

.PHONY: check $(STAGES) frame frame-update sandbox-tier release-tier gate-tier toolchain-tier

check:
	@for stage in $(STAGES); do \
		$(MAKE) --no-print-directory $$stage || exit 1; \
	done

define stage
@if $(2); then printf 'check: $(1) ... ok\n'; else printf 'check: $(1) ... FAIL\n'; exit 1; fi
endef

# Three verdicts relayed from the command's exit code, so two of them can never blur.
define skippable
@status=0; $(2) || status=$$?; \
  case $$status in \
    0)  printf 'check: $(1) ... ok\n' ;; \
    77) printf 'check: $(1) ... SKIPPED\n' ;; \
    *)  printf 'check: $(1) ... FAIL\n'; exit 1 ;; \
  esac
endef

fmt:
	$(call stage,fmt,cargo fmt --all --check)

# `-D warnings`: a clippy warning is a failure, not a note.
clippy:
	$(call stage,clippy,cargo clippy --all-targets -- -D warnings)

audit:
	$(call stage,audit,bash scripts/audit.sh .)

test:
	$(call stage,test,cargo test --workspace)

# `--nocapture` lets each fixture's own `check: frame ... ok` line out beneath this stage's
# verdict, and one thread keeps two runs reading in the same order.
frames:
	$(call stage,frames,cargo test -p sindri --test frames -- --nocapture --test-threads=1)

# `SINDRI_FRAME` names the frame to print and never changes what is compared, so a stray
# variable cannot make a test report a picture it did not check. Each frame test is named
# after its fixture with the dashes underscored, so the filter names exactly one frame.
frame:
	$(if $(F),,$(error make frame needs F=<frame>-<cols>x<rows>, e.g. F=boot-120x40))
	@SINDRI_FRAME=$(F) cargo test -q -p sindri --test frames -- --nocapture $(subst -,_,$(F))

# `INSTA_UPDATE=force` is insta's own blessing switch: `cargo insta` is not installed here.
frame-update:
	$(if $(F),,$(error make frame-update needs F=<frame>-<cols>x<rows>, e.g. F=boot-120x40))
	INSTA_UPDATE=force cargo test -q -p sindri --test frames -- $(subst -,_,$(F))

# The table is an estimate of a token count and not one, so the estimator is printed with
# it, and `--nocapture` is what lets both out.
bench:
	$(call stage,bench,cargo test -p sindri-core --test wording_bench -- --nocapture)

# `gate` runs the probe's contract test first and `gate-tier` is the probe alone, as the
# other tiers are split, because a test that rang the gate would ring itself.
gate:
	$(call stage,gate-test,bash scripts/check-probe-test.sh)
	$(call skippable,gate,bash scripts/check-probe.sh)

gate-tier:
	$(call skippable,gate,bash scripts/check-probe.sh)

toolchain:
	$(call skippable,toolchain-test,bash scripts/toolchain-probe-test.sh)
	$(call skippable,toolchain,bash scripts/toolchain-probe.sh)

toolchain-tier:
	$(call skippable,toolchain,bash scripts/toolchain-probe.sh)

sandbox:
	$(call stage,sandbox-test,bash scripts/sandbox-probe-test.sh)
	$(call skippable,sandbox,bash scripts/sandbox-probe.sh)

sandbox-tier:
	$(call skippable,sandbox,bash scripts/sandbox-probe.sh)

# Every eval test is `#[ignore]`d, so `make test` compiles the evals and judges none of
# them, and a drift is reported by the stage that owns it. One `cargo test` per family,
# with `&&` so the first family that fails stops the rest.

# The mermaid corpus has no golden: the renderer measures labels against the fonts the
# machine has, so a golden would be a golden of the box that blessed it.
EVALS := cargo test -p sindri-core --test eval_wire -- --ignored --nocapture && \
         cargo test -p sindri-core --test eval_redaction -- --ignored --nocapture && \
         cargo test -p sindri --test eval_doctor -- --ignored --nocapture && \
         cargo test -p sindri --test eval_mermaid -- --ignored --nocapture

evals:
	$(call stage,evals,$(EVALS))

no-relink:
	$(call stage,no-relink,bash scripts/no-relink-proof.sh)

# `skippable` for bwrap alone, when the kernel forbids unprivileged user namespaces:
# everything else is pinned by `devbox.json`, so a musl link that fails is a FAIL — the
# same `undefined reference` is what a broken toolchain and a broken tree print.
release:
	$(call stage,release-test,bash scripts/release-probe-test.sh)
	$(call skippable,release,bash scripts/release-probe.sh)

release-tier:
	$(call skippable,release,bash scripts/release-probe.sh)
