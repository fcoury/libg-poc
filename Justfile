set shell := ["zsh", "-cu"]

root := justfile_directory()
ghostty_location := root + "/.tools/libghostty"

default:
  @just --list

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
