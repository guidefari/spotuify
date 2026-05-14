{
  description = "spotuify — Spotify TUI/CLI/MCP for keyboard-native power users";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        # Pinned to whatever the workspace's rust-toolchain says.
        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

        linuxDeps = with pkgs; [
          alsa-lib
          dbus
          libpulseaudio
          pkg-config
          # PipeWire ALSA-compat shim picks this up at runtime; no
          # build-time dep needed.
        ];

      in {
        # Default package: build spotuify from source. Linux uses the
        # alsa audio backend (which routes through pipewire-alsa on
        # modern distros). macOS uses portaudio. Other platforms can
        # override `features` via overrideAttrs.
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "spotuify";
          version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).workspace.package.version or "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = pkgs.lib.optionals pkgs.stdenv.isLinux (linuxDeps ++ [ pkgs.cmake ]);
          buildInputs = pkgs.lib.optionals pkgs.stdenv.isLinux linuxDeps;

          # No embedded-playback by default — keeps cold build time
          # reasonable. Power users who want it: `nix build .#withEmbedded`.
          buildNoDefaultFeatures = false;
          buildFeatures = [];

          # Spotuify ships an extensive doc + recipe tree; don't
          # ship them in the runtime closure.
          postInstall = ''
            mkdir -p $out/share/spotuify/install
            cp -r install $out/share/spotuify/
            mkdir -p $out/share/spotuify/recipes
            cp -r docs/recipes/* $out/share/spotuify/recipes/ || true
          '';

          meta = with pkgs.lib; {
            description = "Spotify TUI/CLI/MCP for keyboard-native power users";
            homepage = "https://github.com/planetaryescape/spotuify";
            license = licenses.mit;
            platforms = platforms.unix ++ platforms.windows;
          };
        };

        # Dev shell — `nix develop` to get the same toolchain CI uses.
        devShells.default = pkgs.mkShell {
          nativeBuildInputs = [ rustToolchain pkgs.cargo-deb pkgs.cargo-nextest pkgs.cargo-watch ]
            ++ pkgs.lib.optionals pkgs.stdenv.isLinux linuxDeps;
          shellHook = ''
            export RUST_LOG=info
            echo "spotuify dev shell — rust $(rustc --version)"
          '';
        };

        apps.default = flake-utils.lib.mkApp {
          drv = self.packages.${system}.default;
          name = "spotuify";
        };
      });
}
