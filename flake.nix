{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-23.11-small";
  inputs.flake-compat = { url = "github:edolstra/flake-compat"; flake = false; };

  outputs = { self, nixpkgs, flake-utils, }:
    let
      inherit (nixpkgs) lib;

      officialRelease = false;

      version = lib.fileContents ./.version + versionSuffix;
      versionSuffix =
        if officialRelease
          then ""
        else "pre${builtins.substring 0 8 (self.lastModifiedDate or self.lastModified or "19700101")}_${self.shortRev or "dirty"}";

      systems = [ "i686-linux" "x86_64-linux" "aarch64-linux" ];
      stdenvs = [
        "ccacheStdenv"
        "clangStdenv"
        "gccStdenv"
        "libcxxStdenv"
        "stdenv"
      ];

      forAllStdenvs = f:
        lib.listToAttrs
          (map
            (stdenvName: {
              name = "${stdenvName}Packages";
              value = f stdenvName;
            })
          stdenvs);

      binaryTarball = { nix, brief-cli, pkgs }: pkgs.callPackage ./scripts/binary-tarball.nix {
        inherit nix brief-cli;
      };

    in {
      hydraJobs = {
        build = forAllSystems (system: self.packages.${system}.brief-cli);

        shellInputs = forAllSystems (system: self.devShells.${system}.default.inputDerivation);

        binaryTarball = forAllSystems (system: binaryTarball {
          nix = nixpkgs.legacyPackages.${system}.nix;
          pkgs = nixpkgsFor.legacyPackages.${system}.native;
          brief = self.brief;
        });

        installerScript = installScriptFor [
          # Native
          self.hydraJobs.binaryTarball."x86_64-linux"
          self.hydraJobs.binaryTarball."i686-linux"
          self.hydraJobs.binaryTarball."aarch64-linux"
        ]
      };

      packages = forAllSystems (system: {
        brief-cli =
      });

      devShells = forAllSystems (system: {
        default = self.devShells.${system}.brief-cli;

        brief-cli =
      });
    };
}
