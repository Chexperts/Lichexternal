{
  description = "Lichexternal: Cross-platform asynchronous service bridging Lichess External Engine API with chess engines";

  inputs = {
    nixpkgs.url = "https://channels.nixos.org/nixos-unstable/nixexprs.tar.xz";

    systems.url = "github:nix-systems/default";

    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };

    git-hooks = {
      url = "github:cachix/git-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    crane.url = "github:ipetkov/crane";
  };

  outputs =
    inputs@{ flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [
        inputs.git-hooks.flakeModule
      ];

      systems = import inputs.systems;

      flake = {
        nixosModules.default = import ./nix/modules/nixos.nix inputs.self;
        homeManagerModules.default = import ./nix/modules/home-manager.nix inputs.self;
      };

      perSystem =
        {
          config,
          system,
          pkgs,
          lib,
          ...
        }:
        let
          fenixPkg = inputs.fenix.packages.${system};

          rustToolchain = fenixPkg.combine [
            fenixPkg.stable.toolchain
            fenixPkg.targets.wasm32-unknown-unknown.stable.rust-std
            fenixPkg.targets.x86_64-pc-windows-gnu.stable.rust-std
          ];

          craneLib = (inputs.crane.mkLib pkgs).overrideToolchain rustToolchain;

          src = craneLib.cleanCargoSource ./.;

          commonArgs = {
            inherit src;
            strictDeps = true;
            buildInputs = lib.optionals pkgs.stdenv.hostPlatform.isDarwin [
              pkgs.darwin.apple_sdk.frameworks.Security
              pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
            ];
          };

          cargoArtifacts = craneLib.buildDepsOnly commonArgs;

          aliases = {
            lichext = "lichexternal";
            lichextd = "lichexternald";
          };

          lichexternal = craneLib.buildPackage (
            commonArgs
            // {
              inherit cargoArtifacts;
              pname = "lichexternal";
              version = "0.1.0";
              postInstall = lib.concatLines (
                lib.mapAttrsToList (alias: target: "ln -s ${target} $out/bin/${alias}") aliases
              );
            }
          );

          # Windows cross-compilation package via MinGW (Linux hosts only)
          windowsPackage = lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux (
            let
              mingwPkgs = pkgs.pkgsCross.mingwW64;
              craneLibWindows = (inputs.crane.mkLib mingwPkgs).overrideToolchain (_: rustToolchain);
              windowsArtifacts = craneLibWindows.buildDepsOnly (
                commonArgs
                // {
                  CARGO_BUILD_TARGET = "x86_64-pc-windows-gnu";
                  nativeBuildInputs = [ mingwPkgs.buildPackages.stdenv.cc ];
                  buildInputs = [ mingwPkgs.windows.pthreads ];
                }
              );
            in
            craneLibWindows.buildPackage (
              commonArgs
              // {
                cargoArtifacts = windowsArtifacts;
                CARGO_BUILD_TARGET = "x86_64-pc-windows-gnu";
                nativeBuildInputs = [ mingwPkgs.buildPackages.stdenv.cc ];
                buildInputs = [ mingwPkgs.windows.pthreads ];
                doCheck = false;
                pname = "lichexternal-windows";
                version = "0.1.0";
              }
            )
          );
        in
        {
          pre-commit.settings.hooks = {
            nixfmt.enable = true;
            deadnix.enable = true;
            statix.enable = true;
            actionlint.enable = true;
            rustfmt.enable = true;
          };

          # Self-correcting formatter script (runs hooks and handles clean 2nd-pass)
          formatter =
            let
              cfg = config.pre-commit.settings;
            in
            pkgs.writeShellScriptBin "pre-commit-fmt" ''
              set -euo pipefail
              export PATH="${
                pkgs.lib.makeBinPath (
                  [
                    cfg.gitPackage
                    cfg.package
                  ]
                  ++ cfg.enabledPackages
                )
              }:$PATH"

              exitcode=0
              if [ "$#" -gt 0 ]; then
                ${pkgs.lib.getExe cfg.package} run -c ${cfg.configFile} --files "$@" || exitcode=$?
              else
                if [ -n "''${PRJ_ROOT:-}" ]; then
                  cd "$PRJ_ROOT"
                fi
                ${pkgs.lib.getExe cfg.package} run -c ${cfg.configFile} --all-files || exitcode=$?
              fi

              if [ "$exitcode" -eq 1 ]; then
                if [ "$#" -gt 0 ]; then
                  ${pkgs.lib.getExe cfg.package} run -c ${cfg.configFile} --files "$@"
                else
                  ${pkgs.lib.getExe cfg.package} run -c ${cfg.configFile} --all-files
                fi
              else
                exit "$exitcode"
              fi
            '';

          apps = lib.genAttrs ([ "lichexternald" ] ++ (lib.attrNames aliases)) (name: {
            type = "app";
            program = lib.getExe' lichexternal name;
          });

          packages = {
            inherit lichexternal;
            default = lichexternal;
          }
          // (lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux { windows = windowsPackage; });

          devShells.default = pkgs.mkShell {
            inputsFrom = [
              config.pre-commit.devShell
              lichexternal
            ];
            packages = with pkgs; [
              rustToolchain
              gh
            ];
          };
        };
    };
}
