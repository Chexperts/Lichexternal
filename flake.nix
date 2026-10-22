{
  description = "Lichexternal: Cross-platform asynchronous service bridging Lichess External Engine API with chess engines";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      flake-utils,
      crane,
      ...
    }:
    let
      # Supported host systems for native evaluation
      supportedSystems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
        "x86_64-darwin"
      ];

      pkgsFor =
        system:
        import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };
    in
    nixpkgs.lib.recursiveUpdate
      (flake-utils.lib.eachSystem supportedSystems (
        system:
        let
          pkgs = pkgsFor system;

        inherit (pkgs) lib;

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          targets = [
            "wasm32-unknown-unknown"
            "x86_64-pc-windows-gnu"
            "x86_64-unknown-linux-gnu"
          ];
          extensions = [
            "rust-src"
            "rust-analyzer"
            "clippy"
          ];
        };

        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        src = craneLib.cleanCargoSource ./.;

        commonArgs = {
          inherit src;
          strictDeps = true;
          buildInputs =
            [ ]
            ++ lib.optionals pkgs.stdenv.isDarwin [
              pkgs.darwin.apple_sdk.frameworks.Security
              pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
            ];
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        lichexternal = craneLib.buildPackage (
          commonArgs
          // {
            inherit cargoArtifacts;
            pname = "lichexternal";
            version = "0.1.0";
          }
        );

        # Windows cross-compilation package via MinGW
        mingwPkgs = pkgs.pkgsCross.mingwW64;
        craneLibWindows = (crane.mkLib mingwPkgs).overrideToolchain rustToolchain;
        windowsArtifacts = craneLibWindows.buildDepsOnly (
          commonArgs
          // {
            CARGO_BUILD_TARGET = "x86_64-pc-windows-gnu";
            depsBuildBuild = [
              mingwPkgs.stdenv.cc
              mingwPkgs.windows.pthreads
            ];
            nativeBuildInputs = [ mingwPkgs.stdenv.cc ];
          }
        );
        windowsPackage = craneLibWindows.buildPackage (
          commonArgs
          // {
            cargoArtifacts = windowsArtifacts;
            CARGO_BUILD_TARGET = "x86_64-pc-windows-gnu";
            depsBuildBuild = [
              mingwPkgs.stdenv.cc
              mingwPkgs.windows.pthreads
            ];
            nativeBuildInputs = [ mingwPkgs.stdenv.cc ];
            doCheck = false;
            pname = "lichexternal-windows";
            version = "0.1.0";
          }
        );
      in
      {
        packages = {
          default = lichexternal;
          lichexternal = lichexternal;
          windows = windowsPackage;
        };

        devShells.default = pkgs.mkShell {
          inputsFrom = [ lichexternal ];
          packages = with pkgs; [
            rustToolchain
            gh
          ];
        };
      }))
      {
        # Target-indexed packages mapping
        packages = {
          x86_64-windows.default = self.packages.x86_64-linux.windows;
        };

        # NixOS & Home Manager modules
        nixosModules.default = import ./nix/modules/nixos.nix self;
        homeManagerModules.default = import ./nix/modules/home-manager.nix self;
      };
}
