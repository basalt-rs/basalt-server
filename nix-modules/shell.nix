{ ... }:
{
  perSystem =
    { pkgs, ... }:
    {
      devShells.default = pkgs.mkShell {
        buildInputs = with pkgs; [
          # nix things
          nixfmt
          nil
          alejandra
          # rust things
          bacon
          cargo
          rust-analyzer
          rustc
          rustfmt
          clippy
          glibc
          openssl
          cargo-machete
          redocly
          # misc
          just
        ];
        nativeBuildInputs = [ pkgs.pkg-config ];
        env.RUST_SRC_PATH = "${pkgs.rust.packages.stable.rustPlatform.rustLibSrc}";
      };
    };
}
