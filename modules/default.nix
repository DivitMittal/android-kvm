{inputs, ...}: {
  flake.homeManagerModules = {
    default = {
      imports = [(inputs.import-tree ./home)];
    };

    android-kvm = import ./home/android-kvm.nix;
  };
}
