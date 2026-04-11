{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
  };

  outputs = {
    nixpkgs,
    fenix,
    crane,
    flake-utils,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = nixpkgs.legacyPackages.${system};

      pname = "corona-ng";
      version = "0.1.0";

      rust-toolchain = with fenix.packages.${system};
        combine [
          stable.rustc
          stable.cargo
          stable.rust-src
          stable.rust-analyzer
        ];

      craneLib = (crane.mkLib pkgs).overrideToolchain rust-toolchain;

      commonArgs = {
        inherit pname version;
        src = craneLib.cleanCargoSource (craneLib.path ./.);

        nativeBuildInputs = with pkgs; [
          rust-toolchain
          lld
        ];

        buildInputs = with pkgs;
          [
            openssl
          ]
          ++ lib.optionals stdenv.isDarwin [
            darwin.apple_sdk.frameworks.Security
          ];
      };

      cargoArtifacts = craneLib.buildDepsOnly commonArgs;
    in {
      packages.default = craneLib.buildPackage (commonArgs
        // {
          inherit cargoArtifacts;
          RUSTFLAGS = "-C link-arg=-fuse-ld=lld";

          meta = with pkgs.lib; {
            description = "CoronaNG sign up helper";
            license = licenses.mit;
          };
        });

      devShells.default = pkgs.mkShell {
        nativeBuildInputs =
          commonArgs.nativeBuildInputs
          ++ (with pkgs; [
            cargo-watch
            cargo-tarpaulin
          ]);

        inherit (commonArgs) buildInputs;

        shellHook = ''
          export RUST_SRC_PATH=${fenix.packages.${system}.stable.rust-src}/lib/rustlib/src/rust/library
        '';
      };

      formatter = pkgs.nixpkgs-fmt;
    });
}
