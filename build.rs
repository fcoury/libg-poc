fn main() {
    // Required for libghostty static library linking on macOS.
    // This mirrors the frameworks used by Ghostty's macOS support libs
    // plus Metal/Objective-C and libc++ for ImGui's Metal backend.
    #[cfg(target_os = "macos")]
    {
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
        ];

        for framework in frameworks {
            println!("cargo:rustc-link-lib=framework={framework}");
        }

        // Objective-C runtime and libc++ for C++ deps.
        println!("cargo:rustc-link-lib=objc");
        println!("cargo:rustc-link-lib=c++");
    }
}
