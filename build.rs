use std::env;

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();

    match target_os.as_str() {
        "linux" => configure_linux(&target_arch),
        "macos" | "darwin" => configure_macos(),
        "windows" => configure_windows(),
        _ => panic!("Unsupported operating system"),
    }
}

fn configure_linux(target_arch: &str) {
    println!("cargo:rustc-link-lib=dylib=c");
    println!("cargo:rustc-link-lib=dylib=dl");
    println!("cargo:rustc-link-lib=dylib=pthread");

    if target_arch == "x86_64" {
        println!("cargo:rustc-link-search=native=/lib/x86_64-linux-gnu");
        println!("cargo:rustc-link-search=native=/usr/lib/x86_64-linux-gnu");
    } else if target_arch == "aarch64" {
        println!("cargo:rustc-link-search=native=/lib/aarch64-linux-gnu");
        println!("cargo:rustc-link-search=native=/usr/lib/aarch64-linux-gnu");
    }

    println!("cargo:rustc-link-search=native=/usr/local/lib");
    println!("cargo:rustc-link-search=native=/usr/lib");
    println!("cargo:rustc-link-search=native=/lib");
}

fn configure_macos() {
    println!("cargo:rustc-link-lib=dylib=c");
    println!("cargo:rustc-link-lib=dylib=dl");
    println!("cargo:rustc-link-lib=dylib=pthread");
    println!("cargo:rustc-link-search=native=/usr/lib");
    println!("cargo:rustc-link-search=native=/usr/local/lib");

    // Support Homebrew
    if std::path::Path::new("/opt/homebrew/lib").exists() {
        // Apple Silicon (M1/M2)
        println!("cargo:rustc-link-search=native=/opt/homebrew/lib");
    }
    if std::path::Path::new("/usr/local/opt").exists() {
        // Intel
        println!("cargo:rustc-link-search=native=/usr/local/opt");
    }
}

fn configure_windows() {
    if let Ok(vcpkg_root) = env::var("VCPKG_ROOT") {
        let target_triplet = if cfg!(target_arch = "x86_64") {
            "x64-windows-static"
        } else {
            "x86-windows-static"
        };
        let lib_path = format!("{}\\installed\\{}\\lib", vcpkg_root, target_triplet);

        // vcpkg lib path
        println!("cargo:rustc-link-search=native={}", lib_path);

        //  MSVC
        let system_libs = [
            "secur32", "ws2_32", "wininet", "crypt32", "bcrypt", "ncrypt", "ole32", "oleaut32",
            "gdi32", "user32", "psapi", "advapi32", "shell32", "strmiids", "mfplat", "mfuuid",
            "kernel32", "uuid", "version", "msvcrt", "libcmt",
        ];
        for lib in system_libs.iter() {
            println!("cargo:rustc-link-lib={}", lib);
        }

        // ffmpeg static libs
        let ffmpeg_libs = [
            "avcodec",
            "avformat",
            "avutil",
            "swscale",
            "swresample",
            "avfilter",
            "avdevice",
        ];
        for lib in ffmpeg_libs.iter() {
            println!("cargo:rustc-link-lib=static={}", lib);
        }
    } else {
        panic!("'VCPKG_ROOT' not found");
    }
}
