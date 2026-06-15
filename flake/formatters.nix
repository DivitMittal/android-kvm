{inputs, ...}: {
  imports = [inputs.treefmt-nix.flakeModule];

  perSystem.treefmt = {
    projectRootFile = "flake.nix";
    flakeCheck = false;

    programs = {
      alejandra.enable = true;
      rustfmt.enable = true;
      taplo.enable = true;
    };
  };
}
