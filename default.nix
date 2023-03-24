{ pkgs ? import <nixpkgs> { system = builtins.currentSystem; }
, lib ? pkgs.lib
, fetchFromGitHub ? pkgs.fetchFromGitHub
, rustPlatform ? pkgs.rustPlatform
}:

rustPlatform.buildRustPackage rec {
  pname = "gh-backup";
  version = "0.0.1";

  src = fetchFromGitHub {
    owner = "maxammann";
    repo = pname;
    rev = "c78d1ab";
    hash = "sha256-zmz2cq+XR/ljd0oUQN7X3B3yPPUUbpVEQT0JhIJ5Qk0=";
  };

  nativeBuildInputs = [ pkgs.pkg-config ];

  buildInputs = [ pkgs.darwin.apple_sdk.frameworks.Security pkgs.openssl ];

  cargoHash = "sha256-uXLVIyXkmI1Ec/uXx3ZapZSXrOD4hpJ2SaazWUkBwY4=";

  meta = with lib; {
    description = "Blazingly fast tool to backup a Github organisation (written in Rust)";
    homepage = "https://github.com/maxammann/gh-backup";
    license = licenses.unlicense;
    maintainers = [ "Max Ammann" ];
  };
}