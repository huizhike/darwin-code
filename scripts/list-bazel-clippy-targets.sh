#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

# Resolve the dynamic targets before printing anything so callers do not
# continue with a partial list if `bazel query` fails.
manual_rust_test_targets="$(bazel query 'kind("rust_test rule", attr(tags, "manual", //darwin-rs/... except //darwin-rs/v8-poc/...))')"

printf '%s\n' \
  "//darwin-rs/..." \
  "-//darwin-rs/v8-poc:all"

# `--config=clippy` on the `workspace_root_test` wrappers does not lint the
# underlying `rust_test` binaries. Add the internal manual `*-unit-tests-bin`
# targets explicitly so inline `#[cfg(test)]` code is linted like
# `cargo clippy --tests`.
printf '%s\n' "${manual_rust_test_targets}"
