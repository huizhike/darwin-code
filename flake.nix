{
  description = "Development Nix flake for Darwin Code CLI";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, rust-overlay, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems f;

      # Read the version from the workspace Cargo.toml (the single source of
      # truth used by the release workflow).
      cargoToml = builtins.fromTOML (builtins.readFile ./darwin-rs/Cargo.toml);
      cargoVersion = cargoToml.workspace.package.version;

      # Cargo.toml carries the Darwin Code package version used by the
      # release/install surfaces.
      version = cargoVersion;
    in
    {
      packages = forAllSystems (system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };
          darwin-rs = pkgs.callPackage ./darwin-rs {
            inherit version;
            rustPlatform = pkgs.makeRustPlatform {
              cargo = pkgs.rust-bin.stable.latest.minimal;
              rustc = pkgs.rust-bin.stable.latest.minimal;
            };
          };
        in
        {
          darwin-rs = darwin-rs;
          default = darwin-rs;
        }
      );

      devShells = forAllSystems (system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };
          rust = pkgs.rust-bin.stable.latest.default.override {
            extensions = [ "rust-src" "rust-analyzer" ];
          };
        in
        {
          default = pkgs.mkShell {
            buildInputs = [
              rust
              pkgs.pkg-config
              pkgs.openssl
              pkgs.cmake
              pkgs.llvmPackages.clang
              pkgs.llvmPackages.libclang.lib
            ];
            PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
            LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
            # Use clang for BoringSSL compilation (avoids GCC 15 warnings-as-errors)
            shellHook = ''
              export CC=clang
              export CXX=clang++
            '';
          };
        }
      );
    };
}
