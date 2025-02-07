use std::collections::HashMap;

use ffmpeg::Dictionary as AvDictionary;

/// A wrapper type for ffmpeg options.
#[derive(Debug, Clone)]
pub struct Options(AvDictionary<'static>);

impl Options {
    /// Creates options such that ffmpeg will prefer TCP transport when reading RTSP stream (over
    /// the default UDP format).
    ///
    /// This sets the `rtsp_transport` to `tcp` in ffmpeg options.
    pub fn preset_rtsp_transport_tcp() -> Self {
        let mut opts = AvDictionary::new();
        opts.set("rtsp_transport", "tcp");

        Self(opts)
    }

    /// Creates options such that ffmpeg will prefer TCP transport when reading RTSP stream (over
    /// the default UDP format). It also adds some options to reduce the socket and I/O timeouts to
    /// 4 seconds.
    ///
    /// This sets the `rtsp_transport` to `tcp` in ffmpeg options, it also sets `rw_timeout` to
    /// lower (more sane) values.
    pub fn preset_rtsp_transport_tcp_and_sane_timeouts() -> Self {
        let mut opts = AvDictionary::new();
        opts.set("rtsp_transport", "tcp");
        // These can't be too low because ffmpeg takes its sweet time when connecting to RTSP
        // sources sometimes.
        opts.set("rw_timeout", "16000000");
        opts.set("stimeout", "16000000");

        Self(opts)
    }

    /// Creates options such that ffmpeg is instructed to fragment output and mux to fragmented mp4
    /// container format.
    ///
    /// This modifies the `movflags` key to supported fragmented output. The muxer output will not
    /// have a header and each packet contains enough metadata to be streamed without the header.
    /// Muxer output should be compatiable with MSE.
    pub fn preset_fragmented_mov() -> Self {
        let mut opts = AvDictionary::new();
        opts.set(
            "movflags",
            "faststart+frag_keyframe+frag_custom+empty_moov+omit_tfhd_offset",
        );

        Self(opts)
    }

    /// Default options for a H264 encoder.
    pub fn preset_h264() -> Self {
        let mut opts = AvDictionary::new();
        // Set H264 encoder to the medium preset.
        // 1. 基础编码参数
        // preset: 预设编码配置,控制编码速度和质量的平衡
        // - ultrafast,superfast,veryfast,faster,fast
        // - medium (默认)
        // - slow,slower,veryslow
        opts.set("preset", "medium");

        // profile: H.264编码配置文件
        // - baseline: 基本画质,适用于低延迟视频通话
        // - main: 主要画质,用于一般视频流
        // - high: 高画质,用于高清视频,支持8位色深
        opts.set("profile:v", "high");

        // H.264编码级别
        // - 常用级别: 3.0, 3.1, 4.0, 4.1, 4.2, 5.0, 5.1
        opts.set("level", "4.1");

        Self(opts)
    }

    /// Options for a H264 encoder that are tuned for low-latency encoding such as for real-time
    /// streaming.
    pub fn preset_h264_realtime() -> Self {
        let mut opts = AvDictionary::new();
        // Set H264 encoder to the medium preset.
        // preset: 预设编码配置,控制编码速度和质量的平衡
        // - slow: 更高质量,但编码速度较慢
        // - medium: 默认设置,平衡质量和速度
        // - fast: 更快编码速度,但质量可能略降
        opts.set("preset", "medium");

        // quality: NVENC特定的质量控制模式
        // - high: 质量优先模式,产生最佳画质但占用更多GPU资源
        // - speed: 性能优先模式,平衡质量和编码速度
        // - fast: 低延迟优先模式,最快编码速度但质量较低
        opts.set("quality", "fast");

        // Tune for low latency
        opts.set("tune", "zerolatency");

        // profile: H.264编码配置文件
        // - baseline: 基本画质,适用于低延迟视频通话
        // - main: 主要画质,用于一般视频流
        // - high: 高画质,用于高清视频,支持8位色深
        opts.set("profile:v", "main");

        Self(opts)
    }

    /// Convert back to ffmpeg native dictionary, which can be used with `ffmpeg` functions.
    pub(super) fn to_dict(&self) -> AvDictionary {
        self.0.clone()
    }
}

impl Default for Options {
    fn default() -> Self {
        Self(AvDictionary::new())
    }
}

impl From<HashMap<String, String>> for Options {
    /// Converts from `HashMap` to `Options`.
    ///
    /// # Arguments
    ///
    /// * `item` - Item to convert from.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let my_opts = HashMap::new();
    /// options.insert(
    ///     "my_option".to_string(),
    ///     "my_value".to_string(),
    /// );
    ///
    /// let opts: Options = my_opts.into();
    /// ```
    fn from(item: HashMap<String, String>) -> Self {
        let mut opts = AvDictionary::new();
        for (k, v) in item {
            opts.set(&k.clone(), &v.clone());
        }

        Self(opts)
    }
}

impl From<Options> for HashMap<String, String> {
    /// Converts from `Options` to `HashMap`.
    ///
    /// # Arguments
    ///
    /// * `item` - Item to convert from.
    fn from(item: Options) -> Self {
        item.0
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }
}

unsafe impl Send for Options {}
unsafe impl Sync for Options {}
