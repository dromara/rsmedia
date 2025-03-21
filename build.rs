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
    #[cfg(target_env = "msvc")]
    {
        // MSVC not need to link libc
    }
    #[cfg(target_env = "gnu")]
    {
        // GNU (MinGW)
        let mingw_paths = ["C:\\msys64\\mingw64\\lib", "C:\\MinGW\\lib"];
        for path in &mingw_paths {
            if std::path::Path::new(path).exists() {
                println!("cargo:rustc-link-search=native={}", path);
            }
        }

        // necessary
        println!("cargo:rustc-link-lib=static=mingwex");
        println!("cargo:rustc-link-lib=dylib=msvcrt");

        // MinGW lib path
        if let Ok(gcc_dir) = env::var("MINGW_PREFIX") {
            println!("cargo:rustc-link-search=native={}/lib", gcc_dir);
        }
    }
}
