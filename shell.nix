{ pkgs ? import <nixpkgs> {} }:
with pkgs;
mkShell {
  buildInputs = [ gcc cmake libopus ffmpeg yt-dlp ];

  DISCORD_TOKEN = (builtins.readFile ./discord_token);
}
