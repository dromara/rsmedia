
## ffmpeg 安装
```bash
## FFmpeg BuildTools
sudo apt-get install \
    autoconf \
    automake \
    build-essential \
    bzip2 \
    cmake \
    clang \
    gcc \
    g++ \
    git \
    git-core \
    libass-dev \
    libfreetype6-dev \
    libgnutls28-dev \
    libmp3lame-dev \
    libopus-dev \
    libfdk-aac-dev \
    libtheora-dev \
    libvorbis-dev \
    libvpx-dev \
    libx264-dev \
    libx265-dev \
    libxvidcore-dev \
    libv4l-dev \
    libcurl4-openssl-dev \
    libssl-dev \
    libraw1394-dev \
    libdc1394-dev \
    libavc1394-dev \
    libiec61883-dev \
    libjack-dev \
    libfaad-dev \
    libgsm1-dev \
    libzmq3-dev \
    libssh-dev \
    libbluray-dev \
    libopenmpt-dev \
    ocl-icd-opencl-dev \
    libogg-dev \
    libspeex-dev \
    flite1-dev \
    libchromaprint-dev \
    libopenal-dev \
    libcdio-dev \
    libcaca-dev \
    libpocketsphinx-dev \
    libsphinxbase-dev \
    libbs2b-dev \
    liblilv-dev \
    libsratom-dev \
    libsord-dev \
    libserd-dev \
    librubberband-dev \
    libsamplerate0-dev \
    libmysofa-dev \
    libvidstab-dev \
    libzimg-dev \
    libgme-dev \
    librabbitmq-dev \
    libdav1d-dev \
    libzvbi-dev \
    libsnappy-dev \
    libaom-dev \
    libcodec2-dev \
    libshine-dev \
    libtwolame-dev \
    libwebp-dev \
    libsoxr-dev \
    libcdio-paranoia-dev \
    libcdio-cdda-dev \
    libsrt-gnutls-dev \
    libmfx-dev \
    libsdl2-dev \
    libva-dev \
    libvdpau-dev \
    libxcb1-dev \
    libxcb-shm0-dev \
    libxcb-xfixes0-dev \
    libtool \
    libc6 \
    libc6-dev \
    libnuma1 \
    libnuma-dev \
    pkg-config \
    texinfo \
    unzip \
    wget \
    yasm \
    nasm \
    zlib1g-dev \
    libnuma-dev

## System FFmpeg
apt-get -y install \
    libavcodec-dev \
    libavdevice-dev \
    libavfilter-dev \
    libavformat-dev \
    libavutil-dev \
    libpostproc-dev \
    libswresample-dev \
    libswscale-dev
```

## Build

```bash
## ffmpeg7
export FFMPEG_INCLUDE_DIR=/opt/ffmpeg-7.1-d725360//ffmpeg_build/include
export FFMPEG_PKG_CONFIG_PATH=/opt/ffmpeg-7.1-d725360//ffmpeg_build/lib/pkgconfig
#
cargo build --no-default-features --features ffmpeg7,link_system_ffmpeg --verbose
cargo test --no-default-features --features ffmpeg7,link_system_ffmpeg --verbose

## ffmpeg6
export FFMPEG_INCLUDE_DIR=/opt/ffmpeg-6.1-ac60bc2/ffmpeg_build/include
export FFMPEG_PKG_CONFIG_PATH=/opt/ffmpeg-6.1-ac60bc2/ffmpeg_build/lib/pkgconfig
#
cargo build --no-default-features --features ffmpeg6,link_system_ffmpeg --verbose
cargo test --no-default-features --features ffmpeg6,link_system_ffmpeg --verbose

## ffmpeg5
export FFMPEG_INCLUDE_DIR=/opt/ffmpeg-5.1-6e63e49/ffmpeg_build/include
export FFMPEG_PKG_CONFIG_PATH=/opt/ffmpeg-5.1-6e63e49/ffmpeg_build/lib/pkgconfig
#
cargo build --no-default-features --features ffmpeg5,link_system_ffmpeg --verbose
cargo test --no-default-features --features ffmpeg5,link_system_ffmpeg --verbose
```

## 配置宏和feature组合

常见的 `#[cfg()]` 和 `feature` 的组合方式.

以下是一些常见的组合方式和它们的用途：

### 1. `#[cfg(feature = "feature_name")]`
   描述：当指定的特性（feature_name）被启用时，编译此代码。
   示例：
   ```rust
   #[cfg(feature = "ffi")]
   fn use_ffi() {
   // 只有在启用 "ffi" 特性时编译
   }
   ```
   解释：此代码仅在 Cargo.toml 文件中启用了 ffi 特性时会被编译。
### 2. `#[cfg(not(feature = "feature_name"))]`
   描述：当指定的特性（feature_name）没有启用时，编译此代码。
   示例：
   ```rust
   #[cfg(not(feature = "ffi"))]
   fn use_default() {
   // 只有在没有启用 "ffi" 特性时编译
   }
   ```
   解释：此代码仅在 ffi 特性未启用时会被编译。
### 3. `#[cfg(any(feature = "feature1", feature = "feature2"))]`
   描述：当指定的特性之一（feature1 或 feature2）启用时，编译此代码。
   示例：
   ```rust
   #[cfg(any(feature = "ffi", feature = "openssl"))]
   fn use_crypto() {
   // 只有在启用 "ffi" 或 "openssl" 特性之一时编译
   }
   ```
   解释：此代码将在启用 ffi 或 openssl 特性时编译。
### 4.`#[cfg(all(feature = "feature1", feature = "feature2"))]`
   描述：当多个特性同时启用时，编译此代码。
   示例：
   ```rust
   #[cfg(all(feature = "ffi", feature = "openssl"))]
   fn use_ffi_and_openssl() {
   // 只有在同时启用 "ffi" 和 "openssl" 特性时编译
   }
   ```
   解释：此代码仅在 ffi 和 openssl 特性都启用时会被编译。
### 5. `#[cfg(feature = "feature_name" if ...) ]`
   描述：通过条件语句检查特性是否启用，类似于 if 语句判断条件。
   示例：
   ```rust
   #[cfg(feature = "ffi")]
   fn use_ffi() {
   println!("Using ffi feature");
   }
   ```
   解释：如果在 Cargo.toml 中启用了 ffi 特性，那么会编译该部分代码。
### 6. `#[cfg(all(feature = "feature1", not(feature = "feature2")))]`
   描述：当某个特性启用，且另一个特性没有启用时，编译此代码。
   示例：
   ```rust
   #[cfg(all(feature = "ffi", not(feature = "openssl")))]
   fn use_ffi_without_openssl() {
   // 只有在启用 "ffi" 特性而没有启用 "openssl" 时编译
   }
   ```
   解释：此代码仅在启用了 ffi 特性并且没有启用 openssl 特性时会被编译。
### 7. `#[cfg(feature = "feature_name", target_os = "linux")]`
   描述：结合操作系统目标和特性判断，进行跨平台特性的选择。
   示例：
   ```rust
   #[cfg(all(feature = "ffi", target_os = "linux"))]
   fn use_ffi_on_linux() {
   // 只有在启用 "ffi" 特性并且目标操作系统是 Linux 时编译
   }
   ```
   解释：此代码仅在启用了 ffi 特性，并且目标操作系统是 Linux 时会被编译。 
   
> 小结:
> 
> (1) `#[cfg(feature = "feature_name")]`：当特性启用时编译。
> 
> (2) `#[cfg(not(feature = "feature_name"))]`：当特性未启用时编译。
> 
> (3) `#[cfg(any(feature = "feature1", feature = "feature2"))]`：当任意一个特性启用时编译。
> 
> (4) `#[cfg(all(feature = "feature1", feature = "feature2"))]`：当所有特性都启用时编译。
> 
> (5) `#[cfg(all(feature = "feature1", not(feature = "feature2")))]`：当一个特性启用，另一个特性未启用时编译。