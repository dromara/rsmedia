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
        let opts = AVDictionary::new(&utils::from_str("rtsp_transport"), &utils::from_str("tcp"), 0);
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

    /// Default options for a libx264 encoder.
    pub fn preset_h264() -> Self {
        let mut opts = HashMap::new();
        // ultrafast,superfast,veryfast,faster,fast,medium,slow,slower,veryslow,placebo
        opts.insert("preset".to_string(), "medium".to_string());
        // baseline,main,high
        opts.insert("profile:v".to_string(), "high".to_string());

        // HashMap<String, String> -> Options
        opts.into()
    }

    /// Options for a libx264 encoder that are tuned for low-latency encoding such as for real-time streaming.
    pub fn preset_h264_realtime() -> Self {
        let mut opts = HashMap::new();
        // ultrafast,superfast,veryfast,faster,fast,medium,slow,slower,veryslow,placebo
        opts.insert("preset".to_string(), "medium".to_string());
        // baseline,main,high
        opts.insert("profile:v".to_string(), "main".to_string());
        // crf, vbr, cbr, abr
        opts.insert("rc".to_string(), "cbr".to_string());
        // 场景切换敏感度
        opts.insert("scenecut".to_string(), "0".to_string());
        // 周期内部刷新替代关键帧
        opts.insert("intra-refresh".to_string(), "1".to_string());
        // 参考帧数量
        opts.insert("ref".to_string(), "3".to_string());
        // GOP=60（2秒@30fps）
        opts.insert("g".to_string(), "60".to_string());
        // 禁用 B 帧
        opts.insert("bf".to_string(), "0".to_string());
        // 最小量化参数
        opts.insert("qmin".to_string(), "4".to_string());
        // 最大量化参数
        opts.insert("qmax".to_string(), "51".to_string());
        // 启用中等强度去块滤波
        opts.insert("deblock".to_string(), "1:1".to_string());
        // film,animation,grain,stillimage,psnr,ssim,fastdecode,zerolatency
        opts.insert("tune".to_string(), "fastdecode".to_string());
        // 自适应量化模式
        opts.insert("aq-mode".to_string(), "2".to_string());
        // 量化优化, 0: 禁用, 1: 仅用于最终编码, 2: 用于所有模式决策
        opts.insert("trellis".to_string(), "1".to_string());
        opts.insert("threads".to_string(), "auto".to_string());
        // 使用所有可用的分区模式
        opts.insert("partitions".to_string(), "all".to_string());

        // HashMap<String, String> -> Options
        opts.into()
    }

    /// h264_nvenc options only
    pub fn preset_h264_nvenc() -> Self {
        let mut opts = HashMap::new();
        // p1-p7, default(p4), slow, medium, fast, hp, hq, bd
        opts.insert("preset".to_string(), "p7".to_string());
        // baseline, main, high, high444p, high10, high422
        opts.insert("profile".to_string(), "high".to_string());
        // ll, ull, lossless, film, animation, grain, fastdecode, zerolatency, hq
        opts.insert("tune".to_string(), "ll".to_string());
        // constqp, vbr, cbr, vbr_hq, cbr_hq, vbr_minqp, qvbr, cbr_ld_hq, cbr_ll_hq
        // ll_2pass, ll_2pass_quality, ll_2pass_size,
        opts.insert("rc".to_string(), "vbr_hq".to_string());
        opts.insert("qmin".to_string(), "19".to_string());
        opts.insert("qmax".to_string(), "21".to_string());
        opts.insert("spatial-aq".to_string(), "1".to_string());
        opts.insert("temporal-aq".to_string(), "1".to_string());
        opts.insert("aq-strength".to_string(), "8".to_string());
        opts.insert("2pass".to_string(), "1".to_string());
        opts.insert("g".to_string(), "60".to_string());
        opts.insert("bf".to_string(), "0".to_string());
        opts.insert("b_ref_mode".to_string(), "middle".to_string());
        opts.insert("no-scenecut".to_string(), "1".to_string());
        opts.insert("delay".to_string(), "0".to_string());
        opts.insert("zerolatency".to_string(), "1".to_string());

        // HashMap<String, String> -> Options
        opts.into()
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
            .map(|entry| (utils::to_string(entry.key()), utils::to_string(entry.value())))
            .collect()
    }
}

unsafe impl Send for Options {}
unsafe impl Sync for Options {}
