{
  inputs,
  lib,
  ...
}: {
  imports = [inputs.devshell.flakeModule];

  perSystem = {pkgs, ...}: {
    devshells.default = {
      devshell = {
        name = "android-kvm";
        motd = "{202}Welcome to {91}android-kvm {202}devshell!{reset} \n $(menu)";
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
