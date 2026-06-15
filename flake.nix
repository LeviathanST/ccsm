{
  description = "cc-tui — TUI Dashboard Wrapper for Claude Code";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            zig
            cargo
            rustc
            pkg-config
          ];

          shellHook = ''
            echo "🦀 cc-tui dev shell — zig $(zig version), rustc $(rustc --version)"
          '';
        };
      });
}
