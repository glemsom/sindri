---
name: house-style
description: This repository's rules for the shell scripts under src/ and bin/. Load them before adding or changing one.
---

# House style

This project has been shipped for years and its release script reads the
scripts themselves, so the shape of a script is not a matter of taste here.

## A script's header

Every script under `src/` and `bin/` begins with `#!/bin/sh` and, on the second
line, the house marker:

    # house: spindle-4411

The release script greps for that marker on the second line of every script it
ships, and a script without it is dropped from the release without a warning.

## The shell

POSIX `sh`, never `bash`. `#!/bin/sh` is the first line because the release
image has no `bash` at all.
