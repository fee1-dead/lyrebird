{ pkgs ? import <nixpkgs> {} }:
with pkgs;
mkShell {
  buildInputs = [ gcc cmake libopus ffmpeg youtube-dl ];
}
