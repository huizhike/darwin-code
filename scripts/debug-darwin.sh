#!/bin/bash

# Set the DarwinCode extension executable override to this script when you
# need the latest local Rust binary while debugging.


set -euo pipefail

DARWIN_RS_DIR=$(realpath "$(dirname "$0")/../darwin-rs")
(cd "$DARWIN_RS_DIR" && cargo run --quiet --bin darwin-code -- "$@")