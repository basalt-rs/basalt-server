{
  self,
  inputs,
  lib,
  ...
}:
{
  perSystem =
    { pkgs, ... }:
    let
      craneLib = inputs.crane.mkLib pkgs;
      src = craneLib.cleanCargoSource ../../.;

      # Common arguments can be set here to avoid repeating them later
      commonArgs = {
        inherit src;
        strictDeps = true;

        nativeBuildInputs = [
          pkgs.pkg-config
        ];

        buildInputs = [
          pkgs.openssl
        ]
        ++ lib.optionals pkgs.stdenv.isDarwin [
          # Additional darwin specific inputs can be set here
          pkgs.libiconv
        ];

        # Additional environment variables can be set directly
        # MY_CUSTOM_VAR = "some value";
      };

      # Build *just* the cargo dependencies (of the entire workspace),
      # so we can reuse all of that work (e.g. via cachix) when running in CI
      # It is *highly* recommended to use something like cargo-hakari to avoid
      # cache misses when building individual top-level-crates
      cargoArtifacts = craneLib.buildDepsOnly commonArgs;

      individualCrateArgs = commonArgs // {
        inherit cargoArtifacts;
        inherit (craneLib.crateNameFromCargoToml { inherit src; }) version;
        # NB: we disable tests since we'll run them all via cargo-nextest
        doCheck = false;
      };

      fileSetForCrate = lib.fileset.toSource {
        root = ../../.;
        fileset = lib.fileset.unions [
          ../../Cargo.toml
          ../../Cargo.lock
          ../../basalt-server-lib/migration.sql
          (craneLib.fileset.commonCargoSources ../../basalt-server)
          (craneLib.fileset.commonCargoSources ../../basalt-server-lib)
        ];
      };
    in
    {
      packages.basalt-server = craneLib.buildPackage (
        individualCrateArgs
        // {
          pname = "basalt-server";
          cargoExtraArgs = "-p basalt-server --no-default-features";
          src = fileSetForCrate;
        }
      );

    };
}
