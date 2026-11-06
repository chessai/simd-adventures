{
  description = "SIMD adventures development environment";

  inputs = {
    # particular nixos-unstable revision
    nixpkgs.url = "github:NixOS/nixpkgs/9f0c42f8bc7151b8e7e5840fb3bd454ad850d8c5";
    flake-utils.url = "github:numtide/flake-utils";

    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, fenix }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          config = {};
          inherit system;
        };

        fenixPkgs = fenix.packages.${system};

        full-fenix = fenixPkgs.combine [
          # nightly toolchain (for std::simd / portable_simd)
          (fenixPkgs.complete.withComponents [
            "cargo"
            "clippy"
            "rust-src"
            "rustc"
            "rustfmt"
          ])
          # aarch64 std, for cross-compiling NEON code
          fenixPkgs.targets.aarch64-unknown-linux-gnu.latest.rust-std
        ];

        # Python for pngsimd/scripts (corpus generation, summaries)
        pythonEnv = pkgs.python3.withPackages (ps: with ps; [
          numpy
          pillow
          scikit-image
        ]);

        # pngsimd test/benchmark corpus, generated from scikit-image's sample images.
        pngsimd-corpus = pkgs.runCommand "pngsimd-corpus" { nativeBuildInputs = [ pythonEnv ]; } ''
          mkdir -p work/scripts
          cp ${./pngsimd/scripts/make_corpus.py} work/scripts/make_corpus.py
          python3 work/scripts/make_corpus.py
          cp -r work/corpus $out
        '';

        # Cross linker + QEMU user-mode runner for aarch64. `-cpu max` enables
        # every extension QEMU emulates (NEON, SVE/SVE2, ...).
        aarch64Env = {
          CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER =
            "${pkgs.pkgsCross.aarch64-multiplatform.stdenv.cc}/bin/aarch64-unknown-linux-gnu-gcc";
          CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUNNER =
            "${pkgs.qemu-user}/bin/qemu-aarch64 -cpu max";
        };

        # Runs a cargo command in the sandbox against vendored deps.
        cargoCheck = name: command: env:
          pkgs.stdenv.mkDerivation ({
            name = "simd-adventures-${name}";
            src = self;
            nativeBuildInputs = [ full-fenix ];
            corpus = pngsimd-corpus;
            cargoDeps = pkgs.rustPlatform.importCargoLock { lockFile = ./Cargo.lock; };

            buildPhase = ''
              export CARGO_HOME="$TMPDIR/cargo-home"
              mkdir -p "$CARGO_HOME"
              cat > "$CARGO_HOME/config.toml" <<EOF
              [source.crates-io]
              replace-with = "vendored-sources"
              [source.vendored-sources]
              directory = "$cargoDeps"
              EOF

              # generated corpus for pngsimd/tests/png_corpus.rs
              cp -f "$corpus"/* pngsimd/corpus/

              ${command}
            '';
            installPhase = "touch $out";
          } // env);
      in
      {
        packages.default = fenixPkgs.minimal.toolchain;
        packages.pngsimd-corpus = pngsimd-corpus;

        checks = {
          native = cargoCheck "native" "cargo test --release --offline --workspace" {};
          # Every target with every feature (feature-gated comparisons and AVX-512 code). All
          # backends are always compiled, so this also covers ones the build machine cannot run.
          all-features = cargoCheck "all-features"
            "cargo build --release --offline --workspace --all-features --all-targets" {
              nativeBuildInputs = [ full-fenix pkgs.pkg-config ];
              buildInputs = [ pkgs.zlib pkgs.libpng pkgs.openssl ];
            };
        } // pkgs.lib.optionalAttrs (system == "x86_64-linux") {
          aarch64-neon = cargoCheck "aarch64-neon"
            "cargo test --release --offline --workspace --target aarch64-unknown-linux-gnu" aarch64Env;
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            # general dev stuff
            pkg-config
            clang
            llvm
            libclang.lib
            gcc
            cmake
            gnumake

            # rust
            fenixPkgs.rust-analyzer
            full-fenix

            # optional comparison features (pngsimd: zlib, libpng; vcodec: openssl), scripts
            zlib
            libpng
            openssl
            pythonEnv
            valgrind

            # vcodec/fuzz (libFuzzer targets)
            cargo-fuzz

            # aarch64 cross-testing (see aarch64Env)
            pkgsCross.aarch64-multiplatform.stdenv.cc
            qemu-user

            # inspecting codegen / profiling
            cargo-show-asm
            perf
          ];

          inherit (aarch64Env)
            CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER
            CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUNNER;

          shellHook = ''
            # Set up clang/libclang properly
            export LIBCLANG_PATH="${pkgs.libclang.lib}/lib"
            export CLANG_PATH="${pkgs.clang}/bin/clang"
            export BINDGEN_EXTRA_CLANG_ARGS="-I${pkgs.glibc.dev}/include -I${pkgs.libclang.lib}/lib/clang/*/include"

            # C++ compilation environment
            export CC="${pkgs.gcc}/bin/gcc"
            export CXX="${pkgs.gcc}/bin/g++"

            # Runtime paths for the system libraries the optional comparison features link
            # (zlib, libpng, openssl): the linker wrapper sets no runpath for them in a dev shell.
            export LD_LIBRARY_PATH="${pkgs.stdenv.cc.cc.lib}/lib:${pkgs.libclang.lib}/lib:${pkgs.lib.makeLibraryPath [ pkgs.zlib pkgs.libpng pkgs.openssl ]}:$LD_LIBRARY_PATH"

            if command -v rustc >/dev/null 2>&1; then
                echo "rustc version: $(rustc --version | head -n1)"
            else
                echo "Error: rustc not found in PATH"
                exit 1
            fi
          '';
        };
      }
    );
}
