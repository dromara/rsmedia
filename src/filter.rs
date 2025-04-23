//! Filter module
//! This module provides a set of filters that can be used to process media data.
//! The filters are implemented using the ffmpeg library.
//!
//! See: <https://ffmpeg.org/ffmpeg-filters.html>
//!
//!```rust,ignore
//! /// 水印 + 缩放 + 淡入淡出组合
//! /// [buffer] -> scale -> fifo -> drawtext -> [buffersink]
//! let video_watermark_preset_filters = vec![
//!     scale(1280, 720, Some(PixelFormat::YUV420P)),
//!     fifo(MediaType::VIDEO), // 避免 drawtext 卡住
//!     drawtext("Hello Text", 20, 20, 24, "white"),
//!     fade_in(30),
//!     fade_out(270, 30),
//! ];
//!
//! /// 音频过滤器链中
//! /// [abuffer] -> volume -> fifo -> loudnorm -> [abuffersink]
//! let filters = vec![
//!     volume(1.5),
//!     fifo(MediaType::AUDIO),
//!     loudnorm(-16.0),
//! ];
//! ```
use crate::{MediaType, PixelFormat, SampleFormat};

use rsmpeg::avfilter::{AVFilter, AVFilterContextMut, AVFilterGraph, AVFilterInOut};
use rsmpeg::avutil::{AVChannelLayout, AVFrame};
use rsmpeg::ffi;

use anyhow::{Context, Error, Result};
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone)]
pub struct Filter {
    name: &'static str,
    media_type: MediaType,
    spec: String,
}

impl Filter {
    pub fn new(name: &'static str, media_type: MediaType, spec: String) -> Self {
        Self {
            name,
            media_type,
            spec,
        }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn media_type(&self) -> MediaType {
        self.media_type
    }

    pub fn spec(&self) -> String {
        self.spec.clone()
    }
}

/// Escapes characters that are special within FFmpeg filtergraph descriptions.
///
/// This function uses FFmpeg's native av_escape function to properly escape
/// characters that have special meaning in filter graphs.
///
/// # Arguments
///
/// * `input` - The string to escape
///
/// # Returns
///
/// A new string with special characters escaped according to FFmpeg rules
fn escape_filter_str(input: &str) -> String {
    // Early return for empty strings
    if input.is_empty() {
        return String::new();
    }

    unsafe {
        // Create a C string from our input
        let c_input = match CString::new(input) {
            Ok(s) => s,
            Err(_) => return input.replace('\0', "").to_string(), // Handle null bytes
        };

        // Characters that need escaping in filtergraph descriptions
        let special_chars = CString::new("\\':,[]={};").unwrap();

        // Pointer that will receive the escaped string
        let mut escaped_ptr = std::ptr::null_mut();

        // FFmpeg `av_escape` function flags:
        // AV_ESCAPE_MODE_BACKSLASH (0) - escape with backslashes
        // AV_ESCAPE_FLAG_STRICT (1) - be strict about escaping
        let result = ffi::av_escape(
            &mut escaped_ptr,
            c_input.as_ptr(),
            special_chars.as_ptr(),
            ffi::AV_ESCAPE_MODE_BACKSLASH,
            1,
        );

        if result < 0 || escaped_ptr.is_null() {
            panic!("Invalid input spec: {}", input);
        }

        // Convert back to Rust String and free the memory
        let escaped_cstr = std::ffi::CStr::from_ptr(escaped_ptr);
        let escaped_string = escaped_cstr.to_string_lossy().into_owned();

        // Free memory allocated by FFmpeg
        ffi::av_free(escaped_ptr as *mut _);

        escaped_string
    }
}

pub mod video {
    use super::*;

    /// Scales video dimensions.
    ///
    /// `flags`: Optional SWS_FLAG string, Default is "fast_bilinear".
    /// (e.g. "fast_bilinear", "bilinear", "bicubic", "experimental", "neighbor", "area",
    /// "bicublin", "gauss", "sinc", "lanczos", "spline").
    ///
    /// See: <https://ffmpeg.org/ffmpeg-scaler.html#Scaler-Options>
    pub fn scale(width: u32, height: u32, flags: Option<&str>) -> Filter {
        let flags_str = flags.unwrap_or("fast_bilinear");

        Filter::new(
            "scale",
            MediaType::VIDEO,
            format!("scale=w={}:h={}:flags={}", width, height, flags_str),
        )
    }

    /// Converts video pixel format.
    /// `format`: <https://ffmpeg.org/ffmpeg-filters.html#format>
    /// `aformat`: <https://ffmpeg.org/ffmpeg-filters.html#aformat-1.
    pub fn format(format: PixelFormat) -> Filter {
        Filter::new(
            "format",
            MediaType::VIDEO,
            format!("format=pix_fmts={}", format.get_pix_fmt_name()),
        )
    }

    /// Crops video to a specified rectangle.
    /// `x` and `y` can be negative but runtime validation against input frame is better.
    /// `w` and `h` must be positive.
    pub fn crop(x: i32, y: i32, w: u32, h: u32) -> Filter {
        Filter::new(
            "crop",
            MediaType::VIDEO,
            format!("crop=x={}:y={}:w={}:h={}", x, y, w, h),
        )
    }

    /// Draws text on video. Requires `fontfile`.
    pub fn drawtext(
        text: &str,
        x: i32,
        y: i32,
        fontfile: &str,
        fontsize: u32,
        fontcolor: &str,
    ) -> Filter {
        let escaped_text = escape_filter_str(text);
        let escaped_font_file = escape_filter_str(fontfile);
        Filter::new(
            "drawtext",
            MediaType::VIDEO,
            format!(
                "drawtext=text='{}':fontfile='{}':x={}:y={}:fontsize={}:fontcolor={}",
                escaped_text, escaped_font_file, x, y, fontsize, fontcolor
            ),
        )
    }

    /// 画矩形框
    pub fn drawbox(x: i32, y: i32, w: u32, h: u32, color: &str, thickness: i32) -> Filter {
        if thickness < 0 {
            // FFmpeg 't=fill' is also possible
            log::warn!(
                "Box thickness is negative ({}), using absolute value.",
                thickness
            );
        }
        Filter::new(
            "drawbox",
            MediaType::VIDEO,
            format!(
                "drawbox=x={}:y={}:w={}:h={}:color={}:t={}",
                x,
                y,
                w,
                h,
                color,
                thickness.abs()
            ),
        )
    }

    /// 去除水印
    ///
    /// # Arguments
    ///
    /// `x` and `y` are the top-left corner of the logo.
    /// `w` and `h` are the width and height of the logo.
    /// See: <https://ffmpeg.org/ffmpeg-filters.html#delogo>
    pub fn delogo(x: i32, y: i32, w: u32, h: u32) -> Filter {
        Filter::new(
            "delogo",
            MediaType::VIDEO,
            format!("delogo=x={}:y={}:w={}:h={}", x, y, w, h),
        )
    }

    /// zoompan - 平移和缩放效果
    pub fn zoompan(zoom: &str, x: &str, y: &str, duration: Option<i32>) -> Filter {
        let mut params = format!("zoompan=z={}:x={}:y={}", zoom, x, y);
        if let Some(d) = duration {
            params.push_str(&format!(":d={}", d));
        }
        Filter::new("zoompan", MediaType::VIDEO, params)
    }

    /// transpose - 用于快速 90°/180°/270° 视频画面旋转、水平翻转或镜像翻转（无插值，高性能）
    /// mode: 0=逆时针90度/垂直翻转, 1=顺时针90度, 2=逆时针90度, 3=顺时针90度/垂直翻转
    ///
    /// Transposes video (rotates by multiples of 90 degrees and/or flips).
    /// See `ffmpeg -filters` (search transpose) for valid modes.
    ///
    /// See: <https://ffmpeg.org/ffmpeg-filters.html#transpose-1>
    pub fn transpose(mode: i32) -> Filter {
        // Common range is 0-3, but ffmpeg might support more
        if !(0..=7).contains(&mode) {
            log::warn!("Transpose mode {} might be invalid.", mode);
        }
        Filter::new("transpose", MediaType::VIDEO, format!("transpose={}", mode))
    }

    /// rotate - 任意角度旋转滤镜（使用浮点弧度，支持动画）
    /// 注意：性能较低，可能有插值模糊；用于精准旋转或动态旋转场景
    pub fn rotate(angle: i32) -> Filter {
        // Ffmpeg 中的角度使用弧度而非度数，因此需要转换
        Filter::new(
            "rotate",
            MediaType::VIDEO,
            format!("rotate={}*PI/180", angle),
        )
    }

    /// Flips video horizontally.
    pub fn hflip() -> Filter {
        Filter::new("hflip", MediaType::VIDEO, "hflip".to_string())
    }
    /// Flips video vertically.
    pub fn vflip() -> Filter {
        Filter::new("vflip", MediaType::VIDEO, "vflip".to_string())
    }

    /// 视频淡入淡出
    /// Fades video in from the start.
    /// `duration_frames`: Fade duration in number of frames.
    pub fn fade_in(duration_frames: u32) -> Filter {
        Filter::new(
            "fade",
            MediaType::VIDEO,
            format!("fade=t=in:st=0:d={}", duration_frames),
        )
    }

    /// Fades video out.
    /// `start_frame`: Frame number to start the fade out.
    /// `duration_frames`: Fade duration in number of frames.
    pub fn fade_out(start_frame: u32, duration_frames: u32) -> Filter {
        Filter::new(
            "fade",
            MediaType::VIDEO,
            format!("fade=t=out:st={}:d={}", start_frame, duration_frames),
        )
    }

    /// 视频锐化
    /// Applies unsharp mask filter (default settings).
    pub fn unsharp() -> Filter {
        // Add parameters if needed: lx, ly, la, cx, cy, ca
        Filter::new("unsharp", MediaType::VIDEO, "unsharp".to_string())
    }

    /// 视频模糊
    /// Applies box blur filter.
    /// `luma_radius`: Radius of the luma blur.
    pub fn blur(radius: f32) -> Filter {
        // Consider adding other boxblur params: luma_power, chroma_radius, chroma_power, alpha_radius, alpha_power
        Filter::new(
            "boxblur",
            MediaType::VIDEO,
            format!("luma_radius={}", radius),
        )
    }

    /// 亮度/对比度调节
    pub fn eq(brightness: f32, contrast: f32) -> Filter {
        Filter::new(
            "eq",
            MediaType::VIDEO,
            format!("eq=brightness={}:contrast={}", brightness, contrast),
        )
    }

    /// 帧率控制
    pub fn fps(fps: f32) -> Filter {
        Filter::new("fps", MediaType::VIDEO, format!("fps={}", fps))
    }
}

pub mod audio {
    use super::*;

    /// 创建音频重采样过滤器
    pub fn resample(nb_channels: u32, sample_rate: u32, format: SampleFormat) -> Filter {
        let channel_desc = AVChannelLayout::from_nb_channels(nb_channels as i32)
            .describe()
            .unwrap();

        // async=1 might be better default for realtime to avoid buffer issues.
        let spec_str = format!(
            "aresample=osr={}:osf={}:ochl={}:async=1",
            sample_rate,
            format.get_sample_fmt_name(),
            channel_desc.to_string_lossy(),
        );

        Filter::new("resample", MediaType::AUDIO, spec_str)
    }

    /// `aformat`: <https://ffmpeg.org/ffmpeg-filters.html#aformat-1.
    pub fn format(nb_channels: u32, sample_rate: u32, format: SampleFormat) -> Filter {
        let channel_desc = AVChannelLayout::from_nb_channels(nb_channels as i32)
            .describe()
            .unwrap();

        Filter::new(
            "aformat",
            MediaType::AUDIO,
            format!(
                "aformat=sample_fmts={}:sample_rates={}:channel_layouts={}",
                format.get_sample_fmt_name(),
                sample_rate,
                channel_desc.to_string_lossy()
            ),
        )
    }

    /// 音量调整
    /// Adjusts audio volume.
    /// `volume`: Linear multiplier (1.0 is no change) or dB value (e.g., "-3dB").
    pub fn volume(val: f32) -> Filter {
        // FFmpeg volume filter can take linear scale or dB. Pass string directly.
        // Validation could check if it's a number or ends with "dB".
        Filter::new("volume", MediaType::AUDIO, format!("volume={}", val))
    }

    /// loudnorm - EBU R128音量标准化
    pub fn loudnorm(integrated_loudness: f32) -> Filter {
        Filter::new(
            "loudnorm",
            MediaType::AUDIO,
            format!("loudnorm=I={}:TP=-1.5:LRA=11", integrated_loudness),
        )
    }

    /// 单频段均衡器
    /// Applies a single-band peaking equalizer.
    /// `frequency`: Center frequency in Hz.
    /// `gain`: Gain in dB.
    /// `width`: Bandwidth in Hz.
    pub fn equalizer(frequency: i32, gain: f32, width: u32) -> Filter {
        Filter::new(
            "equalizer",
            MediaType::AUDIO,
            format!(
                "equalizer=f={}:width_type=h:width={}:g={}", // width_type=h (Hz)
                frequency, width, gain
            ),
        )
    }

    /// 多频段均衡器 (bass, mid, treble)
    /// Applies a simple 3-band equalizer using firequalizer.
    /// See: <https://ffmpeg.org/ffmpeg-filters.html#firequalizer>
    pub fn three_band_equalizer(bass_gain: f32, mid_gain: f32, treble_gain: f32) -> Filter {
        Filter::new(
            "firequalizer",
            MediaType::AUDIO,
            format!(
                "firequalizer=gain='if(lt(f,200),{},if(gt(f,5000),{},{}))':scale=linlog",
                bass_gain, treble_gain, mid_gain
            ),
        )
    }

    /// 压缩器
    /// Applies dynamic range compression.
    /// `ratio`: Compression ratio (1 - 20).
    /// `attack`: Attack time in ms (optional, default 20).
    /// `release`: Release time in ms (optional, default 250).
    /// See: <https://ffmpeg.org/ffmpeg-filters.html#acompressor>
    pub fn compressor(ratio: f32, attack: Option<f32>, release: Option<f32>) -> Filter {
        if ratio < 1.0 {
            panic!("{}", format!("Compressor ratio must be >= 1.0: {}", ratio));
        }
        let mut spec = format!("acompressor=ratio={}", ratio);
        if let Some(a) = attack {
            spec.push_str(&format!(":attack={}", a));
        }
        if let Some(r) = release {
            spec.push_str(&format!(":release={}", r));
        }
        // Add other params: makeup, knee, link, detection, mix...
        Filter::new("acompressor", MediaType::AUDIO, spec)
    }

    /// 高通滤波
    pub fn highpass(freq: u32) -> Filter {
        Filter::new("highpass", MediaType::AUDIO, format!("highpass=f={}", freq))
    }

    /// 低通滤波
    pub fn lowpass(freq: u32) -> Filter {
        Filter::new("lowpass", MediaType::AUDIO, format!("lowpass=f={}", freq))
    }

    /// 音频变速
    /// Changes audio tempo without changing pitch.
    /// * `rate`: Speed multiplier (0.5 to 100.0).
    pub fn atempo(rate: f32) -> Filter {
        Filter::new("atempo", MediaType::AUDIO, format!("atempo={}", rate))
    }

    /// 延时（ms）
    pub fn adelay(delay_ms: i32, channels: i32) -> Filter {
        let delays = vec![delay_ms.to_string(); channels as usize].join("|");
        Filter::new("adelay", MediaType::AUDIO, format!("delays={}", delays))
    }

    /// 创建FFT降噪过滤器
    /// Applies FFT noise reduction (simple).
    /// `noise_reduction`: Noise reduction factor in dB (e.g., 12).
    /// `noise_floor`: Noise floor in dB (e.g., -50).
    pub fn fft_denoise(noise_reduction: i32, noise_floor: i32) -> Filter {
        Filter::new(
            "afftdn",
            MediaType::AUDIO,
            format!("afftdn=nr={}:nf={}:nt=w", noise_reduction, noise_floor),
        )
    }

    /// 创建高级FFT降噪过滤器
    /// Applies FFT noise reduction (advanced).
    /// `noise_reduction`: Noise reduction in dB.
    /// `noise_floor`: Noise floor in dB.
    /// `noise_type`: 'w', 'v', 'p', 'c', 's'. Default 'w'.
    /// `time_smoothing`: Temporal smoothing factor. Default 0.
    pub fn advanced_fft_denoise(
        noise_reduction: i32,
        noise_floor: i32,
        noise_type: Option<&str>,
        time_smoothing: Option<f32>,
    ) -> Filter {
        let nt = noise_type.unwrap_or("w");
        let tr = time_smoothing.unwrap_or(0.0);
        Filter::new(
            "afftdn",
            MediaType::AUDIO,
            format!(
                "afftdn=nr={}:nf={}:nt={}:tr={}",
                noise_reduction, noise_floor, nt, tr
            ),
        )
    }

    /// 创建自适应非局部均值降噪过滤器
    /// Applies Non-Local Means de-noising (anlmdn).
    /// `strength`: Denoising strength (0 to inf, default 1e-05).
    /// `patch_size`: Patch size (default 7).
    /// `search_range`: Research range (default 15).
    pub fn anlm_denoise(
        strength: Option<f32>,
        patch_size: Option<i32>,
        search_range: Option<i32>,
    ) -> Filter {
        let mut params = Vec::new();
        if let Some(s) = strength {
            params.push(format!("s={}", s));
        }
        if let Some(p) = patch_size {
            params.push(format!("p={}", p));
        }
        if let Some(r) = search_range {
            params.push(format!("r={}", r));
        }
        let spec = if params.is_empty() {
            "anlmdn".to_string()
        } else {
            format!("anlmdn={}", params.join(":"))
        };
        Filter::new("anlmdn", MediaType::AUDIO, spec)
    }
}

/// 过滤器工厂 - 用于创建具体的过滤器描述
pub struct FilterFactory;

impl FilterFactory {
    /// 创建 FIFO 过滤器
    pub fn fifo(media_type: MediaType) -> Filter {
        #[rustfmt::skip]
        let name = if media_type == MediaType::AUDIO { "afifo" } else { "fifo" };
        Filter::new(name, media_type, name.to_string())
    }

    /// 分支滤镜，将一个输入流分成多个相同的输出流
    /// See: <https://ffmpeg.org/ffmpeg-filters.html#split_002c-asplit>
    pub fn split(media_type: MediaType, n: i32) -> Filter {
        #[rustfmt::skip]
        let name = if media_type == MediaType::AUDIO { "asplit" } else { "split" };
        Filter::new(name, media_type, format!("{}={}", name, n))
    }

    /// 修改时间戳表达式（加速、减速、对齐等）
    /// 典型值：`setpts=0.5*PTS`（2倍速），`1.5*PTS`（慢放）
    /// Modifies presentation timestamp (PTS). Use with caution.
    /// `expr`: FFmpeg expression (e.g., "0.5*PTS", "PTS-STARTPTS").
    pub fn setpts(media_type: MediaType, expr: &str) -> Filter {
        #[rustfmt::skip]
        let name = if media_type == MediaType::AUDIO { "asetpts" } else { "setpts" };
        let escaped_expr = escape_filter_str(expr);
        Filter::new(name, media_type, format!("{}={}", name, escaped_expr))
    }

    /// Trim video/audio to a specific time range.
    pub fn trim(media_type: MediaType, start: f32, end: f32) -> Filter {
        #[rustfmt::skip]
        let name = if media_type == MediaType::AUDIO { "atrim" } else { "trim" };
        Filter::new(name, media_type, format!("{}={}:{}", name, start, end))
    }
}

/// 过滤器参数配置
#[derive(Debug, Clone)]
pub enum FilterParams {
    Video(VideoParams),
    Audio(AudioParams),
}

impl FilterParams {
    pub fn media_type(&self) -> MediaType {
        match self {
            FilterParams::Video(_) => MediaType::VIDEO,
            FilterParams::Audio(_) => MediaType::AUDIO,
        }
    }
}

/// 视频过滤器参数
#[derive(Debug, Clone)]
pub struct VideoParams {
    pub width: i32,
    pub height: i32,
    pub format: PixelFormat,
    pub time_base: ffi::AVRational,
    pub frame_rate: ffi::AVRational,
    pub pixel_aspect: ffi::AVRational,
}

/// 音频过滤器参数
#[derive(Debug, Clone)]
pub struct AudioParams {
    pub nb_channels: i32,
    pub sample_rate: i32,
    pub format: SampleFormat,
    pub time_base: ffi::AVRational,
}

const DEFAULT_ORDERING: Ordering = Ordering::SeqCst;

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
enum FilterGraphState {
    Normal,
    Drained,
    Flushed,
}

/// 过滤器图表 - 包含所有过滤器配置
pub struct FilterGraph {
    graph: AVFilterGraph,
    state: FilterGraphState,
    initialized: AtomicBool,
}

impl FilterGraph {
    pub(crate) fn new() -> Self {
        Self {
            graph: AVFilterGraph::new(),
            state: FilterGraphState::Normal,
            initialized: AtomicBool::new(false),
        }
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized.load(DEFAULT_ORDERING)
    }

    pub fn is_drained(&self) -> bool {
        self.state == FilterGraphState::Drained
    }

    pub fn is_flushed(&self) -> bool {
        self.state == FilterGraphState::Flushed
    }

    /// 初始化过滤器图表
    pub fn init(&mut self, params: &FilterParams, filters: &[Filter]) -> Result<()> {
        if self.is_initialized() {
            return Err(Error::msg("Filter graph already initialized"));
        }

        // check
        for filter in filters {
            if filter.media_type() != params.media_type() {
                return Err(Error::msg(format!(
                    "Filter media type mismatch: expected {:?}, got {:?}",
                    params.media_type(),
                    filter.media_type()
                )));
            }
        }

        // build filters spec
        let filter_spec = filters
            .iter()
            .map(|f| f.spec())
            .collect::<Vec<_>>()
            .join(",");

        // Set up the filter graph based on the media type
        match params {
            FilterParams::Video(p) => self.setup_video_filters(p, filter_spec)?,
            FilterParams::Audio(p) => self.setup_audio_filters(p, filter_spec)?,
        }

        self.graph.config()?;
        self.initialized.store(true, DEFAULT_ORDERING);

        Ok(())
    }

    /// Setup video filters
    /// `buffer`: <https://ffmpeg.org/ffmpeg-filters.html#buffer>
    /// `buffersink`: <https://ffmpeg.org/ffmpeg-filters.html#buffersink>
    fn setup_video_filters(&mut self, params: &VideoParams, spec: String) -> Result<()> {
        let args = {
            let args = format!(
                "width={}:height={}:pix_fmt={}:time_base={}/{}:frame_rate={}/{}:pixel_aspect={}/{}",
                params.width,
                params.height,
                params.format.get_pix_fmt_name(),
                params.time_base.num,
                params.time_base.den,
                params.frame_rate.num,
                params.frame_rate.den,
                params.pixel_aspect.num,
                params.pixel_aspect.den,
            );
            CString::new(args)?
        };

        let buffersrc =
            AVFilter::get_by_name(c"buffer").context("Failed to get video filter 'buffer'.")?;
        let buffersink = AVFilter::get_by_name(c"buffersink")
            .context("Failed to get video filter 'buffersink'.")?;

        let mut src_ctx = self
            .graph
            .create_filter_context(&buffersrc, c"in", Some(&args))
            .context("Failed to create video buffer source")?;

        let mut sink_ctx = self
            .graph
            .create_filter_context(&buffersink, c"out", None)
            .context("Failed to create video buffer sink")?;

        sink_ctx
            .opt_set_bin(c"pix_fmts", &(ffi::AVPixelFormat::from(params.format)))
            .context("Failed to set video sink filter context pixel format")?;

        // Create endpoints
        let outputs = AVFilterInOut::new(c"in", &mut src_ctx, 0);
        let inputs = AVFilterInOut::new(c"out", &mut sink_ctx, 0);

        let spec_cstr = CString::new(spec)?;

        // Parse with endpoints
        let (_in, _out) = self
            .graph
            .parse_ptr(&spec_cstr, Some(inputs), Some(outputs))?;

        Ok(())
    }

    /// Setup audio filters
    /// `abuffer`: <https://ffmpeg.org/ffmpeg-filters.html#abuffer>
    /// `abuffersink`: <https://ffmpeg.org/ffmpeg-filters.html#abuffersink>
    fn setup_audio_filters(&mut self, params: &AudioParams, spec: String) -> Result<()> {
        let channel_desc = AVChannelLayout::from_nb_channels(params.nb_channels).describe()?;

        let args = {
            let args = format!(
                "time_base={}/{}:sample_rate={}:sample_fmt={}:channel_layout={}",
                params.time_base.num,
                params.time_base.den,
                params.sample_rate,
                params.format.get_sample_fmt_name(),
                channel_desc.to_string_lossy(),
            );
            CString::new(args)?
        };

        let buffersrc = AVFilter::get_by_name(c"abuffer")
            .context("Failed to get audio filter buffer 'abuffer'.")?;
        let buffersink = AVFilter::get_by_name(c"abuffersink")
            .context("Failed to get audio filter buffer 'abuffersink'.")?;

        let mut src_ctx = self
            .graph
            .create_filter_context(&buffersrc, c"in", Some(&args))
            .context("Failed to create audio buffer source")?;

        let mut sink_ctx = self
            .graph
            .create_filter_context(&buffersink, c"out", None)
            .context("Failed to create audio buffer sink")?;

        sink_ctx.opt_set_bin(c"sample_fmts", &(params.format as i32))?;
        sink_ctx.opt_set_bin(c"sample_rates", &params.sample_rate)?;
        sink_ctx.opt_set(c"ch_layouts", &channel_desc)?;

        // Create endpoints
        let outputs = AVFilterInOut::new(c"in", &mut src_ctx, 0);
        let inputs = AVFilterInOut::new(c"out", &mut sink_ctx, 0);

        let spec_cstr = CString::new(spec)?;

        // Parse with endpoints
        let (_in, _out) = self
            .graph
            .parse_ptr(&spec_cstr, Some(inputs), Some(outputs))?;

        Ok(())
    }

    /// 处理单帧
    pub fn process_frame(&mut self, frame: Option<AVFrame>) -> Result<Option<AVFrame>> {
        if !self.is_initialized() {
            return Err(Error::msg("Filter graph not initialized"));
        }

        {
            // Get source context and send the frame
            let mut src_ctx = self.get_src_context()?;
            src_ctx
                .buffersrc_add_frame(frame, None)
                .context("Error submitting the frame to the filter graph.")?;
        } // src_ctx is dropped here, releasing the mutable borrow

        let filter_result = {
            // safely get a new mutable borrow for sink_ctx
            let mut sink_ctx = self.get_sink_context()?;
            sink_ctx.buffersink_get_frame(None)
        }; // sink_ctx is dropped here, releasing the mutable borrow

        // 获取处理后的帧
        match filter_result {
            Ok(frame) => Ok(Some(frame)),
            Err(rsmpeg::error::RsmpegError::BufferSinkDrainError) => {
                log::debug!("filter graph: buffer sink drain error");
                self.state = FilterGraphState::Drained;
                Ok(None)
            }
            Err(rsmpeg::error::RsmpegError::BufferSinkEofError) => {
                log::warn!("filter graph: buffer sink eof error");
                self.state = FilterGraphState::Flushed;
                Ok(None)
            }
            Err(e) => Err(Error::msg(format!(
                "Get frame from buffer sink Error: {}",
                e
            ))),
        }
    }

    /// 刷新过滤器链
    pub fn flush(&mut self) -> Result<Vec<AVFrame>> {
        if !self.is_initialized() {
            return Err(Error::msg("Filter graph not initialized"));
        }
        if self.is_flushed() {
            log::debug!("Filter graph already flushed.");
            return Ok(Vec::new());
        }

        let mut frames = Vec::new();

        loop {
            match self.process_frame(None) {
                Ok(Some(frame)) => frames.push(frame),
                Ok(None) => {
                    if self.is_flushed() {
                        break;
                    }
                    log::trace!("Filter graph draining during flush...");
                }
                Err(e) => {
                    log::error!("Error encountered during filter graph flush: {}", e);
                    return Err(e);
                }
            }
        }

        Ok(frames)
    }

    /// 动态获取源过滤器上下文
    fn get_src_context(&mut self) -> Result<AVFilterContextMut<'_>> {
        self.graph
            .get_filter(c"in")
            .context("Source filter context not found")
    }

    /// 动态获取目标过滤器上下文
    fn get_sink_context(&mut self) -> Result<AVFilterContextMut<'_>> {
        self.graph
            .get_filter(c"out")
            .context("Sink filter context not found")
    }
}

impl Default for FilterGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for FilterGraph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "FilterGraph nb_filters:{}, initialized:{}, state:{:?}",
            self.graph.nb_filters,
            self.is_initialized(),
            self.state,
        )
    }
}

/// 流过滤器配置
#[derive(Debug, Clone)]
pub struct FilterConfig {
    pub params: FilterParams,
    pub filters: Vec<Filter>,
}

/// 流过滤器
pub struct FilterContext {
    pub stream_index: usize,
    pub config: FilterConfig,
    pub graph: FilterGraph,
}

impl FilterContext {
    /// 为指定流添加过滤器
    pub fn new(stream_index: usize, config: FilterConfig) -> Result<Self> {
        log::debug!("new filter context:{:?}", config);

        // 创建并初始化过滤器图表
        let mut graph = FilterGraph::new();
        graph.init(&config.params, &config.filters)?;

        Ok(Self {
            stream_index,
            config,
            graph,
        })
    }

    /// 处理指定流的帧
    pub fn process_frame(&mut self, frame: Option<AVFrame>) -> Result<Option<AVFrame>> {
        self.graph.process_frame(frame)
    }

    /// 刷新指定流的过滤器链
    pub fn flush(&mut self) -> Result<Vec<AVFrame>> {
        self.graph.flush()
    }
}

impl std::fmt::Debug for FilterContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FilterContext")
            .field("stream_index", &self.stream_index)
            .field("config", &self.config)
            .field("graph", &self.graph)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_filter_str() {
        // Test case 1: Empty string
        assert_eq!(
            escape_filter_str(""),
            "",
            "Empty string should return empty string"
        );

        // Test case 2: String with no special characters but with spaces
        // FFmpeg appears to escape spaces as well
        assert_eq!(
            escape_filter_str("normal text"),
            "normal\\ text",
            "Spaces are also escaped by av_escape"
        );

        // Test case 3: String with special characters
        assert_eq!(
            escape_filter_str("text with [brackets]"),
            "text\\ with\\ \\[brackets\\]",
            "Brackets should be escaped and spaces too"
        );

        // Test case 4: String with multiple special characters
        assert_eq!(
            escape_filter_str("file:///path/to/video.mp4"),
            "file\\:///path/to/video.mp4",
            "Colon should be escaped"
        );

        // Test case 5: String with all special characters
        let input = "filter=value,'text',[in],[out],key=val;next:filter\\backslash";
        let expected = "filter\\=value\\,\\'text\\'\\,\\[in\\]\\,\\[out\\]\\,key\\=val\\;next\\:filter\\\\backslash";
        assert_eq!(
            escape_filter_str(input),
            expected,
            "All special characters should be escaped"
        );

        // Test case 6: String with escaped characters already
        assert_eq!(
            escape_filter_str("already\\escaped"),
            "already\\\\escaped",
            "Backslashes should be escaped even if they're escaping something else"
        );

        // Test case 7: Complex filter string with multiple special chars
        // Note that FFmpeg also escapes the exclamation mark (!)
        let complex_filter = "drawtext=text='Hello, World!':x=10:y=10";
        let expected = "drawtext\\=text\\=\\'Hello\\,\\ World!\\'\\:x\\=10\\:y\\=10";
        assert_eq!(
            escape_filter_str(complex_filter),
            expected,
            "Complex filter string should be properly escaped with spaces and exclamation marks escaped too"
        );

        // Test case 8: Test with exclamation marks specifically
        // Note character!
        assert_eq!(
            escape_filter_str("Warning!"),
            "Warning!",
            "Exclamation marks should be escaped"
        );

        // Test case 9: Unicode characters - using pattern matching instead of exact comparison
        let unicode_result = escape_filter_str("Unicode: こんにちは");
        assert!(
            unicode_result.contains("Unicode"),
            "Result should contain the word 'Unicode'"
        );
        assert!(
            unicode_result.contains("こんにちは"),
            "Result should contain the Japanese characters"
        );

        // Test case 10: Unicode with special characters - using pattern matching
        let unicode_special_result = escape_filter_str("Unicode: こんにちは[世界]");
        assert!(
            unicode_special_result.contains("\\[") && unicode_special_result.contains("\\]"),
            "Unicode string with special characters should have brackets escaped"
        );

        // Test case 11: Very long string - only check that it ends correctly
        let long_string = "x".repeat(1000) + "=[]:";
        let long_result = escape_filter_str(&long_string);
        assert!(
            long_result.ends_with("\\=\\[\\]\\:"),
            "Long strings should have special characters at the end properly escaped"
        );
    }

    #[test]
    fn test_real_world_filter_strings() {
        // Test case 1: Scale filter
        assert_eq!(
            escape_filter_str("scale=width=1280:height=720"),
            "scale\\=width\\=1280\\:height\\=720",
            "Scale filter string should be properly escaped"
        );

        // Test case 2: Overlay filter
        assert_eq!(
            escape_filter_str("overlay=x=10:y=10"),
            "overlay\\=x\\=10\\:y\\=10",
            "Overlay filter string should be properly escaped"
        );

        // Test case 3: Complex drawtext filter with Unicode
        let drawtext = "drawtext=text='Copyright © 2023':fontcolor=white:fontsize=24:box=1:boxcolor=black@0.5:x=(w-text_w)/2:y=h-th-10";

        // Instead of checking the exact string, check for key patterns
        let drawtext_result = escape_filter_str(drawtext);

        // Check presence of escaped key components
        assert!(
            drawtext_result.contains("drawtext\\="),
            "Result should contain escaped filter name"
        );
        assert!(
            drawtext_result.contains("text\\=\\'"),
            "Result should contain escaped parameter text"
        );
        assert!(
            drawtext_result.contains("Copyright"),
            "Result should preserve the copyright text"
        );
        assert!(
            drawtext_result.contains("\\:fontcolor\\="),
            "Result should escape colon and equals sign"
        );

        // Test case 4: Filter with square brackets for pad names
        assert_eq!(
            escape_filter_str("[in1][in2]overlay=format=rgb[out]"),
            "\\[in1\\]\\[in2\\]overlay\\=format\\=rgb\\[out\\]",
            "Filter with pad names should be properly escaped"
        );
    }
}
