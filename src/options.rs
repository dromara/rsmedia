use crate::strutils;

use rsmpeg::avutil::{AVDictionary, AVDictionaryRef};
use rsmpeg::ffi;

use std::collections::{BTreeMap, HashMap};
use std::ptr::NonNull;

/// A wrapper type for ffmpeg options.
///
/// Internally backed by a [`BTreeMap<String, String>`] (key-sorted, so the
/// type derives a deterministic `Hash`) and lazily materialized into an
/// [`AVDictionary`] at the FFI boundary
/// ([`Options::into_dict`]/[`Options::to_dict`]).
///
/// This avoids a subtle defect of building AVDictionary directly: rsmpeg
/// cannot represent an *empty* dictionary (FFmpeg uses a NULL pointer), so
/// previous eager construction always carried a junk `key=""` entry that made
/// libav log `Option '' not found` on every `open()`. With lazy
/// materialization an empty `Options` converts to `None`.
///
/// FFmpeg Documentation: <https://ffmpeg.org/doxygen/trunk/>
///
/// `libavformat/options_table.h`: <https://www.ffmpeg.org/doxygen/trunk/libavformat_2options__table_8h-source.html>
/// `libavcodec/options_table.h`: <https://www.ffmpeg.org/doxygen/trunk/libavcodec_2options_table_8h_source.html>
///
/// # Example
///
/// ```ignore
/// let mut opts = Options::new();
/// opts.insert("threads", "4");
/// opts.merge(Options::preset_h264()); // preset keys win
/// ```
#[derive(Clone, Default, Hash, PartialEq, Eq)]
pub struct Options(BTreeMap<String, String>);

/// Alias of [`Options`] for the string key/value metadata carried by media
/// structures — `AVFrame.metadata`, container-level and per-stream metadata.
///
/// Use it in field/parameter positions to express "this is a metadata map",
/// reserving the [`Options`] name for encoder/muxer option sets.
pub type Metadata = Options;

impl Options {
    /// Creates an empty options set.
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Inserts a key-value pair, replacing any existing entry (same
    /// overwrite semantics as `av_dict_set` with flags 0).
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) -> &mut Self {
        self.0.insert(key.into(), value.into());
        self
    }

    /// Looks up a value by key.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    /// Removes a key, returning its previous value.
    pub fn remove(&mut self, key: &str) -> Option<String> {
        self.0.remove(key)
    }

    /// Returns true if the key is present.
    pub fn contains_key(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true if there are no entries.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Iterates over key-value pairs in key order.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.0.iter()
    }

    /// Merges `other` into `self`; entries from `other` win on conflicts
    /// (same overwrite semantics as `av_dict_copy` with flags 0).
    pub fn merge(&mut self, other: Options) {
        self.0.extend(other.0);
    }

    /// Materializes into an [`AVDictionary`], or `None` when empty so that
    /// `open(None)` is called instead of feeding libav a junk dictionary.
    ///
    /// Keys/values containing interior NUL bytes are skipped with a warning
    /// (they cannot be represented in a C string).
    pub fn to_dict(&self) -> Option<AVDictionary> {
        self.clone().into_dict()
    }

    /// Consuming variant of [`Options::to_dict`].
    pub fn into_dict(self) -> Option<AVDictionary> {
        let mut dict: Option<AVDictionary> = None;
        for (k, v) in self.0 {
            if k.contains('\0') || v.contains('\0') {
                log::warn!("Skip option with interior NUL: {k:?}={v:?}");
                continue;
            }
            let (key, value) = (strutils::str_to_cstring(&k), strutils::str_to_cstring(&v));
            dict = match dict {
                Some(dict) => Some(dict.set(&key, &value, 0)),
                None => Some(AVDictionary::new(&key, &value, 0)),
            };
        }
        dict
    }

    /// Reads entries back from an [`AVDictionary`].
    pub fn from_dict(dict: &AVDictionary) -> Self {
        dict.into_iter()
            .filter_map(|entry| {
                match (
                    strutils::cstr_to_string(entry.key()),
                    strutils::cstr_to_string(entry.value()),
                ) {
                    (Ok(key), Ok(value)) => Some((key, value)),
                    (bad_key, bad_value) => {
                        log::warn!("Skip non-UTF8 option: {bad_key:?}={bad_value:?}");
                        None
                    }
                }
            })
            .collect()
    }

    /// Reads entries back from a raw `AVDictionary` pointer, the form FFmpeg
    /// structs expose (e.g. `AVFrame.metadata`, `AVStream.metadata`).
    ///
    /// FFmpeg represents an empty dictionary as a null pointer, so a null `dict`
    /// yields an empty `Options`. Non-UTF-8 entries are skipped with a warning.
    ///
    /// # Safety
    ///
    /// `dict` must be null or point to a valid `AVDictionary` that outlives the call.
    pub(crate) unsafe fn from_raw_dict(dict: *mut ffi::AVDictionary) -> Self {
        let Some(ptr) = NonNull::new(dict) else {
            return Self::new();
        };
        // SAFETY: the reference is non-owning (`wrap_ref_pure` wraps the pointer in
        // `ManuallyDrop`) and is dropped before this function returns; the caller
        // guarantees the dictionary outlives the call.
        let borrowed = unsafe { AVDictionaryRef::from_raw(ptr) };
        Self::from_dict(&borrowed)
    }

    /// Replaces the `AVDictionary` at `*dest` with `self`, freeing whatever was
    /// stored there.
    ///
    /// This is the write counterpart of [`from_raw_dict`](Self::from_raw_dict) for
    /// metadata slots embedded in FFmpeg structs (`AVFrame.metadata`,
    /// `AVStream.metadata`, ...). An empty `Options` just frees the slot and leaves
    /// a null pointer — FFmpeg's representation of "no metadata". Ownership of the
    /// freshly built dictionary is transferred to `*dest`.
    ///
    /// # Safety
    ///
    /// `dest` must point at a live `AVDictionary` slot owned by an FFmpeg struct,
    /// or be null.
    pub(crate) unsafe fn write_into_raw_dict(&self, dest: &mut *mut ffi::AVDictionary) {
        unsafe { ffi::av_dict_free(dest) };
        if let Some(dict) = self.to_dict() {
            *dest = dict.into_raw().as_ptr();
        }
    }

    /// Creates options such that ffmpeg will prefer TCP transport when reading RTSP stream (over
    /// the default UDP format). It also adds some options to reduce the socket and I/O timeouts to
    /// 4 seconds.
    ///
    /// This sets the `rtsp_transport` to `tcp` in ffmpeg options,
    /// it also sets `rw_timeout` and `stimeout` to lower (more sane) values.
    pub fn preset_avformat_rtsp_transport_tcp() -> Self {
        let mut opts = Self::new();
        opts
            // These can't be too low because ffmpeg takes its sweet time
            .insert("rtsp_transport", "tcp")
            .insert("rw_timeout", "16000000")
            .insert("stimeout", "16000000");
        opts
    }

    /// Creates options such that ffmpeg is instructed to fragment output and mux to fragmented mp4
    /// container format.
    ///
    /// This modifies the `movflags` key to supported fragmented output. The muxer output will not
    /// have a header and each packet contains enough metadata to be streamed without the header.
    /// Muxer output should be compatiable with MSE.
    pub fn preset_avformat_fragmented_mov() -> Self {
        let mut opts = Self::new();
        opts.insert(
            "movflags",
            "faststart+frag_keyframe+frag_custom+empty_moov+omit_tfhd_offset",
        );
        opts
    }

    /// Creates options for a FLV muxer.
    pub fn preset_avformat_flv() -> Self {
        let mut opts = Self::new();
        opts.insert("flvflags", "no_duration_filesize")
            .insert("fflags", "nobuffer+flush_packets")
            // 添加实时流标志
            .insert("live", "1")
            // 完全禁用元数据更新
            .insert("write_metaf", "0")
            // 设置较小的chunk大小以减少延迟
            .insert("chunk_size", "4096");
        opts
    }

    /// Default avcodec options for a libx264 encoder.
    pub fn preset_h264() -> Self {
        let mut opts = Self::new();
        opts
            // ultrafast,superfast,veryfast,faster,fast,medium,slow,slower,veryslow,placebo
            .insert("preset", "medium")
            // baseline,main,high
            .insert("profile", "high")
            // 场景切换敏感度
            .insert("scenecut", "0");
        opts
    }

    /// Options for a libx264 encoder that are tuned for low-latency encoding such as for real-time streaming.
    pub fn preset_h264_realtime() -> Self {
        // 基址复用 preset_h264()，只覆盖低延迟所需的差异键，避免重复定义公共键值。
        let mut opts = Self::preset_h264();
        opts
            // baseline,main,high，低延迟用 main 而非 high
            .insert("profile", "main")
            // film,animation,grain,stillimage,psnr,ssim,fastdecode,zerolatency
            .insert("tune", "zerolatency")
            // 设置比特率控制,视频比特率
            .insert("b", "3000k")
            // 最大比特率
            .insert("maxrate", "3500k")
            // 缓冲区大小
            .insert("bufsize", "3000k")
            // 恒定质量因子
            .insert("crf", "23")
            // 周期内部刷新替代关键帧
            .insert("intra-refresh", "1")
            // 参考帧数量
            .insert("refs", "3")
            // GOP=60（2秒@30fps）
            .insert("g", "60")
            // 禁用 B 帧
            .insert("bf", "0")
            // 最小量化参数
            .insert("qmin", "4")
            // 最大量化参数
            .insert("qmax", "51")
            // 启用中等强度去块滤波
            .insert("deblock", "1:1")
            // 自适应量化模式
            .insert("aq-mode", "2")
            // 量化优化, 0: 禁用, 1: 仅用于最终编码, 2: 用于所有模式决策
            .insert("trellis", "1")
            .insert("threads", "auto")
            // 使用所有可用的分区模式
            .insert("partitions", "all")
            // 最小关键帧间隔
            .insert("keyint_min", "30")
            // 强制恒定帧率
            .insert("force-cfr", "1")
            // 启用切片线程
            .insert("sliced_threads", "1")
            // 禁用前瞻同步
            .insert("sync-lookahead", "0")
            // 减少前瞻帧数
            .insert("rc-lookahead", "10");
        opts
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
        let mut opts = Self::new();
        opts
            // p1-p7, default(p4), slow, medium, fast, hp, hq, bd, ll, llhq, llhp, lossless
            .insert("preset", "p5")
            // baseline, main, high, high444p, high10, high422
            .insert("profile", "high")
            // ll, ull, lossless, film, animation, grain, fastdecode, zerolatency, hq
            .insert("tune", "ll")
            // 设置比特率 4Mbps
            .insert("b", "4000k")
            .insert("maxrate", "5000k")
            .insert("bufsize", "8000k")
            // constqp, ll_2pass_size, ll_2pass_quality
            // vbr, vbr_hq, vbr_minqp, vbr_2pass
            // cbr, cbr_hq, cbr_ld_hq
            .insert("rc", "cbr")
            // 量化参数
            .insert("qmin", "10")
            .insert("qmax", "18")
            // 启用自适应量化
            .insert("spatial-aq", "1")
            .insert("temporal-aq", "1")
            .insert("aq-strength", "8")
            // GOP设置，较小的GOP有利于快速恢复和低延迟
            .insert("g", "30")
            // 禁用B帧以，避免出现画面闪烁
            .insert("bf", "0")
            .insert("b_ref_mode", "middle")
            // 启用场景切换检测，允许在场景变化时插入I帧
            .insert("no-scenecut", "0")
            // 低延时
            .insert("delay", "0")
            .insert("zerolatency", "1")
            // NVENC特有的参数
            // 增加表面缓冲区数量
            .insert("surfaces", "32")
            // 加权预测，改善低光照
            .insert("weighted_pred", "1");
        opts
    }
}

/// `HashMap<String, String>` -> `Options`
impl From<HashMap<String, String>> for Options {
    fn from(item: HashMap<String, String>) -> Self {
        Self(item.into_iter().collect())
    }
}

/// `BTreeMap<String, String>` -> `Options` (zero-copy: shares the internal
/// representation).
impl From<BTreeMap<String, String>> for Options {
    fn from(item: BTreeMap<String, String>) -> Self {
        Self(item)
    }
}

/// `Options` -> `BTreeMap<String, String>` (zero-copy: yields the internal
/// representation).
impl From<Options> for BTreeMap<String, String> {
    fn from(item: Options) -> Self {
        item.0
    }
}

/// `Options` -> `HashMap<String, String>`
impl From<Options> for HashMap<String, String> {
    fn from(item: Options) -> Self {
        item.0.into_iter().collect()
    }
}

/// Borrowing conversion (clones the entries).
impl From<&Options> for HashMap<String, String> {
    fn from(item: &Options) -> Self {
        item.0.clone().into_iter().collect()
    }
}

impl FromIterator<(String, String)> for Options {
    fn from_iter<I: IntoIterator<Item = (String, String)>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl std::fmt::Debug for Options {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.0, f)
    }
}

impl std::fmt::Display for Options {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

/// Video encoders whose FFmpeg wrapper exposes a `crf` private option.
///
/// Single source of truth: used by [`EncoderBuilder::with_quality`] /
/// [`Quality::Crf`] (fall back to bit-rate control for other codecs) and the
/// [`Quality::Crf`] capability documentation.
pub const CRF_CAPABLE_CODECS: &[&str] = &[
    "libx264",
    "libx265",
    "libvpx",
    "libvpx-vp9",
    "libaom-av1",
    "libsvtav1",
    "libopenh264",
];

/// Rate control strategy for video encode, set via
/// [`crate::EncoderBuilder::with_quality`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    /// Constant Rate Factor — quality-targeted, file size varies.
    /// Lower value = higher quality (x264 scale, sane range 0..=51,
    /// defaults around 18-28). Only supported by the encoders listed in
    /// [`CRF_CAPABLE_CODECS`]; others fall back to [`Quality::Bitrate`]
    /// with a warning.
    Crf(u8),
    /// Explicit target bit rate in bits per second.
    Bitrate(i64),
}

/// H.264-style encoding profile, set via [`crate::EncoderBuilder::with_profile`].
///
/// The variant name is passed as the codec's `profile` private option
/// string (`"baseline"`, `"main"`, `"high"`, ...). `libx264` supports all
/// variants directly; other encoders map what they support and ignore
/// unknown names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoProfile {
    Baseline,
    Main,
    High,
    High10,
    High422,
    High444,
}

impl VideoProfile {
    /// FFmpeg private-option string for this profile (x264 naming).
    pub fn as_option_str(&self) -> &'static str {
        match self {
            VideoProfile::Baseline => "baseline",
            VideoProfile::Main => "main",
            VideoProfile::High => "high",
            VideoProfile::High10 => "high10",
            VideoProfile::High422 => "high422",
            VideoProfile::High444 => "high444",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_options_debug() {
        let opts = Options::preset_h264_realtime();
        println!("{:?}", opts);
    }

    #[test]
    fn test_options_basic_ops() {
        let mut opts = Options::new();
        assert!(opts.is_empty());

        opts.insert("threads", "4").insert("preset", "fast");
        assert_eq!(opts.len(), 2);
        assert_eq!(opts.get("threads"), Some("4"));
        assert_eq!(opts.get("missing"), None);
        assert!(opts.contains_key("preset"));

        // overwrite semantics
        opts.insert("threads", "8");
        assert_eq!(opts.get("threads"), Some("8"));

        assert_eq!(opts.remove("preset"), Some("fast".to_string()));
        assert_eq!(opts.remove("preset"), None);
        assert_eq!(opts.len(), 1);
    }

    #[test]
    fn test_options_merge_other_wins() {
        let mut base = Options::new();
        base.insert("preset", "medium").insert("crf", "23");

        let mut overlay = Options::new();
        overlay.insert("crf", "18").insert("tune", "film");

        base.merge(overlay);
        assert_eq!(base.get("preset"), Some("medium"));
        assert_eq!(base.get("crf"), Some("18"));
        assert_eq!(base.get("tune"), Some("film"));
    }

    #[test]
    fn test_options_empty_to_dict_is_none() {
        // 根治旧实现：空 Options 物化为 None，不再携带 key="" 的脏条目。
        let opts = Options::new();
        assert!(opts.into_dict().is_none());
    }

    #[test]
    fn test_options_dict_roundtrip() {
        let mut opts = Options::new();
        opts.insert("crf", "23").insert("profile", "high");

        let dict = opts.to_dict().expect("non-empty must materialize");
        let back = Options::from_dict(&dict);
        assert_eq!(back.get("crf"), Some("23"));
        assert_eq!(back.get("profile"), Some("high"));
        assert_eq!(back.len(), 2);
    }

    #[test]
    fn test_options_hashmap_roundtrip() {
        let mut map = HashMap::new();
        map.insert("a".to_string(), "1".to_string());

        let opts: Options = map.clone().into();
        assert_eq!(opts.get("a"), Some("1"));

        let back: HashMap<String, String> = opts.into();
        assert_eq!(back, map);
    }

    #[test]
    fn test_options_btreemap_roundtrip() {
        let mut map = BTreeMap::new();
        map.insert("a".to_string(), "1".to_string());
        map.insert("b".to_string(), "2".to_string());

        let opts: Options = map.clone().into();
        assert_eq!(opts.get("b"), Some("2"));

        let back: BTreeMap<String, String> = opts.into();
        assert_eq!(back, map);
    }

    #[test]
    fn test_options_from_raw_dict_null_is_empty() {
        // FFmpeg 的空字典就是 NULL 指针：from_raw_dict(NULL) 必须得到空 Options。
        let opts = unsafe { Options::from_raw_dict(std::ptr::null_mut()) };
        assert!(opts.is_empty());
    }

    #[test]
    fn test_options_raw_dict_read() {
        let mut dict = AVDictionary::new(c"title", c"hello", 0);
        // SAFETY: `dict` 是合法字典，且在本调用期间存活。
        let opts = unsafe { Options::from_raw_dict(dict.as_mut_ptr()) };
        assert_eq!(opts.get("title"), Some("hello"));
        assert_eq!(opts.len(), 1);
    }

    #[test]
    fn test_options_write_into_raw_dict_roundtrip() {
        let mut opts = Options::new();
        opts.insert("title", "hello").insert("artist", "rsmedia");

        let mut dest: *mut ffi::AVDictionary = std::ptr::null_mut();
        // SAFETY: `dest` 为 NULL，函数按"空槽位"处理。
        unsafe { opts.write_into_raw_dict(&mut dest) };
        assert!(!dest.is_null(), "non-empty options must materialize");

        // SAFETY: `dest` 刚由 to_dict 物化，合法且存活。
        let back = unsafe { Options::from_raw_dict(dest) };
        assert_eq!(back.get("title"), Some("hello"));
        assert_eq!(back.get("artist"), Some("rsmedia"));

        // SAFETY: 释放测试自建的字典，避免泄漏。
        unsafe { ffi::av_dict_free(&mut dest) };
        assert!(dest.is_null());
    }

    #[test]
    fn test_options_write_into_raw_dict_empty_stores_null() {
        let mut dest: *mut ffi::AVDictionary = std::ptr::null_mut();
        // SAFETY: `dest` 为 NULL。
        unsafe { Options::new().write_into_raw_dict(&mut dest) };
        assert!(
            dest.is_null(),
            "empty Options must store NULL (no metadata)"
        );

        // 已有内容时写入空 Options：旧字典被释放并清回 NULL。
        let owned = AVDictionary::new(c"title", c"hello", 0);
        let mut dest = owned.into_raw().as_ptr();
        // SAFETY: `dest` 指向刚转移的合法字典。
        unsafe { Options::new().write_into_raw_dict(&mut dest) };
        assert!(dest.is_null());
    }

    #[test]
    fn test_options_presets_fixed_keys() {
        // "profile:v"/"b:v" 是 ffmpeg CLI 语法，AVOption 查找不到会被静默
        // 忽略；preset 必须使用真正的 AVOption 名。
        assert_eq!(Options::preset_h264().get("profile"), Some("high"));
        assert_eq!(Options::preset_h264().get("profile:v"), None);

        let rt = Options::preset_h264_realtime();
        assert_eq!(rt.get("profile"), Some("main"));
        assert_eq!(rt.get("b"), Some("3000k"));
        assert_eq!(rt.get("b:v"), None);
        // x264opts 是 CLI 组合器，AVOption 中不存在
        assert_eq!(rt.get("x264opts"), None);

        assert_eq!(Options::preset_h264_nvenc().get("b"), Some("4000k"));
    }
}
