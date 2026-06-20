{inputs, ...}: {
  imports = [inputs.treefmt-nix.flakeModule];

  perSystem.treefmt = {
    projectRootFile = "flake.nix";
    settings.global = {
      excludes = [
        ".github/*"
      ];
    };

    flakeCheck = false;

    programs = {
      ## Nix
      alejandra.enable = true;
      deadnix.enable = true;
      statix.enable = true;
      ## Rust
      rustfmt.enable = true;
      ## TOML
      taplo.enable = true;
      ## Markdown
      mdformat.enable = true;
    };
  };
}
