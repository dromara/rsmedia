use crate::utils;

use rsmpeg::avutil::AVDictionary;

use std::collections::HashMap;

/// A macro to create a HashMap with key-value pairs.
macro_rules! map {
    // empty
    () => {
        ::std::collections::HashMap::new()
    };
    // key-value pairs
    ($($key:expr => $value:expr),+ $(,)?) => {
        {
            let mut map = ::std::collections::HashMap::new();
            $(
                map.insert($key.into(), $value.into());
            )+
            map
        }
    };
}

/// A wrapper type for ffmpeg options.
///
/// FFmpeg Documentation: <https://ffmpeg.org/doxygen/trunk/>
///
/// `libavformat/options_table.h`: <https://www.ffmpeg.org/doxygen/trunk/libavformat_2options__table_8h-source.html>
/// `libavcodec/options_table.h`: <https://www.ffmpeg.org/doxygen/trunk/libavcodec_2options__table_8h_source.html>
#[derive(Clone)]
pub struct Options(AVDictionary);

impl Options {
    pub fn new(dict: AVDictionary) -> Self {
        Self(dict)
    }

    /// Creates options such that ffmpeg will prefer TCP transport when reading RTSP stream (over
    /// the default UDP format). It also adds some options to reduce the socket and I/O timeouts to
    /// 4 seconds.
    ///
    /// This sets the `rtsp_transport` to `tcp` in ffmpeg options,
    /// it also sets `rw_timeout` and `stimeout` to lower (more sane) values.
    pub fn preset_avformat_rtsp_transport_tcp() -> Self {
        let opts = map! {
            "rtsp_transport" => "tcp",
            // These can't be too low because ffmpeg takes its sweet time
            "rw_timeout" => "16000000",
            "stimeout" => "16000000",
        };

        // HashMap<String, String> -> Options
        opts.into()
    }

    /// Creates options such that ffmpeg is instructed to fragment output and mux to fragmented mp4
    /// container format.
    ///
    /// This modifies the `movflags` key to supported fragmented output. The muxer output will not
    /// have a header and each packet contains enough metadata to be streamed without the header.
    /// Muxer output should be compatiable with MSE.
    pub fn preset_avformat_fragmented_mov() -> Self {
        let opts = map! {
            "movflags" => "faststart+frag_keyframe+frag_custom+empty_moov+omit_tfhd_offset",
        };

        // HashMap<String, String> -> Options
        opts.into()
    }

    /// Creates options for a FLV muxer.
    pub fn preset_avformat_flv() -> Self {
        let opts = map! {
            "flvflags" => "no_duration_filesize",
            "fflags" => "nobuffer+flush_packets",
             // 添加实时流标志
            "live" => "1",
             // 完全禁用元数据更新
            "write_metaf" => "0",
             // 设置较小的chunk大小以减少延迟
            "chunk_size" => "4096",
        };

        // HashMap<String, String> -> Options
        opts.into()
    }

    /// Default avcodec options for a libx264 encoder.
    pub fn preset_h264() -> Self {
        let opts = map! {
             // ultrafast,superfast,veryfast,faster,fast,medium,slow,slower,veryslow,placebo
            "preset" => "medium",
             // baseline,main,high
            "profile:v" => "high",
            // 场景切换敏感度
            "scenecut" => "0",
        };

        // HashMap<String, String> -> Options
        opts.into()
    }

    /// Options for a libx264 encoder that are tuned for low-latency encoding such as for real-time streaming.
    pub fn preset_h264_realtime() -> Self {
        let opts = map! {
            // ultrafast,superfast,veryfast,faster,fast,medium,slow,slower,veryslow,placebo
            "preset" => "medium",
            // baseline,main,high
            "profile:v" => "main",
             // film,animation,grain,stillimage,psnr,ssim,fastdecode,zerolatency
            "tune" => "zerolatency",
            // rc 参数在 libx264 中不是直接支持的，应该使用 rate_control 或相关参数如 crf, qp 或 bitrate
            // crf, vbr, cbr, abr
            // "rc" => "cbr",
            // 设置比特率控制,视频比特率
            "b:v" => "3000k",
            // 最大比特率
            "maxrate" => "3500k",
            // 缓冲区大小
            "bufsize" => "3000k",
            // 恒定质量因子
            "crf" => "23",
            // 场景切换敏感度
            "scenecut" => "0",
            // 周期内部刷新替代关键帧
            "intra-refresh" => "1",
            // 参考帧数量
            "refs" => "3",
            // GOP=60（2秒@30fps）
            "g" => "60",
            // 禁用 B 帧
            "bf" => "0",
            // 最小量化参数
            "qmin" => "4",
            // 最大量化参数
            "qmax" => "51",
            // 启用中等强度去块滤波
            "deblock" => "1:1",
            // 自适应量化模式
            "aq-mode" => "2",
            // 量化优化, 0: 禁用, 1: 仅用于最终编码, 2: 用于所有模式决策
            "trellis" => "1",
            "threads" => "auto",
            // 使用所有可用的分区模式
            "partitions" => "all",
            // 最小关键帧间隔
            "keyint_min" => "30",
            // 强制恒定帧率
            "force-cfr" => "1",
            // 启用切片线程
            "sliced_threads" => "1",
            // 禁用前瞻同步
            "sync-lookahead" => "0",
            // 减少前瞻帧数
            "rc-lookahead" => "10",
            // 使用x264opts组合参数进行更精细的控制
            "x264opts" => "no-mbtree:no-weightb:nal-hrd=cbr",
        };

        // HashMap<String, String> -> Options
        opts.into()
    }

    /// h264_nvenc options only
    ///
    /// FFMpeg with NVENC:
    /// <https://superuser.com/questions/1296374/best-settings-for-ffmpeg-with-nvenc>
    ///
    /// NVENC Preset Migration Guide:
    /// <https://docs.nvidia.com/video-technologies/video-codec-sdk/12.1/nvenc-preset-migration-guide/index.html>
    ///
    pub fn preset_h264_nvenc() -> Self {
        let opts = map! {
             // p1-p7, default(p4), slow, medium, fast, hp, hq, bd, ll, llhq, llhp, lossless
            "preset" => "p5",
            // baseline, main, high, high444p, high10, high422
            "profile" => "high",
            // ll, ull, lossless, film, animation, grain, fastdecode, zerolatency, hq
            "tune" => "ll",
            // 设置比特率 4Mbps
            "b:v" => "4000k",
            "maxrate" => "5000k",
            "bufsize" => "8000k",
            // constqp, ll_2pass_size, ll_2pass_quality
            // vbr, vbr_hq, vbr_minqp, vbr_2pass
            // cbr, cbr_hq, cbr_ld_hq
            "rc" => "cbr",
            // 添加锐度增强，低延迟不需要，增强画质可以启用
            // "rc-lookahead" => "20",
            // 量化参数
            "qmin" => "10",
            "qmax" => "18",
            // 启用自适应量化
            "spatial-aq" => "1",
            "temporal-aq" => "1",
            "aq-strength" => "8",
            // 启用2pass编码,注意与 zerolatency 可能冲突
            // "2pass" => "1",
            // GOP设置，较小的GOP有利于快速恢复和低延迟
            "g" => "30",
            // 禁用B帧以，避免出现画面闪烁
            "bf" => "0",
            "b_ref_mode" => "middle",
            // 启用场景切换检测，允许在场景变化时插入I帧
            "no-scenecut" => "0",
            // 低延时
            "delay" => "0",
            "zerolatency" => "1",
            // NVENC特有的参数
            // 增加表面缓冲区数量
            "surfaces" => "32",
            // 加权预测，改善低光照
            "weighted_pred" => "1",
        };

        // HashMap<String, String> -> Options
        opts.into()
    }

    /// 转换为 AVDictionary 但不转移所有权
    pub fn as_dict(&self) -> &AVDictionary {
        &self.0
    }

    /// 转换为 AVDictionary 并转移所有权
    pub fn into_dict(self) -> AVDictionary {
        self.0
    }

    /// 创建一个 AVDictionary 的副本
    pub fn to_dict(&self) -> AVDictionary {
        self.0.clone()
    }
}

/// HashMap<String, String> -> Options
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

/// Converts from `&Options` to `HashMap<String, String>`.
impl From<&Options> for HashMap<String, String> {
    fn from(item: &Options) -> Self {
        item.0
            .into_iter()
            .map(|entry| {
                (
                    utils::to_string(entry.key()).unwrap(),
                    utils::to_string(entry.value()).unwrap(),
                )
            })
            .collect()
    }
}

/// `Options` -> `HashMap<String, String>`
impl From<Options> for HashMap<String, String> {
    /// Converts from `Options` to `HashMap`.
    ///
    /// # Arguments
    ///
    /// * `item` - Item to convert from.
    fn from(item: Options) -> Self {
        (&item).into()
    }
}

impl std::fmt::Debug for Options {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let dict: HashMap<String, String> = self.into();
        write!(f, "{:?}", dict)
    }
}

impl std::fmt::Display for Options {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

unsafe impl Send for Options {}
unsafe impl Sync for Options {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_options_debug() {
        let opts = Options::preset_h264_realtime();
        println!("{:?}", opts);
    }
}
