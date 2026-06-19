{self, ...}: {
  perSystem = {pkgs, ...}: {
    packages.default = pkgs.rustPlatform.buildRustPackage {
      pname = "android-kvm";
      version = "0.1.0";
      src = self;
      cargoLock = {
        lockFile = ../Cargo.lock;
        outputHashes = {
          "input-capture-0.4.0" = "sha256-zCFYzIGWm5ShL/dO/SxwyHCCDerWveLiWpoeGDWKLi4=";
          "input-event-0.4.0" = "sha256-zCFYzIGWm5ShL/dO/SxwyHCCDerWveLiWpoeGDWKLi4=";
        };
      };
      nativeBuildInputs = [pkgs.pkg-config];
      buildInputs = pkgs.lib.optionals pkgs.stdenv.isLinux [
        pkgs.libX11
        pkgs.libXtst
      ];

      meta = {
        description = "USB Android software KVM backed by scrcpy";
        mainProgram = "android-kvm";
        platforms = pkgs.lib.platforms.all;
      };
    };
  };
}
