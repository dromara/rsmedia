use std::env;
use std::path::PathBuf;

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
    let vcpkg_root = env::var("VCPKG_ROOT").expect("VCPKG_ROOT must be set");
    let vcpkg_root = PathBuf::from(vcpkg_root);
    // 检查可用的 triplet
    let triplets = ["x64-windows-static", "x64-windows-static-md", "x64-windows"];

    let mut available_triplets = Vec::new();
    for triplet in triplets.iter() {
        let lib_path = vcpkg_root.join("installed").join(triplet).join("lib");
        if lib_path.exists() {
            available_triplets.push(*triplet);
        }
    }

    // 优先使用 static 版本
    let target_triplet = if available_triplets.contains(&"x64-windows-static") {
        "x64-windows-static"
    } else if available_triplets.contains(&"x64-windows-static-md") {
        "x64-windows-static-md"
    } else {
        "x64-windows"
    };

    println!("cargo:warning=Using triplet: {}", target_triplet);

    // 添加库搜索路径
    let lib_path = vcpkg_root
        .join("installed")
        .join(target_triplet)
        .join("lib");
    println!("cargo:rustc-link-search=native={}", lib_path.display());

    let system_libs = [
        // COM 和 Media Foundation
        "ole32", "oleaut32", "mfplat", "mfuuid", "strmiids", // 安全相关
        "secur32", "crypt32", "bcrypt", "ncrypt", // 网络相关
        "ws2_32", "wininet", // 图形和 UI
        "gdi32", "user32", "shell32", // 系统核心
        "kernel32", "advapi32", "psapi", // 其他
        "uuid", "version",
    ];

    let ffmpeg_libs = [
        "avcodec",
        "avformat",
        "avutil",
        "swscale",
        "swresample",
        "avfilter",
        "avdevice",
    ];

    match target_triplet {
        "x64-windows-static" => {
            // 完全静态链接
            println!("cargo:rustc-link-arg=/NODEFAULTLIB:msvcrt.lib");
            println!("cargo:rustc-link-arg=/DEFAULTLIB:libcmt.lib");

            // 链接 FFmpeg 静态库
            for lib in ffmpeg_libs.iter() {
                println!("cargo:rustc-link-lib=static={}", lib);
            }
        }
        "x64-windows-static-md" => {
            // 使用动态运行时的静态链接
            println!("cargo:rustc-link-arg=/NODEFAULTLIB:libcmt.lib");
            println!("cargo:rustc-link-arg=/DEFAULTLIB:msvcrt.lib");

            // 链接 FFmpeg 库
            for lib in ffmpeg_libs.iter() {
                println!("cargo:rustc-link-lib={}", lib);
            }
        }
        "x64-windows" => {
            // 动态链接
            println!("cargo:rustc-link-arg=/DEFAULTLIB:msvcrt.lib");

            // 链接 FFmpeg 动态库
            for lib in ffmpeg_libs.iter() {
                println!("cargo:rustc-link-lib=dylib={}", lib);
            }
        }
        _ => panic!("Unsupported triplet"),
    }

    // 链接系统库
    for lib in system_libs.iter() {
        println!("cargo:rustc-link-lib={}", lib);
    }

    // 重新运行条件
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=VCPKG_ROOT");
}
