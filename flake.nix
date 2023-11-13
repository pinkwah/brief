{
  inputs = {
    nixpkgs.url = "nixpkgs";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils, }:
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" ] (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        pkgsStatic = pkgs.pkgsStatic;
      in {
        formatter = pkgs.nixfmt;

        packages = rec {
          default = brief-cli;

          brief-cli = pkgsStatic.rustPlatform.buildRustPackage {
            pname = "brief-cli";
            version = "0.1.0";

            cargoSha256 = "sha256-GjKeDV+NxQDy1PhsSNvE34kxBdFiXzsTOkAvd/0SvbA=";

            src = ./brief-cli;
          };
        };

        devShells = rec {
          default = brief-cli;

          brief-cli = pkgs.mkShell {
            nativeBuildInputs = with pkgs; [ cargo rustc ];

            packages = with pkgs; [ clippy rustfmt ];

            shellHook = ''
              export RUST_SRC_PATH=${pkgs.rustPlatform.rustLibSrc}
            '';
          };
        };
      });
}
