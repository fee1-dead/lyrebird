{ pkgs ? import <nixpkgs> {} }:
with pkgs;
mkShell {
  buildInputs = [ gcc cmake libopus ffmpeg youtube-dl ];
  # DISCORD_TOKEN = "xxx"; # If you intend to add this token, please run `git update-index --skip-worktree shell.nix`.
  # If you make any changes, then you may run `git update-index --no-skip-worktree shell.nix`.
}
