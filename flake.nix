{
  inputs = {
    nixpkgs.url = "nixpkgs";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    flake-utils.lib.eachSystem
      [
        "x86_64-linux"
        "aarch64-linux"
      ]
      (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          pkgsStatic = pkgs.pkgsStatic;

          mkBrief =
            pkgs:
            (pkgs.rustPlatform.buildRustPackage {
              pname = "brief-cli";
              version = "0.1.0";

              cargoSha256 = "sha256-Ntwc9tyvUPCYxCnmeX1n+EUHYoGI81mgdZDc1FluP1s=";
              src = ./brief-cli;
            });

        in
        {
          formatter = pkgs.nixfmt-rfc-style;

          packages = rec {
            default = brief;
            brief = mkBrief pkgs;
            brief-static = mkBrief pkgsStatic;
          };

          devShells = rec {
            default = brief-cli;

            brief-cli = pkgs.mkShell {
              nativeBuildInputs = with pkgs; [
                cargo
                rustc
              ];

              packages = with pkgs; [
                clippy
                rustfmt
                rust-analyzer
              ];

              shellHook = ''
                export RUST_SRC_PATH=${pkgs.rustPlatform.rustLibSrc}
              '';
            };
          };
        }
      );
}
