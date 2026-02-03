set shell := ["zsh", "-cu"]

root := justfile_directory()
ghostty_location := root + "/.tools/libghostty"

default:
  @just --list

# Clone Ghostty v1.1.2 and build libghostty (requires Zig 0.13.0 and Xcode)
setup-libghostty:
  #!/usr/bin/env zsh
  set -euo pipefail

  TOOLS="{{root}}/.tools"
  GHOSTTY_REPO="$TOOLS/ghostty"
  ZIG_VERSION="0.13.0"
  ZIG_DIR="$TOOLS/zig-$ZIG_VERSION"
  GHOSTTY_VERSION="v1.1.2"
  DEST="{{ghostty_location}}"

  # Download Zig 0.13.0 if needed (required for Ghostty 1.1.2)
  if [[ ! -d "$ZIG_DIR" ]]; then
    echo "Downloading Zig $ZIG_VERSION (required for Ghostty $GHOSTTY_VERSION)..."
    ARCH=$(uname -m)
    if [[ "$ARCH" == "arm64" ]]; then
      ZIG_ARCH="aarch64"
    else
      ZIG_ARCH="x86_64"
    fi
    ZIG_URL="https://ziglang.org/download/$ZIG_VERSION/zig-macos-$ZIG_ARCH-$ZIG_VERSION.tar.xz"
    curl -L "$ZIG_URL" -o "$TOOLS/zig-$ZIG_VERSION.tar.xz"
    cd "$TOOLS" && tar xf "zig-$ZIG_VERSION.tar.xz"
    mv "zig-macos-$ZIG_ARCH-$ZIG_VERSION" "$ZIG_DIR"
    rm "zig-$ZIG_VERSION.tar.xz"
  fi
  ZIG="$ZIG_DIR/zig"
  echo "Using Zig: $($ZIG version)"

  # Clone Ghostty at specific version
  if [[ ! -d "$GHOSTTY_REPO" ]]; then
    echo "Cloning Ghostty $GHOSTTY_VERSION..."
    git clone --branch "$GHOSTTY_VERSION" --depth 1 https://github.com/ghostty-org/ghostty.git "$GHOSTTY_REPO"
  else
    echo "Ghostty repo exists, checking version..."
    cd "$GHOSTTY_REPO"
    CURRENT=$(git describe --tags 2>/dev/null || echo "unknown")
    if [[ "$CURRENT" != "$GHOSTTY_VERSION" ]]; then
      echo "Switching to $GHOSTTY_VERSION..."
      git fetch --tags
      git checkout "$GHOSTTY_VERSION"
    fi
  fi

  # Build libghostty xcframework (disable sentry crash reporting)
  echo "Building libghostty xcframework..."
  cd "$GHOSTTY_REPO"
  "$ZIG" build -Doptimize=ReleaseFast -Dapp-runtime=none -Dsentry=false

  # On macOS, Ghostty builds a static library in the xcframework.
  # We need to convert it to a dynamic library for ghostty-sys.
  XCFW="$GHOSTTY_REPO/macos/GhosttyKit.xcframework/macos-arm64_x86_64"
  if [[ ! -d "$XCFW" ]]; then
    XCFW="$GHOSTTY_REPO/macos/GhosttyKit.xcframework/macos-arm64"
  fi
  STATIC_LIB="$XCFW/libghostty.a"
  if [[ ! -f "$STATIC_LIB" ]]; then
    STATIC_LIB="$XCFW/libghostty-fat.a"
  fi

  if [[ ! -f "$STATIC_LIB" ]]; then
    echo "Error: Static library not found in $XCFW"
    ls -la "$XCFW" 2>/dev/null || echo "Directory does not exist"
    exit 1
  fi

  echo "Converting static library to dynamic library..."
  mkdir -p "$DEST"

  # Copy static lib
  cp "$STATIC_LIB" "$DEST/libghostty.a"

  # Use Homebrew freetype (required dependency)
  FREETYPE_PREFIX=$(brew --prefix freetype 2>/dev/null)
  if [[ -z "$FREETYPE_PREFIX" ]]; then
    echo "Error: freetype not found. Install with: brew install freetype"
    exit 1
  fi

  # Create arm64 dylib from static lib, linking required frameworks and deps
  # Uses system zlib (-lz) and Homebrew freetype
  clang -arch arm64 -dynamiclib -all_load "$STATIC_LIB" \
    -L"$FREETYPE_PREFIX/lib" -lfreetype \
    -o "$DEST/libghostty.dylib" \
    -framework AppKit \
    -framework Carbon \
    -framework CoreFoundation \
    -framework CoreGraphics \
    -framework CoreText \
    -framework CoreVideo \
    -framework CoreServices \
    -framework Foundation \
    -framework GameController \
    -framework IOSurface \
    -framework Metal \
    -framework MetalKit \
    -framework OpenGL \
    -framework QuartzCore \
    -lobjc \
    -lc++ \
    -lbz2 \
    -install_name "@rpath/libghostty.dylib"

  # Create symlink for ghostty-sys (expects .so)
  ln -sf libghostty.dylib "$DEST/libghostty.so"

  # Copy headers
  echo "Copying headers..."
  cp -r "$XCFW/Headers/"* "$DEST/"

  echo "Done! You can now run 'just build'"

build:
  test -f {{ghostty_location}}/libghostty.dylib
  GHOSTTY_LOCATION={{ghostty_location}} cargo build

run:
  test -f {{ghostty_location}}/libghostty.dylib
  GHOSTTY_LOCATION={{ghostty_location}} DYLD_LIBRARY_PATH={{ghostty_location}} cargo run

tauri-install:
  cd tauri-app && npm install

tauri-check:
  test -f {{ghostty_location}}/libghostty.dylib
  cd tauri-app/src-tauri && GHOSTTY_LOCATION={{ghostty_location}} DYLD_LIBRARY_PATH={{ghostty_location}} cargo check

tauri-dev:
  test -f {{ghostty_location}}/libghostty.dylib
  cd tauri-app && GHOSTTY_LOCATION={{ghostty_location}} DYLD_LIBRARY_PATH={{ghostty_location}} npm run tauri dev
