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
  tomlFormat = pkgs.formats.toml { };
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

    xdg.configFile."lichexternal/config.toml" = mkIf (cfg.settings != null) {
      source = tomlFormat.generate "lichexternal-config.toml" cfg.settings;
    };

    systemd.user.services.lichexternald = mkIf pkgs.stdenv.hostPlatform.isLinux {
      Unit = {
        Description = "Lichess External Engine Daemon";
        After = [ "network-online.target" ];
      };
      Service = {
        ExecStart = "${cfg.package}/bin/lichextd";
        Restart = "on-failure";
        RestartSec = 5;
      };
      Install = {
        WantedBy = [ "default.target" ];
      };
    };

    launchd.user.agents.lichexternald = mkIf pkgs.stdenv.hostPlatform.isDarwin {
      enable = true;
      config = {
        ProgramArguments = [ "${cfg.package}/bin/lichextd" ];
        KeepAlive = true;
        RunAtLoad = true;
        StandardOutPath = "${config.home.homeDirectory}/Library/Logs/lichexternald.log";
        StandardErrorPath = "${config.home.homeDirectory}/Library/Logs/lichexternald.err.log";
      };
    };
  };
}
