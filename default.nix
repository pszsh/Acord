{ pkgs ? import <nixpkgs> {} }:

let 
acord = pkgs.rustPlatform.buildRustPackage rec {
    pname = "acord";
    version = "0.1.0";

    src = ./.; # Or fetchFromGitHub

# The hash of the cargo dependencies
        cargoHash = "sha256-Tx5dHEnpbs2MmoGKRlXeRhZRWWbDaXYnwjX+oo37s8E=";
    cargoBuildFlags = [ "-p" "acord-linux" ];
# Required for consistency if your tests also need the -p flag
    cargoCheckFlags = [ "-p" "acord-linux" ];



# Optionally add native build inputs (e.g., pkg-config, cmake)
    nativeBuildInputs = with pkgs; [ pkg-config ];
    buildInputs = with pkgs; [ openssl ];
};
in 
pkgs.writeShellScript "acord" ''
exec ${pkgs.steam-run}/bin/steam-run ${acord}/bin/acord "$@"
''


