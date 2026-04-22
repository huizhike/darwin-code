#!/bin/bash

set -euo pipefail

SCRIPT_DIR=$(realpath "$(dirname "$0")")
trap "popd >> /dev/null" EXIT
pushd "$SCRIPT_DIR/.." >> /dev/null || {
  echo "Error: Failed to change directory to $SCRIPT_DIR/.."
  exit 1
}
pnpm install
pnpm run build
rm -rf ./dist/openai-darwin-code-*.tgz
pnpm pack --pack-destination ./dist
mv ./dist/openai-darwin-code-*.tgz ./dist/darwin-code.tgz
docker build -t darwin-code -f "./Dockerfile" .
