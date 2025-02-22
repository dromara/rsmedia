use crate::utils;

use rsmpeg::avutil::AVDictionary;

use std::collections::HashMap;

/// A wrapper type for ffmpeg options.
#[derive(Clone)]
pub struct Options(AVDictionary);

impl Options {
    /// Creates options such that ffmpeg will prefer TCP transport when reading RTSP stream (over
    /// the default UDP format).
    ///
    /// This sets the `rtsp_transport` to `tcp` in ffmpeg options.
    pub fn preset_rtsp_transport_tcp() -> Self {
        let opts = AVDictionary::new(
            &utils::from_str("rtsp_transport"),
            &utils::from_str("tcp"),
            0,
        );
        Self(opts)
    }

    /// Creates options such that ffmpeg will prefer TCP transport when reading RTSP stream (over
    /// the default UDP format). It also adds some options to reduce the socket and I/O timeouts to
    /// 4 seconds.
    ///
    /// This sets the `rtsp_transport` to `tcp` in ffmpeg options, it also sets `rw_timeout` to
    /// lower (more sane) values.
    pub fn preset_rtsp_transport_tcp_and_sane_timeouts() -> Self {
        let opts = AVDictionary::new(
            &utils::from_str("rtsp_transport"),
            &utils::from_str("tcp"),
            0,
        )
        // These can't be too low because ffmpeg takes its sweet time when connecting to RTSP
        // sources sometimes.
        .set(
            &utils::from_str("rw_timeout"),
            &utils::from_str("16000000"),
            0,
        )
        .set(
            &utils::from_str("stimeout"),
            &utils::from_str("16000000"),
            0,
        );

        Self(opts)
    }

    /// Creates options such that ffmpeg is instructed to fragment output and mux to fragmented mp4
    /// container format.
    ///
    /// This modifies the `movflags` key to supported fragmented output. The muxer output will not
    /// have a header and each packet contains enough metadata to be streamed without the header.
    /// Muxer output should be compatiable with MSE.
    pub fn preset_fragmented_mov() -> Self {
        let opts = AVDictionary::new(
            &utils::from_str("movflags"),
            &utils::from_str("faststart+frag_keyframe+frag_custom+empty_moov+omit_tfhd_offset"),
            0,
        );

        Self(opts)
    }

    /// Default options for a H264 encoder.
    pub fn preset_h264() -> Self {
        // Set H264 encoder to the medium preset.
        // preset: 预设编码配置,控制编码速度和质量的平衡
        // - ultrafast,superfast,veryfast,faster,fast
        // - medium (默认)
        // - slow,slower,veryslow
        let opts = AVDictionary::new(&utils::from_str("preset"), &utils::from_str("medium"), 0);

        Self(opts)
    }

    /// Options for a H264 encoder that are tuned for low-latency encoding such as for real-time
    /// streaming.
    pub fn preset_h264_realtime() -> Self {
        // Set H264 encoder to the medium preset.
        // preset: 预设编码配置,控制编码速度和质量的平衡
        // - slow: 更高质量,但编码速度较慢
        // - medium: 默认设置,平衡质量和速度
        // - fast: 更快编码速度,但质量可能略降
        let opts = AVDictionary::new(&utils::from_str("preset"), &utils::from_str("medium"), 0)
            // quality: NVENC特定的质量控制模式
            // - high: 质量优先模式,产生最佳画质但占用更多GPU资源
            // - speed: 性能优先模式,平衡质量和编码速度
            // - fast: 低延迟优先模式,最快编码速度但质量较低
            .set(&utils::from_str("quality"), &utils::from_str("fast"), 0)
            // Tune for low latency
            .set(&utils::from_str("tune"), &utils::from_str("zerolatency"), 0);

        Self(opts)
    }

    /// Convert back to ffmpeg native dictionary, which can be used with `ffmpeg` functions.
    pub fn to_dict(&self) -> AVDictionary {
        self.0.clone()
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
        let mut dict = AVDictionary::new(&utils::from_str(""), &utils::from_str(""), 0);
        for (k, v) in item {
            dict = dict.set(&utils::from_str(&k), &utils::from_str(&v), 0);
        }
        Self(dict)
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
            .map(|entry| {
                (
                    utils::to_string(entry.key()),
                    utils::to_string(entry.value()),
                )
            })
            .collect()
    }
}

unsafe impl Send for Options {}
unsafe impl Sync for Options {}
