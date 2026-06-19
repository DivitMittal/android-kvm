{
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.programs.android-kvm;
  toml = pkgs.formats.toml {};
in {
  options.programs.android-kvm = {
    enable = lib.mkEnableOption "android-kvm";

    package = lib.mkOption {
      type = lib.types.nullOr lib.types.package;
      default = pkgs.android-kvm or null;
      defaultText = lib.literalExpression "pkgs.android-kvm";
      description = "android-kvm package to install.";
    };

    settings = lib.mkOption {
      type = toml.type;
      default = {};
      example = {
        android-edge = "right";
        activation-pixels = 24;
        release-pixels = 4;
        audio-always-on = true;
        scrcpy = {
          audio-enabled = true;
          audio-buffer-ms = 200;
          keyboard = "uhid";
          mouse = "uhid";
        };
      };
      description = "Configuration written to android-kvm/config.toml.";
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion = cfg.package != null;
        message = "programs.android-kvm.package must be set when android-kvm is not available in pkgs.";
      }
    ];

    home.packages = [cfg.package];
    xdg.configFile."android-kvm/config.toml".source = toml.generate "android-kvm-config.toml" cfg.settings;
  };
}
