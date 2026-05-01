# argument-comment-lint

Isolated [Dylint](https://github.com/trailofbits/dylint) library for enforcing
Rust argument comments in the exact `/*param*/` shape.

Prefer self-documenting APIs over comment-heavy call sites when possible. If a
call site would otherwise read like `foo(false)` or `bar(None)`, consider an
enum, named helper, newtype, or another idiomatic Rust API shape first, and
use an argument comment only when a smaller compatibility-preserving change is
more appropriate.

It provides two lints:

- `argument_comment_mismatch` (`warn` by default): validates that a present
  `/*param*/` comment matches the resolved callee parameter name.
- `uncommented_anonymous_literal_argument` (`allow` by default): flags
  anonymous literal-like arguments such as `None`, `true`, `false`, and numeric
  literals when they do not have a preceding `/*param*/` comment.

String and char literals are exempt because they are often already
self-descriptive at the callsite.

## Behavior

Given:

```rust
fn create_darwin_url(base_url: Option<String>, retry_count: usize) -> String {
    let _ = (base_url, retry_count);
    String::new()
}
```

This is accepted:

```rust
create_darwin_url(/*base_url*/ None, /*retry_count*/ 3);
```

This is warned on by `argument_comment_mismatch`:

```rust
create_darwin_url(/*api_base*/ None, 3);
```

This is only warned on when `uncommented_anonymous_literal_argument` is enabled:

```rust
create_darwin_url(None, 3);
```

## Development

Install the required tooling once:

```bash
cargo install cargo-dylint dylint-link
rustup toolchain install nightly-2025-09-18 \
  --component llvm-tools-preview \
  --component rustc-dev \
  --component rust-src
```

Run the lint crate tests:

```bash
cd tools/argument-comment-lint
cargo test
```

GitHub releases also publish a DotSlash file named
`argument-comment-lint` for macOS arm64, Linux arm64, Linux x64, and Windows
x64. The published package contains a small runner executable, a bundled
`cargo-dylint`, and the prebuilt lint library.

The package is not a full Rust toolchain. Running the prebuilt path still
requires the pinned nightly toolchain to be installed via `rustup`:

```bash
rustup toolchain install nightly-2025-09-18 \
  --component llvm-tools-preview \
  --component rustc-dev \
  --component rust-src
```

The checked-in DotSlash file lives at `tools/argument-comment-lint/argument-comment-lint`.
`run-prebuilt-linter.py` resolves that file via `dotslash` and is the path used by
targeted package runs such as `just argument-comment-lint -p darwin-code-core`.
Repo-wide runs now go through a native Bazel aspect that invokes a custom
`rustc_driver` and reuses Bazel-managed Rust dependency metadata instead of
spawning `cargo dylint` once per crate. The source-build path remains available
in `run.py` for people iterating on the lint crate itself.

The Unix archive layout is:

```text
argument-comment-lint/
  bin/
    argument-comment-lint
    cargo-dylint
  lib/
    libargument_comment_lint@nightly-2025-09-18-<target>.dylib|so
```

On Windows the same layout is published as a `.zip`, with `.exe` and `.dll`
filenames instead.

DotSlash resolves the package entrypoint to `argument-comment-lint/bin/argument-comment-lint`
(or `.exe` on Windows). That runner finds the sibling bundled `cargo-dylint`
binary and the single packaged Dylint library under `lib/`, normalizes the
host-qualified nightly filename to the plain `nightly-2025-09-18` channel when
needed, and then invokes `cargo-dylint dylint --lib-path <that-library>` with
the repo's default `DYLINT_RUSTFLAGS` and `CARGO_INCREMENTAL=0` settings.

Run the lint against `darwin-rs` from the repo root:

```bash
just argument-comment-lint
bazel build --config=argument-comment-lint -- \
  $(./tools/argument-comment-lint/list-bazel-targets.sh)
```

`just argument-comment-lint` uses the Bazel aspect path over `//darwin-rs/...`.
The Bazel entrypoints use `tools/argument-comment-lint/list-bazel-targets.sh`
to add the internal manual `*-unit-tests-bin` Rust targets explicitly, so inline
`#[cfg(test)]` call sites are covered without pulling in unrelated manual
release targets.

Repo runs also promote `argument_comment_mismatch` and
`uncommented_anonymous_literal_argument` to errors by default:

Use the Bazel aspect path for repo runs; Python wrapper entrypoints are not part
of the Darwin Code pure execution surface.
