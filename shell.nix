{ pkgs ? import <nixpkgs> {} }:
with pkgs;
mkShell {
  buildInputs = [ gcc cmake libopus yt-dlp ];

  DISCORD_TOKEN = (builtins.readFile ./discord_token);
}
