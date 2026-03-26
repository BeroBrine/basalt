{
  description = "Basalt Rust project";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs =
    {
      self,
      nixpkgs,
      utils,
      rust-overlay,
    }:
    utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        # Full Rust toolchain including rust-analyzer + rust-src
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [
            "rust-src"
            "rust-analyzer"
          ];
          # targets = [ "aarch64-unknown-linux-gnu" ]; # add if you need cross-compilation
        };
      in
      {
        # Keep your naersk build if you still want it
        packages.default = (pkgs.callPackage ./naersk.nix { }).buildPackage ./.;

        devShells.default = pkgs.mkShell {
          buildInputs = [
            rustToolchain # ← This brings cargo, rustc, rustfmt, clippy, rust-analyzer
            pkgs.pre-commit
            # Add any extra native libraries your project needs here, e.g.:
            # pkgs.openssl pkgs.pkg-config
          ];

          # Very important for rust-analyzer
          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";

          shellHook = ''
            echo "✅ Rust dev shell loaded with rust-analyzer from Nix"
          '';
        };
      }
    );
}
