{
  description = "Dedaliano — structural analysis engine + AI backend";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
          targets = [ "wasm32-unknown-unknown" ];
        };
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = [
            # Rust
            rustToolchain
            pkgs.wasm-pack
            pkgs.cargo-watch

            # Node / frontend
            pkgs.nodejs_22
            pkgs.nodePackages.npm

            # Backend runtime deps
            pkgs.pkg-config
            pkgs.openssl

            # Docker (optional — uses host docker)
            pkgs.docker-compose

            # Dev tools
            pkgs.just
            pkgs.jq
            pkgs.curl
          ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
            pkgs.apple-sdk_15
          ];

          shellHook = ''
            echo "🏗  Dedaliano dev shell ready"
            echo "   make dev        — start backend + web"
            echo "   make test       — run all tests"
            echo "   make docker-up  — start via docker compose"
          '';

          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
        };

        # Verification tooling: Lean (elan reads verify/lean-toolchain), Kani, SAT solvers.
        # Kani is not in nixpkgs: scripts/kani-setup.sh installs it into ./.tools, and it
        # keeps its own nightly toolchain and CBMC under ./.tools/kani (KANI_HOME).
        devShells.verify = pkgs.mkShell {
          buildInputs = [
            rustToolchain
            pkgs.elan
            pkgs.cbmc
            pkgs.kissat
            pkgs.cadical
            pkgs.git
            pkgs.curl
            pkgs.jq
          ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
            pkgs.apple-sdk_15
          ];

          shellHook = ''
            root=$(git rev-parse --show-toplevel)
            export ELAN_HOME="$root/.tools/elan"
            export KANI_HOME="$root/.tools/kani"
            export PATH="$root/.tools/bin:$ELAN_HOME/bin:$PATH"
          '';
        };
      }
    );
}
