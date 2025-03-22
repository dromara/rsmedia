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
    // 获取 VCPKG_ROOT 并检查 triplet
    let vcpkg_root = PathBuf::from(env::var("VCPKG_ROOT").expect("VCPKG_ROOT not found"));
    let triplets = ["x64-windows-static", "x64-windows-static-md"];

    // 查找可用的 triplet
    let mut found_triplet = None;
    for triplet in triplets.iter() {
        let lib_path = vcpkg_root.join("installed").join(triplet).join("lib");
        if lib_path.exists() {
            println!("cargo:warning=Found triplet: {}", triplet);
            found_triplet = Some((triplet, lib_path));
            break;
        }
    }

    let (triplet, lib_path) = found_triplet.expect("No valid vcpkg triplets found!");

    // 添加 vcpkg 库路径
    println!("cargo:rustc-link-search=native={}", lib_path.display());

    // 配置运行时
    match *triplet {
        "x64-windows-static" => {
            println!("cargo:rustc-link-arg=/NODEFAULTLIB:msvcrt.lib");
            println!("cargo:rustc-link-arg=/DEFAULTLIB:libcmt.lib");
        }
        "x64-windows-static-md" => {
            println!("cargo:rustc-link-arg=/NODEFAULTLIB:libcmt.lib");
            println!("cargo:rustc-link-arg=/DEFAULTLIB:msvcrt.lib");
        }
        _ => unreachable!(),
    }

    // 添加 Windows SDK 路径
    if let Ok(windows_sdk_dir) = env::var("WindowsSdkDir") {
        let sdk_version = env::var("WindowsSDKLibVersion").unwrap_or("10.0.22621.0".to_string());

        let sdk_lib_path = PathBuf::from(windows_sdk_dir.clone())
            .join("Lib")
            .join(&sdk_version)
            .join("um")
            .join("x64");
        println!("cargo:rustc-link-search=native={}", sdk_lib_path.display());

        let sdk_ucrt_path = PathBuf::from(windows_sdk_dir)
            .join("Lib")
            .join(&sdk_version)
            .join("ucrt")
            .join("x64");
        println!("cargo:rustc-link-search=native={}", sdk_ucrt_path.display());
    }

    // Visual Studio 路径
    if let Ok(vs_path) = env::var("VCINSTALLDIR") {
        let vs_lib_path = PathBuf::from(vs_path).join("lib").join("x64");
        println!("cargo:rustc-link-search=native={}", vs_lib_path.display());
    }

    // FFmpeg
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

    let system_libs = [
        "gdi32",
        "psapi",
        "ole32",
        "strmiids",
        "uuid",
        "oleaut32",
        "shlwapi",
        "user32",
        "ws2_32",
        "vfw32",
        "secur32",
        "bcrypt",
        "advapi32",
        "shell32",
        "mf",
        "mfplat",
        "mfplay",
        "mfreadwrite",
        "mfuuid",
        "evr",
        "dxva2",
        "wmcodecdspuuid",
    ];

    for lib in system_libs.iter() {
        println!("cargo:rustc-link-lib={}", lib);
    }

    // 显式链接 GUID 库
    println!("cargo:rustc-link-arg=mfuuid.lib");
    println!("cargo:rustc-link-arg=strmiids.lib");

    // 链接器选项
    let linker_flags = [
        "/DYNAMICBASE",
        "/NXCOMPAT",
        "/HIGHENTROPYVA",
        "/OPT:REF",
        "/OPT:ICF",
    ];

    for flag in linker_flags.iter() {
        println!("cargo:rustc-link-arg={}", flag);
    }

    // 重新运行条件
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=VCPKG_ROOT");
    println!("cargo:rerun-if-env-changed=WindowsSdkDir");
    println!("cargo:rerun-if-env-changed=VCINSTALLDIR");
}
