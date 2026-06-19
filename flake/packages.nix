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
    };
  };
}
