{
  description = "Rust example flake for Zero to Nix";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      # Systems supported
      allSystems = [
        "x86_64-linux" # 64-bit Intel/AMD Linux
        "aarch64-linux" # 64-bit ARM Linux

      ];

      # Helper to provide system-specific attributes
      forAllSystems = f: nixpkgs.lib.genAttrs allSystems (system: f {
        pkgs = import nixpkgs {
          inherit system;
        };
      });
    in
    {

      devShells = forAllSystems ({ pkgs }: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            openssl
            pkg-config
            glib
            gtk3
            rustc
            cargo
          ];
          #PKG_CONFIG_PATH = "${pkgs.openssl}/lib/pkgconfig";
        };
      });

      packages = forAllSystems ({ pkgs }: {
        default = 
        pkgs.rustPlatform.buildRustPackage rec {
            name = "tunes";
            src = ./.;
            cargoLock = {
              lockFile = ./Cargo.lock;
            };
            nativeBuildInputs = with pkgs; [ pkg-config ];
            buildInputs = with pkgs; [ libhandy rustc cargo openssl pkg-config makeWrapper glib gtk3 ];
            #PKG_CONFIG_PATH = "${pkgs.openssl}/lib/pkgconfig";
          };
      });
    };
}
