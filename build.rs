use std::env;
use std::path::PathBuf;

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();

    match target_os.as_str() {
        "macos" | "darwin" => configure_macos(),
        "linux" => configure_linux(&target_arch),
        "windows" => configure_windows(&target_arch),
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

fn configure_windows(target_arch: &str) {
    // 获取 VCPKG_ROOT 并检查 triplet
    let vcpkg_root = PathBuf::from(env::var("VCPKG_ROOT").expect("VCPKG_ROOT not found"));
    let triplets = if target_arch == "x86_64" {
        vec![
            "x64-windows",
            "x64-windows-release",
            "x64-windows-static",
            "x64-windows-static-md",
        ]
    } else if target_arch == "aarch64" {
        vec![
            "arm64-windows",
            "arm64-windows-static",
            "arm64-windows-static-md",
        ]
    } else {
        panic!("Unsupported target architecture: {}", target_arch);
    };

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
    let is_static = triplet.contains("static");

    // 添加 vcpkg 库路径
    println!("cargo:rustc-link-search=native={}", lib_path.display());

    // 配置运行时
    if triplet.ends_with("-static") {
        println!("cargo:rustc-link-arg=/NODEFAULTLIB:msvcrt.lib");
        println!("cargo:rustc-link-arg=/DEFAULTLIB:libcmt.lib");
    } else {
        println!("cargo:rustc-link-arg=/NODEFAULTLIB:libcmt.lib");
        println!("cargo:rustc-link-arg=/DEFAULTLIB:msvcrt.lib");
    }

    // FFmpeg
    let ffmpeg_libs = [
        "avutil",
        "swscale",
        "swresample",
        "avcodec",
        "avformat",
        "avfilter",
        "avdevice",
    ];

    for lib in ffmpeg_libs.iter() {
        if is_static {
            println!("cargo:rustc-link-lib=static={}", lib);
        } else {
            println!("cargo:rustc-link-lib={}", lib);
        }
    }

    // Windows 系统库
    let system_libs = [
        // 基础系统库
        "kernel32",
        "user32",
        "gdi32",
        "advapi32",
        "shell32",
        "ole32",
        "oleaut32",
        "uuid",
        "ws2_32",
        // COM 和 Media Foundation
        "mf",
        "mfplat",
        "mfplay",
        "mfreadwrite",
        "mfuuid",
        "strmiids",
        "dxva2",
        "evr",
        // Security APIs
        "secur32",
        "security",
        "crypt32",
        "bcrypt",
        "ncrypt",
        "credui",
        "schannel",
        // 其他必需库
        "shlwapi",
        "psapi",
        "vfw32",
        "comdlg32",
        "comctl32",
        "msacm32",
        "winmm",
    ];

    for lib in system_libs.iter() {
        println!("cargo:rustc-link-lib={}", lib);
    }

    // 显式链接
    println!("cargo:rustc-link-arg=mfuuid.lib");
    println!("cargo:rustc-link-arg=strmiids.lib");
    println!("cargo:rustc-link-arg=secur32.lib");
    println!("cargo:rustc-link-arg=bcrypt.lib");
    println!("cargo:rustc-link-arg=dxva2.lib");
    println!("cargo:rustc-link-arg=ole32.lib");
    println!("cargo:rustc-link-arg=user32.lib");

    // 链接器选项
    let mut linker_flags = vec![
        // 基础安全选项
        "/NXCOMPAT",          // 启用数据执行保护 (DEP)
        "/DYNAMICBASE",       // 启用 ASLR
        "/HIGHENTROPYVA",     // 启用高熵 ASLR
        "/LARGEADDRESSAWARE", // 启用大内存地址支持
        // 优化选项
        "/OPT:REF",        // 删除未引用的函数和数据
        "/OPT:ICF",        // 合并重复的函数
        "/INCREMENTAL:NO", // 禁用增量链接
        // 调试和安全检查
        "/DEBUG",        // 包含调试信息
        "/DEBUGTYPE:CV", // 使用 CodeView 格式的调试信息
    ];

    // 架构特定的链接器选项
    if target_arch == "x86_64" {
        linker_flags.extend_from_slice(&[
            "/GUARD:CF",  // 启用控制流保护
            "/CETCOMPAT", // 启用 CET Shadow Stack (仅 x64)
        ]);
    } else if target_arch == "aarch64" {
        linker_flags.push("/GUARD:CF"); // ARM64 支持 CFG，但不支持 CET
    }
    for flag in linker_flags.iter() {
        println!("cargo:rustc-link-arg={}", flag);
    }

    // Windows SDK 和 Visual Studio 路径
    if let Ok(windows_sdk_dir) = env::var("WindowsSdkDir") {
        let sdk_version = env::var("WindowsSDKLibVersion").unwrap_or("10.0.22621.0".to_string());
        let arch_path = if target_arch == "x86_64" {
            "x64"
        } else {
            "arm64"
        };

        let sdk_lib_path = PathBuf::from(windows_sdk_dir.clone())
            .join("Lib")
            .join(&sdk_version)
            .join("um")
            .join(arch_path);
        if !sdk_lib_path.exists() {
            panic!("Windows SDK path not found: {}", sdk_lib_path.display());
        }
        println!("cargo:rustc-link-search=native={}", sdk_lib_path.display());

        let sdk_ucrt_path = PathBuf::from(windows_sdk_dir)
            .join("Lib")
            .join(&sdk_version)
            .join("ucrt")
            .join(arch_path);
        if !sdk_ucrt_path.exists() {
            panic!(
                "Windows SDK UCRT path not found: {}",
                sdk_ucrt_path.display()
            );
        }
        println!("cargo:rustc-link-search=native={}", sdk_ucrt_path.display());
    }

    if let Ok(vs_path) = env::var("VCINSTALLDIR") {
        let arch_path = if target_arch == "x86_64" {
            "x64"
        } else {
            "arm64"
        };
        let vs_lib_path = PathBuf::from(vs_path).join("lib").join(arch_path);
        println!("cargo:rustc-link-search=native={}", vs_lib_path.display());
    }

    // 重新运行条件
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=VCPKG_ROOT");
    println!("cargo:rerun-if-env-changed=WindowsSdkDir");
    println!("cargo:rerun-if-env-changed=VCINSTALLDIR");
}
