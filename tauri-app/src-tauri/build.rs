fn main() {
    tauri_build::build();

    // Link macOS frameworks required by libghostty.
    #[cfg(target_os = "macos")]
    {
        use std::{env, path::PathBuf};

        // Ensure the runtime loader can find libghostty.dylib during dev runs.
        // We also copy the dylib next to the built binary for convenience.
        fn top_level_cargo_target_dir() -> PathBuf {
            let pkg_name = env::var("CARGO_PKG_NAME").unwrap();
            let out_dir = env::var_os("OUT_DIR").unwrap();
            let mut target = PathBuf::from(&out_dir);
            let pop = |target: &mut PathBuf| assert!(target.pop(), "malformed OUT_DIR: {:?}", out_dir);
            while !target
                .file_name()
                .unwrap()
                .to_string_lossy()
                .contains(&pkg_name)
            {
                pop(&mut target);
            }
            pop(&mut target);
            pop(&mut target);
            target
        }

        println!("cargo:rerun-if-env-changed=GHOSTTY_LOCATION");
        if let Ok(ghostty_location) = env::var("GHOSTTY_LOCATION") {
            let dylib_path = PathBuf::from(&ghostty_location).join("libghostty.dylib");
            if dylib_path.exists() {
                let target_dir = top_level_cargo_target_dir();
                let dest = target_dir.join("libghostty.dylib");
                let _ = std::fs::copy(&dylib_path, &dest);

                // Add rpath so the binary can load libghostty.dylib directly
                // from the provided GHOSTTY_LOCATION.
                println!("cargo:rustc-link-arg=-Wl,-rpath,{}", ghostty_location);
            }
        }

        let frameworks = [
            "AppKit",
            "Carbon",
            "CoreFoundation",
            "CoreGraphics",
            "CoreText",
            "CoreVideo",
            "Foundation",
            "Metal",
            "MetalKit",
            "OpenGL",
            "QuartzCore",
            "GameController",
        ];

        for framework in frameworks {
            println!("cargo:rustc-link-lib=framework={framework}");
        }

        println!("cargo:rustc-link-lib=objc");
        println!("cargo:rustc-link-lib=c++");
    }
}
