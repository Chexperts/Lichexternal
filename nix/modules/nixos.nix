# NixOS module for lichexternald
self:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  inherit (lib)
    mkEnableOption
    mkOption
    mkIf
    types
    ;
  cfg = config.services.lichexternald;
  tomlFormat = pkgs.formats.toml { };
in
{
  options.services.lichexternald = {
    enable = mkEnableOption "Lichexternal background bridge daemon";

    package = mkOption {
      type = types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      description = "The lichexternal package to use.";
    };

    configFile = mkOption {
      type = types.nullOr types.path;
      default = null;
      description = "Path to the lichexternal configuration TOML file.";
    };

    settings = mkOption {
      type = types.nullOr (types.attrsOf types.anything);
      default = null;
      description = "Structured configuration converted to TOML. Mutually exclusive with configFile.";
    };
  };

  config = mkIf cfg.enable {
    systemd.services.lichexternald = {
      description = "Lichess External Engine Daemon";
      after = [ "network-online.target" ];
      wants = [ "network-online.target" ];
      wantedBy = [ "multi-user.target" ];

      serviceConfig = {
        ExecStart =
          if cfg.configFile != null then
            "${cfg.package}/bin/lichextd --config ${cfg.configFile}"
          else if cfg.settings != null then
            "${cfg.package}/bin/lichextd --config ${tomlFormat.generate "config.toml" cfg.settings}"
          else
            "${cfg.package}/bin/lichextd";
        Restart = "on-failure";
        RestartSec = 5;
        DynamicUser = true;
        ProtectSystem = "strict";
        ProtectHome = "read-only";
        PrivateTmp = true;
        NoNewPrivileges = true;
      };
    };
  };
}
