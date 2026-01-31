{
  description = "math stuff";

  inputs.flake-utils.url = "github:numtide/flake-utils";

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem
      (system:
        let pkgs = nixpkgs.legacyPackages.${system}; in
        {
          devShells.default = with pkgs;
            mkShell {
              buildInputs = [ gcc pkg-config cmake libopus yt-dlp glibc ];
            };
        }
      );
}