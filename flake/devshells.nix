{
  inputs,
  lib,
  ...
}: {
  imports = [inputs.devshell.flakeModule];

  perSystem = {
    pkgs,
    config,
    ...
  }: {
    devshells.default = {
      devshell = rec {
        name = "android-kvm";
        motd = "{202}Welcome to {91}${name} {202}devshell!{reset} \n $(menu)";
        startup = {
          git-hooks.text = ''
            ${config.pre-commit.installationScript}
          '';
        };
        packages =
          lib.attrsets.attrValues {
            inherit
              (pkgs)
              android-tools
              apm-cli
              cargo
              clippy
              pkg-config
              rustc
              rust-analyzer
              rustfmt
              scrcpy
              ;
          }
          ++ lib.optionals pkgs.stdenv.isLinux [
            pkgs.libX11
            pkgs.libXtst
          ];
      };
    };
  };
}
