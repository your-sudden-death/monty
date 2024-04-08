{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

        libraries = with pkgs; [
          webkitgtk
          gtk3
          cairo
          gdk-pixbuf
          fuse3
          libxkbcommon
          glib
          dbus
          sqlite
          xorg.libXcursor
          fontconfig
          freetype
          xorg.libXrandr
          xorg.libXi
          xorg.libX11
          openssl_3
          librsvg
          libclang
          lm_sensors
          vulkan-headers
          vulkan-loader
          wayland
          llvmPackages_15.libllvm

          gst_all_1.gstreamer
          # Common plugins like "filesrc" to combine within e.g. gst-launch
          gst_all_1.gst-plugins-base
          # Specialized plugins separated by quality
          gst_all_1.gst-plugins-good
          gst_all_1.gst-plugins-bad
          gst_all_1.gst-plugins-ugly
          gst_all_1.gst-plugins-rs
          # Plugins to reuse ffmpeg to play almost every video format
          gst_all_1.gst-libav
          # Support the Video Audio (Hardware) Acceleration API
          gst_all_1.gst-vaapi
        ];

        packages = with pkgs; [
          curl
          protobuf
          wget
          wayland
          pkg-config
          fontconfig
          freetype
          dbus
          openssl_3
          python3
          fuse3
          glib
          gtk3
          vulkan-headers
          vulkan-loader
          sqlite
          lm_sensors
          libsoup
          clang
          sass
          librsvg
          (rust-bin.beta.latest.default.override {
            extensions = [ "rust-src" "rust-analyzer" ];
          })

          gst_all_1.gstreamer
          # Common plugins like "filesrc" to combine within e.g. gst-launch
          gst_all_1.gst-plugins-base
          # Specialized plugins separated by quality
          gst_all_1.gst-plugins-good
          gst_all_1.gst-plugins-bad
          gst_all_1.gst-plugins-ugly
          gst_all_1.gst-plugins-rs
          # Plugins to reuse ffmpeg to play almost every video format
          gst_all_1.gst-libav
          # Support the Video Audio (Hardware) Acceleration API
          gst_all_1.gst-vaapi
        ];
      in {
        devShell = pkgs.mkShell {
          buildInputs = packages;

          shellHook = ''
            export PATH="$PATH":"$HOME/.cargo/bin"
            export LD_LIBRARY_PATH=${
              pkgs.lib.makeLibraryPath libraries
            }:$LD_LIBRARY_PATH

            export LIBRARY_PATH=${
              pkgs.lib.makeLibraryPath libraries
            }:$LD_LIBRARY_PATH

          '';
        };
      });
}
