{
  description = "remote-view";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, rust-overlay }:
    let
      systems = [ "aarch64-darwin" "x86_64-darwin" "aarch64-linux" "x86_64-linux" ];
      forAll = f: nixpkgs.lib.genAttrs systems (s: f (import nixpkgs {
        system = s;
        overlays = [ rust-overlay.overlays.default ];
      }));
    in
    {
      devShells = forAll (pkgs:
        let
          toolchain = pkgs.rust-bin.stable.latest.default.override {
            targets = [ "wasm32-unknown-unknown" ];
            extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" ];
          };
        in
        {
          default = pkgs.mkShell {
            packages = [
              toolchain
              pkgs.cargo-leptos
              pkgs.wasm-bindgen-cli
              pkgs.binaryen
              pkgs.dart-sass
              pkgs.pkg-config
            ];
          };
        });

      formatter = forAll (pkgs: pkgs.nixpkgs-fmt);
    };
}
