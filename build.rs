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
    let vcpkg_root = env::var("VCPKG_ROOT").expect("VCPKG_ROOT not found.");
    let vcpkg_root = PathBuf::from(vcpkg_root);
    // 检查可用的 triplet
    let triplets = ["x64-windows-static", "x64-windows-static-md", "x64-windows"];

    // 存储所有可用的 triplet 路径
    let mut available_lib_paths = Vec::new();

    // 检查每个 triplet 的库路径
    for triplet in triplets.iter() {
        let lib_path = vcpkg_root.join("installed").join(triplet).join("lib");
        if lib_path.exists() {
            println!("cargo:warning=Found triplet: {}", triplet);
            available_lib_paths.push(lib_path);
        }
    }

    if available_lib_paths.is_empty() {
        panic!("No valid vcpkg triplets found!");
    }

    // 添加所有可用的库路径
    for lib_path in &available_lib_paths {
        println!("cargo:rustc-link-search=native={}", lib_path.display());
    }

    // Windows SDK 库路径
    if let Ok(windows_sdk_dir) = env::var("WindowsSdkDir") {
        let sdk_lib_path = PathBuf::from(windows_sdk_dir)
            .join("Lib")
            .join(env::var("WindowsSDKLibVersion").unwrap_or("10.0.22621.0".to_string()))
            .join("um")
            .join("x64");
        println!("cargo:rustc-link-search=native={}", sdk_lib_path.display());
    }

    // 分组链接所需的系统库
    // 1. 安全相关库
    let security_libs = [
        "secur32",  // 包含 AcquireCredentialsHandleA 等
        "security", // 额外的安全功能
        "crypt32",  // 加密相关
        "bcrypt",   // BCrypt API
        "ncrypt",   // NCrypt API
        "credui",   // 凭据相关
        "schannel", // SSL/TLS
    ];

    // 2. COM 和 Media Foundation 相关库
    let com_mf_libs = [
        "ole32",          // COM 基础
        "oleaut32",       // COM 自动化
        "mfplat",         // Media Foundation 平台
        "mf",             // Media Foundation 核心
        "mfuuid",         // Media Foundation UUID
        "strmiids",       // DirectShow UUID
        "dxva2",          // DirectX Video Acceleration
        "evr",            // Enhanced Video Renderer
        "wmcodecdspuuid", // Windows Media Codec
    ];

    // 3. Windows 核心库
    let core_libs = [
        "kernel32", // 核心系统功能
        "user32",   // 用户界面
        "gdi32",    // 图形设备接口
        "shell32",  // Shell 功能
        "advapi32", // 高级 Windows 32 基础 API
        "wsock32",  // Windows Sockets (旧版)
        "ws2_32",   // Windows Sockets 2
        "iphlpapi", // IP Helper API
        "uuid",     // UUID 生成
        "normaliz", // 国际化
        "psapi",    // 进程状态 API
        "comdlg32", // Common Dialog
        "version",  // Version checking
        "winmm",    // Windows Multimedia
        "imm32",    // Input Method Manager
    ];

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

    // 检查是否存在静态库版本
    let use_static = available_lib_paths
        .iter()
        .any(|p| p.to_string_lossy().contains("static"));

    // 根据可用的库类型设置链接方式
    for lib in ffmpeg_libs.iter() {
        if use_static {
            println!("cargo:rustc-link-lib=static={}", lib);
        } else {
            println!("cargo:rustc-link-lib={}", lib);
        }
    }

    // 链接系统库
    for lib in security_libs.iter() {
        println!("cargo:rustc-link-lib={}", lib);
    }

    for lib in com_mf_libs.iter() {
        println!("cargo:rustc-link-lib={}", lib);
    }

    for lib in core_libs.iter() {
        println!("cargo:rustc-link-lib={}", lib);
    }

    // 运行时库设置
    if use_static {
        println!("cargo:rustc-link-arg=/NODEFAULTLIB:msvcrt.lib");
        println!("cargo:rustc-link-arg=/DEFAULTLIB:libcmt.lib");
    } else {
        println!("cargo:rustc-link-arg=/NODEFAULTLIB:libcmt.lib");
        println!("cargo:rustc-link-arg=/DEFAULTLIB:msvcrt.lib");
    }

    // 链接器选项
    println!("cargo:rustc-link-arg=/DYNAMICBASE"); // ASLR
    println!("cargo:rustc-link-arg=/NXCOMPAT"); // DEP
    println!("cargo:rustc-link-arg=/HIGHENTROPYVA"); // ASLR
                                                     // 优化和调试选项
    println!("cargo:rustc-link-arg=/OPT:REF"); // 移除未引用的函数和数据
    println!("cargo:rustc-link-arg=/DEBUG"); // 生成调试信息
    println!("cargo:rustc-link-arg=/MANIFEST"); // 生成清单文件

    // 重新运行条件
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=VCPKG_ROOT");
    println!("cargo:rerun-if-env-changed=WindowsSdkDir");
}
