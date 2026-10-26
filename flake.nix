{
  description = "Lichexternal: Cross-platform asynchronous service bridging Lichess External Engine API with chess engines";

  inputs = {
    nixpkgs.url = "https://channels.nixos.org/nixos-unstable/nixexprs.tar.zst";

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
        inputs.flake-parts.flakeModules.partitions
        inputs.git-hooks.flakeModule
      ];

      partitionedAttrs.checks = "dev";

      partitions.dev = {
        extraInputsFlake = ./dev;
        module =
          { inputs, ... }:
          {
            perSystem =
              {
                pkgs,
                lib,
                system,
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
              in
              {
                checks = {
                  cargo-test = craneLib.cargoTest (
                    commonArgs
                    // {
                      inherit cargoArtifacts;
                      LICHESS_OPENAPI_SPEC = "${inputs.lichess-api}/doc/specs/lichess-api.yaml";
                    }
                  );
                  cargo-clippy = craneLib.cargoClippy (
                    commonArgs
                    // {
                      inherit cargoArtifacts;
                      cargoClippyExtraArgs = "--all-targets --all-features -- --deny warnings";
                    }
                  );
                };
              };
          };
      };

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

          crateInfo = craneLib.crateNameFromCargoToml { cargoToml = ./Cargo.toml; };
          inherit (crateInfo) version;

          commonArgs = {
            inherit src;
            strictDeps = true;
            buildInputs = lib.optionals pkgs.stdenv.hostPlatform.isDarwin [
              pkgs.darwin.apple_sdk.frameworks.Security
              pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
            ];
          };

          cargoArtifacts = craneLib.buildDepsOnly commonArgs;

          lichexternal = craneLib.buildPackage (
            commonArgs
            // {
              inherit cargoArtifacts version;
              pname = "lichexternal";
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
                inherit version;
              }
            )
          );

          typescriptBindings = craneLib.mkCargoDerivation (
            commonArgs
            // {
              inherit cargoArtifacts version;
              pname = "lichexternal-typescript-bindings";
              doInstallCargoArtifacts = false;
              buildPhaseCargoCommand = ''
                                mkdir -p "$out"
                                export TS_RS_EXPORT_DIR="$out"
                                cargo test -p lichexternal-core --features typescript --lib

                                cat << 'EOF' > "$out/index.ts"
                export * from "./AcquireRequest";
                export * from "./AcquireResponse";
                export * from "./CreateEngineRequest";
                export * from "./EngineMetadata";
                export * from "./EngineProfile";
                export * from "./SearchLimit";
                export * from "./UciVariant";
                export * from "./UpdateEngineRequest";
                export * from "./WorkRequest";
                EOF

                                cat << EOF > "$out/package.json"
                {
                  "name": "@chexperts/lichexternal-types",
                  "version": "${version}",
                  "description": "TypeScript type definitions for Lichess External Engine bridge protocols",
                  "main": "index.ts",
                  "types": "index.ts",
                  "repository": {
                    "type": "git",
                    "url": "git+https://github.com/Chexperts/Lichexternal.git"
                  },
                  "license": "AGPL-3.0-or-later"
                }
                EOF
              '';
            }
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

          apps = {
            default = {
              type = "app";
              program = lib.getExe' lichexternal "lichext";
            };
            lichext = {
              type = "app";
              program = lib.getExe' lichexternal "lichext";
            };
            lichextd = {
              type = "app";
              program = lib.getExe' lichexternal "lichextd";
            };
          };

          packages = {
            inherit lichexternal;
            default = lichexternal;
            typescript-bindings = typescriptBindings;
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
