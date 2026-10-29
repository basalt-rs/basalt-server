{ self, ... }:
{
  perSystem =
    {
      pkgs,
      self',
      ...
    }:
    {
      checks = {
        # Build the server package as a check
        basalt-server-build = self'.packages.basalt-server;

        # Formatting check for rust files
        rust-fmt = pkgs.stdenvNoCC.mkDerivation {
          name = "rust-format-check";
          src = self;
          nativeBuildInputs = [
            pkgs.cargo
            pkgs.findutils
            pkgs.rustfmt
          ];
          buildPhase = ''
            cd "$src"
            cargo fmt --check --all
          '';
          installPhase = ''
            mkdir -p $out
          '';
        };
      };
    };
}
