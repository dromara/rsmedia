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

#[derive(Debug, Copy, Clone)]
enum LinkType {
    Static,   // x64-windows-static
    StaticMD, // x64-windows-static-md
}

impl LinkType {
    fn from_triplet(triplet: &str) -> Option<Self> {
        match triplet {
            "x64-windows-static" => Some(LinkType::Static),
            "x64-windows-static-md" => Some(LinkType::StaticMD),
            _ => None,
        }
    }

    fn configure_runtime(&self) {
        match self {
            LinkType::Static => {
                println!("cargo:rustc-link-arg=/NODEFAULTLIB:msvcrt.lib");
                println!("cargo:rustc-link-arg=/DEFAULTLIB:libcmt.lib");
            }
            LinkType::StaticMD => {
                println!("cargo:rustc-link-arg=/NODEFAULTLIB:libcmt.lib");
                println!("cargo:rustc-link-arg=/DEFAULTLIB:msvcrt.lib");
            }
        }
    }
}

struct VcpkgConfig {
    link_type: LinkType,
    lib_paths: Vec<PathBuf>,
}

impl VcpkgConfig {
    fn new() -> Result<Self, String> {
        let vcpkg_root = PathBuf::from(env::var("VCPKG_ROOT").map_err(|_| "VCPKG_ROOT not found")?);

        // 检查静态链接的 triplet
        let triplets = ["x64-windows-static", "x64-windows-static-md"];

        let mut available_configs = Vec::new();
        for triplet in triplets.iter() {
            let lib_path = vcpkg_root.join("installed").join(triplet).join("lib");
            if lib_path.exists() {
                println!("cargo:warning=Found triplet: {}", triplet);
                if let Some(link_type) = LinkType::from_triplet(triplet) {
                    available_configs.push((link_type, lib_path));
                }
            }
        }

        if available_configs.is_empty() {
            return Err("No valid vcpkg triplets found!".to_string());
        }

        // 优先选择 Static 链接类型
        let (link_type, primary_lib_path) = available_configs
            .iter()
            .find(|(lt, _)| matches!(lt, LinkType::Static))
            .or_else(|| {
                available_configs
                    .iter()
                    .find(|(lt, _)| matches!(lt, LinkType::StaticMD))
            })
            .ok_or("No valid configuration found")?;

        let lib_paths = vec![primary_lib_path.clone()];

        Ok(VcpkgConfig {
            link_type: *link_type,
            lib_paths,
        })
    }

    fn configure_ffmpeg(&self, libs: &[&str]) {
        match self.link_type {
            LinkType::Static => {
                for lib in libs {
                    println!("cargo:rustc-link-lib=static={}", lib);
                }
            }
            LinkType::StaticMD => {
                for lib in libs {
                    println!("cargo:rustc-link-lib={}", lib);
                }
            }
        }
    }

    fn add_library_paths(&self) {
        // 添加 vcpkg 库路径
        for path in &self.lib_paths {
            println!("cargo:rustc-link-search=native={}", path.display());
        }

        // Windows SDK 路径
        if let Ok(windows_sdk_dir) = env::var("WindowsSdkDir") {
            let sdk_version =
                env::var("WindowsSDKLibVersion").unwrap_or("10.0.22621.0".to_string());
            let sdk_lib_path = PathBuf::from(windows_sdk_dir.clone())
                .join("Lib")
                .join(&sdk_version)
                .join("um")
                .join("x64");
            println!("cargo:rustc-link-search=native={}", sdk_lib_path.display());

            // 添加 SDK 的其他必要路径
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
    }
}

fn configure_windows() {
    let vcpkg_config = VcpkgConfig::new().unwrap();

    // 添加库搜索路径
    vcpkg_config.add_library_paths();

    // FFmpeg 库
    let ffmpeg_libs = [
        "avcodec",
        "avformat",
        "avutil",
        "swscale",
        "swresample",
        "avfilter",
        "avdevice",
    ];
    vcpkg_config.configure_ffmpeg(&ffmpeg_libs);

    // 配置运行时
    vcpkg_config.link_type.configure_runtime();

    let system_libs = [
        // 基础系统库
        "gdi32",    // Graphics Device Interface
        "psapi",    // Process Status API
        "ole32",    // COM/OLE Support
        "strmiids", // DirectShow GUID definitions
        "uuid",     // COM GUID definitions
        "oleaut32", // OLE Automation
        "shlwapi",  // Shell Light-weight Utility
        "user32",   // User Interface
        "ws2_32",   // Windows Sockets
        "vfw32",    // Video for Windows
        "secur32",  // Security Support Provider
        "bcrypt",   // Cryptography
        "advapi32", // Advanced Windows Services
        "shell32",  // Shell Services
        "mfplat",   // Media Foundation Platform
    ];

    // 链接系统库
    for lib in system_libs.iter() {
        println!("cargo:rustc-link-lib={}", lib);
    }

    // 链接器选项
    let linker_flags = [
        "/DYNAMICBASE",   // ASLR
        "/NXCOMPAT",      // DEP
        "/HIGHENTROPYVA", // 高熵 ASLR
        "/OPT:REF",       // 移除未引用的函数
        "/OPT:ICF",       // 相同代码折叠
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
