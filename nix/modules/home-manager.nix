# Home Manager module for lichexternald
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
in
{
  options.services.lichexternald = {
    enable = mkEnableOption "Lichexternal user service daemon";
    package = mkOption {
      type = types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      description = "The lichexternal package to use.";
    };
    settings = mkOption {
      type = types.nullOr (types.attrsOf types.anything);
      default = null;
      description = "Configuration settings written to ~/.config/lichexternal/config.toml";
    };
  };

  config = mkIf cfg.enable {
    home.packages = [ cfg.package ];

    systemd.user.services.lichexternald = {
      Unit = {
        Description = "Lichess External Engine Daemon";
        After = [ "network-online.target" ];
      };
      Service = {
        ExecStart = "${cfg.package}/bin/lichexternald";
        Restart = "on-failure";
        RestartSec = 5;
      };
      Install = {
        WantedBy = [ "default.target" ];
      };
    };
  };
}
