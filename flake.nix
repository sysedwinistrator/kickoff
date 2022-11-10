{
  description = "Minimalistic program launcher";

  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    naersk.url = "github:nix-community/naersk";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs = {
    self,
    flake-utils,
    naersk,
    nixpkgs,
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        pkgs = (import nixpkgs) {
          inherit system;
        };

        naersk' = pkgs.callPackage naersk {};
      in rec {
        # For `nix build` & `nix run`:
        defaultPackage = naersk'.buildPackage {
          src = ./.;
          nativeBuildInputs = with pkgs; [cmake pkg-config];
          buildInputs = with pkgs; [freetype fontconfig wayland];
          postInstall = ''
            ${pkgs.patchelf} --rpath ${pkgs.lib.makeLibraryPath buildInputs} $out/bin/kickoff
          '';
        };

        # For `nix develop` (optional, can be skipped):
        devShell = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [rustc cargo];
        };
      }
    );
}
