#!/usr/bin/env bash
# Install Kani into ./.tools (it is not packaged in nixpkgs). Run inside `nix develop .#verify`.
set -euo pipefail
version=0.68.0
root=$(git rev-parse --show-toplevel)
if [ "$(cargo kani --version 2>/dev/null | awk 'NR==1{print $4}')" = "$version" ]; then
	exit 0
fi
cargo install --locked --root "$root/.tools" --version "$version" kani-verifier
cargo kani setup
