

```bash
## FFmpeg BuildTools
sudo apt-get install \
    autoconf \
    automake \
    bzip2 \
    build-essential \
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
    pkg-config \
    texinfo \
    wget \
    yasm \
    zlib1g-dev \
    nasm \
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

### Build

```bash
export FFMPEG_INCLUDE_DIR=/opt/ffmpeg-6.1-ac60bc2/ffmpeg_build/include
export FFMPEG_PKG_CONFIG_PATH=/opt/ffmpeg-6.1-ac60bc2/ffmpeg_build/lib/pkgconfig

```