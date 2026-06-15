{self, ...}: {
  perSystem = {pkgs, ...}: {
    packages.default = pkgs.rustPlatform.buildRustPackage {
      pname = "android-kvm";
      version = "0.1.0";
      src = self;
      cargoLock.lockFile = ../Cargo.lock;
      nativeBuildInputs = [pkgs.pkg-config];
    };
  };
}
