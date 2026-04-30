#!/bin/bash

# Set the Darwin Code extension executable override to this script when you
# need the latest local Rust binary while debugging.


set -euo pipefail

CODEX_RS_DIR=$(realpath "$(dirname "$0")/../codex-rs")
(cd "$CODEX_RS_DIR" && cargo run --quiet --bin darwin-code -- "$@")