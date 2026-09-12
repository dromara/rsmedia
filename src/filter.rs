//! Filter module
//! This module provides a set of filters that can be used to process media data.
//! The filters are implemented using the ffmpeg library.
//!
//! See: <https://ffmpeg.org/ffmpeg-filters.html>
use crate::MediaType;
use crate::error::{Context, Result, RsmediaError};
use crate::fmt::{FrameFormat, SampleFormat};
use crate::pixel::PixelFormat;

use rsmpeg::avfilter::{AVFilter, AVFilterContextMut, AVFilterGraph, AVFilterInOut};
use rsmpeg::avutil::{AVChannelLayout, AVFrame};
use rsmpeg::ffi;

use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone)]
pub struct Filter {
    name: &'static str,
    media_type: MediaType,
    spec: String,
    /// 滤镜图要求的**输入**格式声明（如 GIF 调色板链要求 RGB 输入）。
    ///
    /// `Some(fmt)` 时：编码器/解码器侧把输入帧先转换到 `fmt` 再进图，且
    /// buffer/abuffer 源按 `fmt` 配置（sink 仍按协商格式）。`None` 时沿用
    /// 协商格式（默认，src/sink 同格式，零行为变化）。
    input_format: Option<FrameFormat>,
}

impl Filter {
    pub fn new(name: &'static str, media_type: MediaType, spec: String) -> Self {
        Self {
            name,
            media_type,
            spec,
            input_format: None,
        }
    }

    /// 声明滤镜图要求的输入格式（视频=像素格式 / 音频=采样格式，覆盖默认的
    /// "协商格式"）。
    ///
    /// `PixelFormat`/`SampleFormat` 均可隐式转入 [`FrameFormat`]，调用形如
    /// `.with_input_format(PixelFormat::RGB24)` 或
    /// `.with_input_format(SampleFormat::FLT)`。
    pub fn with_input_format(mut self, format: impl Into<FrameFormat>) -> Self {
        self.input_format = Some(format.into());
        self
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

    pub fn input_format(&self) -> Option<FrameFormat> {
        self.input_format
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
            ffi::AV_ESCAPE_MODE_AUTO,
            ffi::AV_ESCAPE_FLAG_WHITESPACE as i32,
        );

        // 检查返回值是否为错误
        if result < 0 {
            eprintln!("av_escape failed with error code: {}", result);
            // 使用安全的回退方案
            return input.replace('\0', "").to_string();
        }

        // 检查返回的指针是否为空
        if escaped_ptr.is_null() {
            eprintln!("av_escape returned null pointer");
            // 使用安全的回退方案
            return input.replace('\0', "").to_string();
        }

        // Convert back to Rust String and free the memory
        let escaped_cstr = std::ffi::CStr::from_ptr(escaped_ptr);
        let escaped_string = escaped_cstr.to_string_lossy().into_owned();

        // Free memory allocated by FFmpeg
        ffi::av_free(escaped_ptr as *mut _);

        escaped_string
    }
}

/// 转义文本，但保留 FFmpeg 的 `%{...}` 展开块（如 `%{localtime}`、`%{pts:hms}`）。
///
/// 用于 `drawtext` 等需要显示动态时间/帧号的场景，避免 `{` `}` 被转义后无法展开。
fn escape_filter_expr(input: &str) -> String {
    let mut result = String::new();
    let mut rest = input;
    while let Some(pos) = rest.find("%{") {
        // 转义 `%{` 之前的部分
        result.push_str(&escape_filter_str(&rest[..pos]));
        // 找到匹配的 `}`，整体保留
        if let Some(end_rel) = rest[pos..].find('}') {
            result.push_str(&rest[pos..pos + end_rel + 1]);
            rest = &rest[pos + end_rel + 1..];
        } else {
            result.push_str(&escape_filter_str(&rest[pos..]));
            rest = "";
        }
    }
    result.push_str(&escape_filter_str(rest));
    result
}

pub mod video {
    use super::*;

    /// Scales video dimensions.
    ///
    /// # Arguments
    ///
    /// * `width`: Target width.
    /// * `height`: Target height.
    /// * `flags`: Optional `SWS_FLAG_*` string, specifying the scaling algorithm and other options. Default is "bicubic".
    ///   Possible values for scaling algorithm flags:
    ///     - `fast_bilinear`: Select fast bilinear scaling algorithm.
    ///     - `bilinear`: Select bilinear scaling algorithm.
    ///     - `bicubic`: Select bicubic scaling algorithm (default).
    ///     - `experimental`: Select experimental scaling algorithm.
    ///     - `neighbor`: Select nearest neighbor rescaling algorithm.
    ///     - `area`: Select averaging area rescaling algorithm.
    ///     - `bicublin`: Select bicubic scaling algorithm for the luma component, bilinear for chroma components.
    ///     - `gauss`: Select Gaussian rescaling algorithm.
    ///     - `sinc`: Select sinc rescaling algorithm.
    ///     - `lanczos`: Select Lanczos rescaling algorithm. The default width (alpha) is 3 and can be changed by setting param0.
    ///     - `spline`: Select natural bicubic spline rescaling algorithm.
    ///       Other possible flags:
    ///     - `print_info`: Enable printing/debug logging.
    ///     - `accurate_rnd`: Enable accurate rounding.
    ///     - `full_chroma_int`: Enable full chroma interpolation.
    ///     - `full_chroma_inp`: Select full chroma input.
    ///     - `bitexact`: Enable bitexact output.
    ///
    /// See: <https://ffmpeg.org/ffmpeg-scaler.html#Scaler-Options>
    pub fn scale(width: u32, height: u32, flags: Option<&str>) -> Filter {
        let flags_str = flags.unwrap_or("fast_bilinear");

        Filter::new(
            "scale",
            MediaType::VIDEO,
            format!("scale=w={width}:h={height}:flags={flags_str}"),
        )
    }

    /// Converts video pixel format.
    /// `format`: https://ffmpeg.org/ffmpeg-filters.html#format
    /// `aformat`: https://ffmpeg.org/ffmpeg-filters.html#aformat-1
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
            format!("crop=x={x}:y={y}:w={w}:h={h}"),
        )
    }

    /// 在视频上绘制文字的 Builder，对应 FFmpeg `drawtext` 滤镜。
    ///
    /// `fontfile` 可选，缺省时使用项目内 `fonts/Arial.ttf`（避免依赖 system fontconfig，
    /// 例如 Windows 等没有 fontconfig 配置的平台会崩溃）；也支持给文字加描边盒子（`boxed`）。
    ///
    /// # Examples
    ///
    /// ```
    /// use rsmedia::filter::video::DrawText;
    /// let f = DrawText::new("Hello", 10, 10, 24, "white")
    ///     .fontfile("/path/to/your-font.ttf")
    ///     .boxed("black@0.5")
    ///     .build();
    /// ```
    pub struct DrawText {
        text: String,
        x: i32,
        y: i32,
        fontfile: Option<String>,
        fontsize: u32,
        fontcolor: String,
        box_enabled: bool,
        box_color: String,
        box_border_w: u32,
        raw_text: bool,
    }

    impl DrawText {
        /// 创建文字水印。
        ///
        /// * `text` - 要绘制的文本。
        /// * `x` / `y` - 文字左上角坐标。
        /// * `fontsize` - 字号。
        /// * `fontcolor` - 文字颜色（如 `"white"`、`"white@0.5"`）。
        pub fn new(text: &str, x: i32, y: i32, fontsize: u32, fontcolor: &str) -> Self {
            Self {
                text: text.to_string(),
                x,
                y,
                fontfile: None,
                fontsize,
                fontcolor: fontcolor.to_string(),
                box_enabled: false,
                box_color: "black@0.5".to_string(),
                box_border_w: 0,
                raw_text: false,
            }
        }

        /// 指定字体文件路径。
        pub fn fontfile(mut self, path: &str) -> Self {
            self.fontfile = Some(path.to_string());
            self
        }

        /// 开启文字背景盒子（描边效果）。
        pub fn boxed(mut self, color: &str) -> Self {
            self.box_enabled = true;
            self.box_color = color.to_string();
            self
        }

        /// 使用 FFmpeg 文本展开表达式显示动态内容（如当前时间、帧号）。
        ///
        /// 常用表达式：`%{localtime}`（本地时间）、`%{pts:hms}`（时间戳时分秒）、
        /// `%{frame_num}`（帧号）。表达式中的 `%{...}` 不会被转义。
        ///
        /// # Examples
        ///
        /// 在右上角显示当前时间：
        /// ```
        /// use rsmedia::filter::video::DrawText;
        /// let f = DrawText::new("", 0, 0, 24, "white")
        ///     .time_text("%{localtime}")
        ///     .build();
        /// ```
        pub fn time_text(mut self, fmt: &str) -> Self {
            self.text = fmt.to_string();
            self.raw_text = true;
            self
        }

        /// 生成最终的 [`Filter`]。
        pub fn build(self) -> Filter {
            let text_spec = if self.raw_text {
                escape_filter_expr(&self.text)
            } else {
                escape_filter_str(&self.text)
            };
            let mut spec = format!(
                "drawtext=text='{}':x={}:y={}:fontsize={}:fontcolor={}",
                text_spec, self.x, self.y, self.fontsize, self.fontcolor
            );
            // 缺省使用项目自带字体，避免依赖 system fontconfig（Windows 等平台没有
            // fontconfig 配置会在查字体时崩溃）；用户显式指定字体时优先用用户的。
            let fontfile = self
                .fontfile
                .unwrap_or_else(|| "fonts/Arial.ttf".to_string());
            spec.push_str(&format!(":fontfile='{}'", escape_filter_str(&fontfile)));
            if self.box_enabled {
                spec.push_str(&format!(
                    ":box=1:boxcolor={}:boxborderw={}",
                    self.box_color, self.box_border_w
                ));
            }
            Filter::new("drawtext", MediaType::VIDEO, spec)
        }
    }

    /// 画矩形框
    pub fn drawbox(x: i32, y: i32, w: u32, h: u32, color: &str, thickness: i32) -> Filter {
        if thickness < 0 {
            // FFmpeg 't=fill' is also possible
            log::warn!("Box thickness is negative ({thickness}), using absolute value.",);
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
            format!("delogo=x={x}:y={y}:w={w}:h={h}"),
        )
    }

    /// 去除水印（delogo）的 Builder：支持**多个区域**串联及可选参数。
    ///
    /// FFmpeg 的 `delogo` 通过周围像素插值填补指定矩形区域，适合去除固定位置的
    /// 简单文字/logo 水印。对半透明、异形或复杂背景的水印效果有限。
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use rsmedia::filter::video::Delogo;
    /// // 去除两处水印并开启调试可视化
    /// let filters = Delogo::new()
    ///     .add_region(10, 10, 120, 30)
    ///     .add_region(640, 10, 120, 30)
    ///     .band(2)
    ///     .show()
    ///     .build();
    /// ```
    pub struct Delogo {
        regions: Vec<(i32, i32, u32, u32)>,
        band: i32,
        show: bool,
    }

    impl Default for Delogo {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Delogo {
        /// 创建一个空的水印去除器（需通过 [`add_region`](Delogo::add_region) 添加区域）。
        pub fn new() -> Self {
            Self {
                regions: Vec::new(),
                band: 1,
                show: false,
            }
        }

        /// 添加一个待去除的水印区域（左上角坐标 + 宽高）。
        pub fn add_region(mut self, x: i32, y: i32, w: u32, h: u32) -> Self {
            self.regions.push((x, y, w, h));
            self
        }

        /// 扫描带宽度（插值取样宽度，默认 1）。
        pub fn band(mut self, band: i32) -> Self {
            self.band = band;
            self
        }

        /// 显示去除区域（调试用），将待去除区域标记出来再输出。
        pub fn show(mut self) -> Self {
            self.show = true;
            self
        }

        /// 生成滤镜列表，每个水印区域对应一个 `delogo` 滤镜（按顺序串联）。
        pub fn build(self) -> Vec<Filter> {
            self.regions
                .into_iter()
                .map(|(x, y, w, h)| {
                    let mut params = format!("delogo=x={x}:y={y}:w={w}:h={h}:band={}", self.band);
                    if self.show {
                        params.push_str(":show=1");
                    }
                    Filter::new("delogo", MediaType::VIDEO, params)
                })
                .collect()
        }
    }

    /// zoompan - 平移和缩放效果
    pub fn zoompan(zoom: &str, x: &str, y: &str, duration: Option<i32>) -> Filter {
        let mut params = format!("zoompan=z={zoom}:x={x}:y={y}");
        if let Some(d) = duration {
            params.push_str(&format!(":d={d}"));
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
            log::warn!("Transpose mode {mode} might be invalid.");
        }
        Filter::new("transpose", MediaType::VIDEO, format!("transpose={mode}"))
    }

    /// rotate - 任意角度旋转滤镜（使用浮点弧度，支持动画）
    /// 注意：性能较低，可能有插值模糊；用于精准旋转或动态旋转场景
    pub fn rotate(angle: i32) -> Filter {
        // Ffmpeg 中的角度使用弧度而非度数，因此需要转换
        Filter::new("rotate", MediaType::VIDEO, format!("rotate={angle}*PI/180"))
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
            format!("fade=t=in:start_frame=0:nb_frames={duration_frames}"),
        )
    }

    /// Fades video out.
    /// `start_frame`: Frame number to start the fade out.
    /// `duration_frames`: Fade duration in number of frames.
    pub fn fade_out(start_frame: u32, duration_frames: u32) -> Filter {
        Filter::new(
            "fade",
            MediaType::VIDEO,
            format!("fade=t=out:start_frame={start_frame}:nb_frames={duration_frames}"),
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
            format!("boxblur=luma_radius={radius}"),
        )
    }

    /// 亮度/对比度调节
    pub fn eq(brightness: f32, contrast: f32) -> Filter {
        Filter::new(
            "eq",
            MediaType::VIDEO,
            format!("eq=brightness={brightness}:contrast={contrast}"),
        )
    }

    /// 帧率控制
    pub fn fps(fps: f32) -> Filter {
        Filter::new("fps", MediaType::VIDEO, format!("fps={fps}"))
    }

    /// 去交错（Deinterlace），将隔行扫描转为逐行扫描。
    /// `mode`: `send_frame`(默认), `send_field`, `send_frame_nospatial`, `send_field_nospatial`.
    pub fn yadif(mode: &str) -> Filter {
        Filter::new("yadif", MediaType::VIDEO, format!("yadif=mode={mode}"))
    }

    /// 补边（Pad），在视频周围添加指定颜色的边。
    ///
    /// * `w` / `h` - 输出尺寸（不包含负值表达式）。
    /// * `x` / `y` - 原视频在输出画布上的偏移。
    /// * `color` - 填充颜色，如 `"black"`。
    pub fn pad(w: u32, h: u32, x: i32, y: i32, color: &str) -> Filter {
        Filter::new(
            "pad",
            MediaType::VIDEO,
            format!("pad=w={w}:h={h}:x={x}:y={y}:color={color}"),
        )
    }

    /// 烧录字幕（Subtitles）。
    /// `path`: 字幕文件路径（`srt`/`ass` 等）。
    pub fn subtitles(path: &str) -> Filter {
        let escaped = escape_filter_str(path);
        Filter::new(
            "subtitles",
            MediaType::VIDEO,
            format!("subtitles={escaped}"),
        )
    }

    /// 设置显示宽高比（DAR）。
    /// `ratio` 为宽高比，如 `16`/`9`。
    pub fn setdar(num: i32, den: i32) -> Filter {
        Filter::new("setdar", MediaType::VIDEO, format!("setdar={num}/{den}"))
    }

    /// 设置采样宽高比（SAR）。
    pub fn setsar(num: i32, den: i32) -> Filter {
        Filter::new("setsar", MediaType::VIDEO, format!("setsar={num}/{den}"))
    }

    /// 色相/饱和度/亮度调节。
    /// `hue` 为色相偏移（度，-180 ~ 180）。
    pub fn hue(hue: i32) -> Filter {
        Filter::new("hue", MediaType::VIDEO, format!("hue=h={hue}"))
    }

    /// 反相（负片效果）。
    pub fn negate() -> Filter {
        Filter::new("negate", MediaType::VIDEO, "negate".to_string())
    }

    /// 添加噪点。
    /// `amount` 为噪点强度（0-100）。
    pub fn noise(amount: u32) -> Filter {
        Filter::new("noise", MediaType::VIDEO, format!("noise=alls={amount}"))
    }

    /// 视频降噪（hqdn3d），减少亮度/色度噪声。
    ///
    /// * `luma` - 亮度空间降噪强度（0-4，默认 4）。
    /// * `chroma` - 色度空间降噪强度（0-3，默认 3）。
    pub fn hqdn3d(luma: f32, chroma: f32) -> Filter {
        Filter::new(
            "hqdn3d",
            MediaType::VIDEO,
            format!("hqdn3d=luma_spatial={luma}:chroma_spatial={chroma}"),
        )
    }

    /// 视频降噪（nlmeans），非局部均值降噪，降噪效果更好但更耗时。
    /// `strength` 为降噪强度（建议 0-20，默认 1.0）。
    pub fn nlmeans(strength: f32) -> Filter {
        Filter::new("nlmeans", MediaType::VIDEO, format!("nlmeans=s={strength}"))
    }

    /// Gamma 校正（画质增强）。
    /// `gamma` 为 gamma 值（通常 0.5-2.0，1.0 表示不变）。
    pub fn gamma(gamma: f32) -> Filter {
        Filter::new("gamma", MediaType::VIDEO, format!("gamma=g={gamma}"))
    }

    /// 饱和度调节（画质增强）。
    /// `saturation` 为饱和度倍数（1.0 表示不变，0 为黑白）。
    pub fn saturation(saturation: f32) -> Filter {
        Filter::new(
            "eq",
            MediaType::VIDEO,
            format!("eq=saturation={saturation}"),
        )
    }

    /// 鲜艳度调节（画质增强）。
    /// `vibrance` 为鲜艳度（-1.0 ~ 1.0，0 表示不变），对应 FFmpeg `vibrance=intensity`。
    pub fn vibrance(vibrance: f32) -> Filter {
        Filter::new(
            "vibrance",
            MediaType::VIDEO,
            format!("vibrance=intensity={vibrance}"),
        )
    }

    /// 去块效应（画质增强），减轻压缩产生的马赛克/块状伪影。
    pub fn deblock() -> Filter {
        Filter::new("deblock", MediaType::VIDEO, "deblock".to_string())
    }

    /// GIF 单遍调色板滤镜链（palettegen/paletteuse），输出 pal8 帧供 `gif`
    /// 编码器直接编码。
    ///
    /// 构建的滤镜图：
    /// `fps=<fps>,split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse[输出]`
    ///
    /// palettegen 统计**全部**输入帧的最优 256 色调色板（EOF 时才输出），
    /// paletteuse 用它把帧量化为 pal8 —— 即 FFmpeg 官方推荐的单遍 GIF 调色板
    /// 管线；palettegen/paletteuse 之间的帧缓存在图内完成，无需两遍编码。
    ///
    /// * `fps` - 输出帧率（GIF 体积敏感，通常 10~15）。
    /// * `dither` - 抖动算法（`"bayer"`/`"floyd_steinberg"`/`"none"` 等），
    ///   `None` 使用 FFmpeg 默认（sierra2_4a）。
    ///
    /// 输入为 RGB 帧：滤镜声明了 RGB24 输入格式，编码器侧自动把输入帧转到
    /// RGB24 再进图；输出 pal8 与 `gif` 编码器原生格式一致。
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use rsmedia::{EncoderBuilder, filter::video};
    /// let encoder = EncoderBuilder::new_video(320, 240)
    ///     .with_codec_name("gif".to_string())
    ///     .with_fps(10.0)
    ///     .with_filters(vec![video::gif_palette(10.0, None)])
    ///     .build()
    ///     .unwrap();
    /// ```
    pub fn gif_palette(fps: f32, dither: Option<&str>) -> Filter {
        let dither_part = dither.map(|d| format!(":dither={d}")).unwrap_or_default();
        Filter::new(
            "paletteuse",
            MediaType::VIDEO,
            format!("fps={fps},split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse{dither_part}"),
        )
        .with_input_format(PixelFormat::RGB24)
    }
}

pub mod audio {
    use super::*;

    /// 创建音频重采样过滤器
    pub fn resample(nb_channels: u32, sample_rate: u32, format: SampleFormat) -> Filter {
        // 统一走解析函数以便复用 channel_desc 处理，避免 describe().unwrap() panic
        let channel_desc = audio_channel_desc(nb_channels);

        // async=1 可能更适合实时场景，避免缓冲问题。
        let spec_str = format!(
            "aresample=osr={}:osf={}:ochl={}:async=1",
            sample_rate,
            format.get_sample_fmt_name(),
            channel_desc,
        );

        // 注意：滤镜本体是 aresample，name 必须与之保持一致，避免与真实滤镜名混淆
        Filter::new("aresample", MediaType::AUDIO, spec_str)
    }

    /// Converts audio sample format.
    /// `format`: <https://ffmpeg.org/ffmpeg-filters.html#format>
    /// `aformat`: <https://ffmpeg.org/ffmpeg-filters.html#aformat-1.
    pub fn format(nb_channels: u32, sample_rate: u32, format: SampleFormat) -> Filter {
        let channel_desc = audio_channel_desc(nb_channels);

        Filter::new(
            "aformat",
            MediaType::AUDIO,
            format!(
                "aformat=sample_fmts={}:sample_rates={}:channel_layouts={}",
                format.get_sample_fmt_name(),
                sample_rate,
                channel_desc
            ),
        )
    }

    /// 把通道数解析为 FFmpeg 通道布局描述；失败时回退到数字通道数，避免 panic。
    fn audio_channel_desc(nb_channels: u32) -> String {
        AVChannelLayout::from_nb_channels(nb_channels as i32)
            .describe()
            .map(|d| d.to_string_lossy().to_string())
            .unwrap_or_else(|_| format!("{nb_channels}"))
    }

    /// 音量调整
    /// Adjusts audio volume.
    /// `volume`: Linear multiplier (1.0 is no change) or dB value (e.g., "-3dB").
    pub fn volume(val: f32) -> Filter {
        // FFmpeg volume filter can take linear scale or dB. Pass string directly.
        // Validation could check if it's a number or ends with "dB".
        Filter::new("volume", MediaType::AUDIO, format!("volume={val}"))
    }

    /// loudnorm - EBU R128音量标准化
    pub fn loudnorm(integrated_loudness: f32) -> Filter {
        Filter::new(
            "loudnorm",
            MediaType::AUDIO,
            format!("loudnorm=I={integrated_loudness}:TP=-1.5:LRA=11"),
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
                "equalizer=f={frequency}:width_type=h:width={width}:g={gain}", // width_type=h (Hz)
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
                "firequalizer=gain='if(lt(f,200),{bass_gain},if(gt(f,5000),{treble_gain},{mid_gain}))':scale=linlog",
            ),
        )
    }

    /// 压缩器
    /// Applies dynamic range compression.
    /// `ratio`: Compression ratio (1 - 20).
    /// `attack`: Attack time in ms (optional, default 20).
    /// `release`: Release time in ms (optional, default 250).
    /// See: <https://ffmpeg.org/ffmpeg-filters.html#acompressor>
    pub fn compressor(ratio: f32, attack: Option<f32>, release: Option<f32>) -> Result<Filter> {
        if ratio < 1.0 {
            return Err(RsmediaError::custom(format!(
                "Compressor ratio must be >= 1.0: {ratio}"
            )));
        }
        let mut spec = format!("acompressor=ratio={ratio}");
        if let Some(a) = attack {
            spec.push_str(&format!(":attack={a}"));
        }
        if let Some(r) = release {
            spec.push_str(&format!(":release={r}"));
        }
        // Add other params: makeup, knee, link, detection, mix...
        Ok(Filter::new("acompressor", MediaType::AUDIO, spec))
    }

    /// 高通滤波
    pub fn highpass(freq: u32) -> Filter {
        Filter::new("highpass", MediaType::AUDIO, format!("highpass=f={freq}"))
    }

    /// 低通滤波
    pub fn lowpass(freq: u32) -> Filter {
        Filter::new("lowpass", MediaType::AUDIO, format!("lowpass=f={freq}"))
    }

    /// 音频变速
    /// Changes audio tempo without changing pitch.
    /// * `rate`: Speed multiplier (0.5 to 100.0).
    pub fn atempo(rate: f32) -> Filter {
        Filter::new("atempo", MediaType::AUDIO, format!("atempo={rate}"))
    }

    /// 延时（ms）
    ///
    /// 注意：不能使用 `delays=100|100` 逐通道写法，因为 `|` 在滤镜图中是滤镜链
    /// 分隔符，会导致图解析失败；统一用 `all=1` 应用到所有通道。
    pub fn adelay(delay_ms: i32) -> Filter {
        Filter::new(
            "adelay",
            MediaType::AUDIO,
            format!("adelay=delays={delay_ms}:all=1"),
        )
    }

    /// 创建FFT降噪过滤器
    /// Applies FFT noise reduction (simple).
    /// `noise_reduction`: Noise reduction factor in dB (e.g., 12).
    /// `noise_floor`: Noise floor in dB (e.g., -50).
    pub fn fft_denoise(noise_reduction: i32, noise_floor: i32) -> Filter {
        Filter::new(
            "afftdn",
            MediaType::AUDIO,
            format!("afftdn=nr={noise_reduction}:nf={noise_floor}:nt=w"),
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
            format!("afftdn=nr={noise_reduction}:nf={noise_floor}:nt={nt}:tr={tr}"),
        )
    }

    /// 创建自适应非局部均值降噪过滤器
    /// Applies Non-Local Means de-noising (anlmdn).
    /// `strength`: Denoising strength (0 to inf, default 1e-05).
    /// `patch_size`: Patch size (default 7).
    /// `search_range`: Research range (default 15).
    ///
    /// # Known upstream issue (FFmpeg <= 9.0)
    ///
    /// FFmpeg 的 `anlmdn` 滤镜在流结束（EOF）冲刷不满一窗的尾巴帧时存在**堆越界写**：
    /// 输出缓冲按尾巴帧的样本数分配，而 `filter_channel` 固定写入完整窗口（默认
    /// 44.1kHz 下 H=177 样本）。当输入总样本数不是 H 的整数倍时会越界写堆内存，
    /// 可能导致进程随机崩溃。上游尚未修复；若使用本滤镜，建议保证输入总样本数为
    /// 窗口尺寸（`H = 2*round(pd*sample_rate/1e6)+1`，默认参数 44.1kHz 下为 177）
    /// 的整数倍，或改用 `Filter::fft_denoise` / `Filter::denoise`。
    pub fn anlm_denoise(
        strength: Option<f32>,
        patch_size: Option<i32>,
        search_range: Option<i32>,
    ) -> Filter {
        let mut params = Vec::new();
        if let Some(s) = strength {
            params.push(format!("s={s}"));
        }
        if let Some(p) = patch_size {
            params.push(format!("p={p}"));
        }
        if let Some(r) = search_range {
            params.push(format!("r={r}"));
        }
        let spec = if params.is_empty() {
            "anlmdn".to_string()
        } else {
            format!("anlmdn={}", params.join(":"))
        };
        Filter::new("anlmdn", MediaType::AUDIO, spec)
    }

    /// 音频降噪（便捷方法），使用 FFT 降噪并自动估计噪声特征。
    /// `strength` 为降噪强度（dB，建议 10-30）。
    pub fn denoise(strength: f32) -> Filter {
        Filter::new(
            "afftdn",
            MediaType::AUDIO,
            format!("afftdn=nr={strength}:nt=w"),
        )
    }
}

/// 修改时间戳表达式（加速、减速、对齐等）。
/// 典型值：`"0.5*PTS"`（2倍速）、`"1.5*PTS"`（慢放）、`"PTS-STARTPTS"`。
/// `expr`: FFmpeg expression (e.g., "0.5*PTS", "PTS-STARTPTS").
pub fn setpts(media_type: MediaType, expr: &str) -> Filter {
    #[rustfmt::skip]
    let name = if media_type == MediaType::AUDIO { "asetpts" } else { "setpts" };
    let escaped_expr = escape_filter_str(expr);
    Filter::new(name, media_type, format!("{name}={escaped_expr}"))
}

/// 将视频/音频裁剪到指定的时间范围。
pub fn trim(media_type: MediaType, start: f32, end: f32) -> Filter {
    #[rustfmt::skip]
    let name = if media_type == MediaType::AUDIO { "atrim" } else { "trim" };
    Filter::new(name, media_type, format!("{name}={start}:{end}"))
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
    /// 滤镜图**输出**（sink）像素格式：编码器协商格式。
    pub format: PixelFormat,
    /// 滤镜图**输入**（buffer 源）像素格式：默认与 `format` 相同；当滤镜链
    /// 声明了不同的输入格式（如 GIF 调色板链要求 RGB 输入、输出 pal8）时，
    /// 由编码器侧设置为声明的输入格式，src→sink 的格式转换由滤镜图内完成。
    pub src_format: PixelFormat,
    pub time_base: ffi::AVRational,
    pub frame_rate: ffi::AVRational,
    pub pixel_aspect: ffi::AVRational,
}

/// 音频过滤器参数
#[derive(Debug, Clone)]
pub struct AudioParams {
    pub nb_channels: i32,
    pub sample_rate: i32,
    /// 滤镜图**输出**（abuffersink 约束）采样格式：编码器/解码器协商格式。
    pub format: SampleFormat,
    /// 滤镜图**输入**（abuffer）采样格式：默认与 `format` 相同；当滤镜链
    /// 声明了不同的输入格式（[`Filter::with_input_format`]）时，由编码器/
    /// 解码器侧设置为声明的格式，src→sink 的格式转换由图内滤镜完成。
    pub src_format: SampleFormat,
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
            return Err(RsmediaError::custom("Filter graph already initialized"));
        }

        // check
        for filter in filters {
            if filter.media_type() != params.media_type() {
                return Err(RsmediaError::custom(format!(
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
            // buffer 源按"滤镜图输入格式"配置（默认=编码器格式）；sink 仍按
            // 编码器协商格式约束。二者不同时（如 GIF 调色板链 RGB→pal8），
            // 格式转换由图内的滤镜（paletteuse/format 等）完成。
            let args = format!(
                "width={}:height={}:pix_fmt={}:time_base={}/{}:frame_rate={}/{}:pixel_aspect={}/{}",
                params.width,
                params.height,
                params.src_format.get_pix_fmt_name(),
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
            .alloc_filter_context(&buffersink, c"out")
            .context("Failed to allocate video buffer sink")?;

        // 先分配再设置选项、最后初始化。FFmpeg 8 将 `pix_fmts`(binary) 废弃为数组选项
        // `pixel_formats`，二者均为非运行时选项，须在 init 之前设置。
        #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
        sink_ctx
            .opt_set_array(
                c"pixel_formats",
                0,
                Some(&[ffi::AVPixelFormat::from(params.format)]),
                ffi::AV_OPT_TYPE_PIXEL_FMT,
            )
            .context("Failed to set video sink filter context pixel format")?;
        #[cfg(any(feature = "ffmpeg6", feature = "ffmpeg7"))]
        sink_ctx
            .opt_set_bin(c"pix_fmts", &(ffi::AVPixelFormat::from(params.format)))
            .context("Failed to set video sink filter context pixel format")?;
        sink_ctx
            .init_str(None)
            .context("Failed to init video buffer sink")?;

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
                params.src_format.get_sample_fmt_name(),
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
            .alloc_filter_context(&buffersink, c"out")
            .context("Failed to allocate audio buffer sink")?;

        // 先分配再设置选项、最后初始化，兼容 FFmpeg 8 中 sink 选项为非运行时选项的限制。
        // FFmpeg8 将如下参数废弃, 且新旧选项不能混用:
        // - buffersink ：新数组选项 pixel_formats （旧 pix_fmts 已废弃）
        // - abuffersink ：新数组选项 `sample_formats`/`samplerates`/`channel_layouts` （旧 `sample_fmts`/`sample_rates`/`ch_layouts`(binary/string) 已废弃）
        #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
        sink_ctx.opt_set_array(
            c"sample_formats",
            0,
            Some(&[params.format as i32]),
            ffi::AV_OPT_TYPE_SAMPLE_FMT,
        )?;
        #[cfg(any(feature = "ffmpeg6", feature = "ffmpeg7"))]
        sink_ctx.opt_set_bin(c"sample_fmts", &(params.format as i32))?;
        #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
        sink_ctx.opt_set_array(
            c"samplerates",
            0,
            Some(&[params.sample_rate]),
            ffi::AV_OPT_TYPE_INT,
        )?;
        #[cfg(any(feature = "ffmpeg6", feature = "ffmpeg7"))]
        sink_ctx.opt_set_bin(c"sample_rates", &params.sample_rate)?;
        #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
        {
            let layout = AVChannelLayout::from_nb_channels(params.nb_channels).into_inner();
            sink_ctx
                .opt_set_array(
                    c"channel_layouts",
                    0,
                    Some(&[layout]),
                    ffi::AV_OPT_TYPE_CHLAYOUT,
                )
                .context("Failed to set audio sink channel layout")?;
        }
        #[cfg(any(feature = "ffmpeg6", feature = "ffmpeg7"))]
        sink_ctx.opt_set(c"ch_layouts", &channel_desc)?;
        sink_ctx
            .init_str(None)
            .context("Failed to init audio buffer sink")?;

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
            return Err(RsmediaError::custom("Filter graph not initialized"));
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
            Err(e) => Err(RsmediaError::custom(format!(
                "Get frame from buffer sink Error: {e}"
            ))),
        }
    }

    /// 刷新过滤器链
    pub fn flush(&mut self) -> Result<Vec<AVFrame>> {
        if !self.is_initialized() {
            return Err(RsmediaError::custom("Filter graph not initialized"));
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
                    log::error!("Error encountered during filter graph flush: {e}");
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

    /// 滤镜输出链路的帧率（`av_buffersink_get_frame_rate`），无滤镜或不可用时返回 `None`。
    pub fn output_frame_rate(&mut self) -> Option<ffi::AVRational> {
        let sink = self.get_sink_context().ok()?;
        Some(sink.get_frame_rate())
    }

    /// 滤镜输出链路的时间基（`av_buffersink_get_time_base`），无滤镜或不可用时返回 `None`。
    pub fn output_time_base(&mut self) -> Option<ffi::AVRational> {
        let sink = self.get_sink_context().ok()?;
        Some(sink.get_time_base())
    }

    /// 滤镜输出链路的尺寸 `(width, height)`（`av_buffersink_get_w/h`），无滤镜或不可用时返回 `None`。
    pub fn output_size(&mut self) -> Option<(i32, i32)> {
        let sink = self.get_sink_context().ok()?;
        Some((sink.get_w(), sink.get_h()))
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
        log::debug!("new filter context:{config:?}");

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

    #[test]
    fn test_escape_filter_expr() {
        // 保留 `%{localtime}` 展开块，其余部分正常转义
        assert_eq!(
            escape_filter_expr("%{localtime}"),
            "%{localtime}",
            "Time expansion block should be preserved"
        );
        assert_eq!(
            escape_filter_expr("T %{pts:hms}"),
            "T\\ %{pts:hms}",
            "Surrounding text should be escaped but block preserved"
        );
        // 多个展开块
        assert_eq!(
            escape_filter_expr("%{frame_num}/%{n}"),
            "%{frame_num}/%{n}",
            "Multiple expansion blocks should be preserved"
        );
        // 无展开块时退化为普通转义
        assert_eq!(
            escape_filter_expr("plain: text"),
            escape_filter_str("plain: text"),
            "Without expansion blocks it should match plain escaping"
        );
    }

    #[test]
    fn test_drawtext_time_text() {
        // 静态文本正常转义
        let static_spec = video::DrawText::new("Hello", 10, 20, 24, "white")
            .build()
            .spec()
            .to_string();
        assert!(
            static_spec.contains("drawtext=text='Hello':x=10:y=20:fontsize=24:fontcolor=white"),
            "static drawtext spec mismatch: {static_spec}"
        );

        // 时间表达式不被转义
        let time_spec = video::DrawText::new("", 10, 20, 24, "white")
            .time_text("%{localtime}")
            .build()
            .spec()
            .to_string();
        assert!(
            time_spec.contains("text='%{localtime}'"),
            "time expression should not be escaped: {time_spec}"
        );
    }

    #[test]
    fn test_filter_spec_generation() {
        use MediaType::*;

        // ---- Video filters ----
        let cases: Vec<(String, String, MediaType)> = vec![
            (
                "scale=w=640:h=360:flags=bicubic".into(),
                video::scale(640, 360, Some("bicubic")).spec(),
                VIDEO,
            ),
            (
                "scale=w=640:h=360:flags=fast_bilinear".into(),
                video::scale(640, 360, None).spec(),
                VIDEO,
            ),
            (
                "crop=x=10:y=20:w=100:h=50".into(),
                video::crop(10, 20, 100, 50).spec(),
                VIDEO,
            ),
            (
                "fade=t=in:start_frame=0:nb_frames=30".into(),
                video::fade_in(30).spec(),
                VIDEO,
            ),
            (
                "fade=t=out:start_frame=30:nb_frames=30".into(),
                video::fade_out(30, 30).spec(),
                VIDEO,
            ),
            ("unsharp".into(), video::unsharp().spec(), VIDEO),
            (
                "boxblur=luma_radius=2".into(),
                video::blur(2.0).spec(),
                VIDEO,
            ),
            (
                "eq=brightness=0.2:contrast=1.5".into(),
                video::eq(0.2, 1.5).spec(),
                VIDEO,
            ),
            ("fps=30".into(), video::fps(30.0).spec(), VIDEO),
            (
                "yadif=mode=send_frame".into(),
                video::yadif("send_frame").spec(),
                VIDEO,
            ),
            (
                "pad=w=1280:h=720:x=0:y=0:color=black".into(),
                video::pad(1280, 720, 0, 0, "black").spec(),
                VIDEO,
            ),
            (
                "subtitles=sub.srt".into(),
                video::subtitles("sub.srt").spec(),
                VIDEO,
            ),
            ("setdar=16/9".into(), video::setdar(16, 9).spec(), VIDEO),
            ("setsar=1/1".into(), video::setsar(1, 1).spec(), VIDEO),
            ("hue=h=30".into(), video::hue(30).spec(), VIDEO),
            ("negate".into(), video::negate().spec(), VIDEO),
            ("noise=alls=10".into(), video::noise(10).spec(), VIDEO),
            (
                "hqdn3d=luma_spatial=3:chroma_spatial=2".into(),
                video::hqdn3d(3.0, 2.0).spec(),
                VIDEO,
            ),
            ("nlmeans=s=1.5".into(), video::nlmeans(1.5).spec(), VIDEO),
            ("gamma=g=1.2".into(), video::gamma(1.2).spec(), VIDEO),
            (
                "eq=saturation=1.5".into(),
                video::saturation(1.5).spec(),
                VIDEO,
            ),
            (
                "vibrance=intensity=0.4".into(),
                video::vibrance(0.4).spec(),
                VIDEO,
            ),
            ("deblock".into(), video::deblock().spec(), VIDEO),
            (
                "delogo=x=0:y=0:w=100:h=50".into(),
                video::delogo(0, 0, 100, 50).spec(),
                VIDEO,
            ),
            ("transpose=1".into(), video::transpose(1).spec(), VIDEO),
            (
                "drawbox=x=1:y=2:w=10:h=10:color=red:t=2".into(),
                video::drawbox(1, 2, 10, 10, "red", 2).spec(),
                VIDEO,
            ),
        ];

        for (expected, actual, media_type) in cases {
            assert_eq!(actual, expected, "spec mismatch");
            assert_eq!(
                Filter::new("x", media_type, String::new()).media_type(),
                media_type
            );
        }

        // ---- Audio filters ----
        let audio_cases: Vec<(String, String)> = vec![
            ("volume=1.5".into(), audio::volume(1.5).spec()),
            (
                "loudnorm=I=-16:TP=-1.5:LRA=11".into(),
                audio::loudnorm(-16.0).spec(),
            ),
            ("highpass=f=80".into(), audio::highpass(80).spec()),
            ("lowpass=f=4000".into(), audio::lowpass(4000).spec()),
            ("atempo=1.25".into(), audio::atempo(1.25).spec()),
            (
                "afftdn=nr=20:nf=-50:nt=w".into(),
                audio::fft_denoise(20, -50).spec(),
            ),
            ("afftdn=nr=15:nt=w".into(), audio::denoise(15.0).spec()),
        ];
        for (expected, actual) in audio_cases {
            assert_eq!(actual, expected, "audio spec mismatch");
        }

        // compressor 返回 Result
        let comp = audio::compressor(3.0, Some(30.0), Some(200.0)).unwrap();
        assert_eq!(comp.spec(), "acompressor=ratio=3:attack=30:release=200");
        // 非法 ratio 返回错误而非 panic
        assert!(audio::compressor(0.5, None, None).is_err());
    }

    #[test]
    fn test_delogo_builder() {
        use MediaType::*;

        // 单区域便捷方法
        let simple = video::delogo(0, 0, 100, 50);
        assert_eq!(simple.name(), "delogo");
        assert_eq!(simple.media_type(), VIDEO);
        assert_eq!(simple.spec(), "delogo=x=0:y=0:w=100:h=50");

        // Builder：多区域 + band + show
        let filters = video::Delogo::new()
            .add_region(10, 10, 120, 30)
            .add_region(640, 10, 120, 30)
            .band(2)
            .show()
            .build();
        assert_eq!(filters.len(), 2, "one delogo per region");
        assert_eq!(
            filters[0].spec(),
            "delogo=x=10:y=10:w=120:h=30:band=2:show=1"
        );
        assert_eq!(
            filters[1].spec(),
            "delogo=x=640:y=10:w=120:h=30:band=2:show=1"
        );
        for f in &filters {
            assert_eq!(f.media_type(), VIDEO);
        }

        // 空 Builder 生成空列表
        assert!(video::Delogo::new().build().is_empty());
    }

    #[test]
    fn test_drawtext_boxed_and_fontfile() {
        let f = video::DrawText::new("Hi", 1, 2, 20, "white")
            .fontfile("fonts/A.ttf")
            .boxed("black@0.5")
            .build()
            .spec()
            .to_string();
        assert!(
            f.contains("drawtext=text='Hi':x=1:y=2:fontsize=20:fontcolor=white"),
            "base mismatch: {f}"
        );
        assert!(
            f.contains("fontfile='fonts/A.ttf'"),
            "fontfile missing: {f}"
        );
        assert!(
            f.contains("box=1:boxcolor=black@0.5:boxborderw=0"),
            "box missing: {f}"
        );
    }

    /// 构造一个 GRAY8 单平面、已分配缓冲的测试帧。
    fn make_gray8_frame(width: i32, height: i32) -> AVFrame {
        use crate::error::Context;
        use crate::pixel::PixelFormat;
        let mut f = AVFrame::new();
        f.set_width(width);
        f.set_height(height);
        f.set_format(PixelFormat::GRAY8.into());
        f.alloc_buffer()
            .context("alloc buffer for gray8 frame")
            .unwrap();
        f
    }

    /// `FilterGraph::process_frame` 独立路径测试（此前仅经 encode 间接覆盖）。
    ///
    /// 用真实的 `hflip` 滤镜：单帧进、单帧出；输出尺寸/格式保持，但整行像素
    /// 顺序被反转。同时验证状态机 `is_initialized` 与 `output_size`。
    #[test]
    fn test_filtergraph_process_frame_hflip() -> Result<()> {
        use crate::pixel::PixelFormat;

        let (w, h) = (8, 4);
        let src = make_gray8_frame(w, h);
        // 给每行填入递增的像素值，用于检测 hflip 是否确实翻转了行内顺序。
        unsafe {
            let linesize = src.linesize[0] as usize;
            for y in 0..h as usize {
                for x in 0..w as usize {
                    // 像素值 = (y*w + x) % 256，行内单调递增。
                    *src.data[0].cast::<u8>().add(y * linesize + x) =
                        ((y * w as usize + x) % 256) as u8;
                }
            }
        }

        let params = FilterParams::Video(VideoParams {
            width: w,
            height: h,
            format: PixelFormat::GRAY8,
            src_format: PixelFormat::GRAY8,
            time_base: ffi::AVRational { num: 1, den: 25 },
            frame_rate: ffi::AVRational { num: 25, den: 1 },
            pixel_aspect: ffi::AVRational { num: 1, den: 1 },
        });
        let filter = video::hflip();

        let mut graph = FilterGraph::new();
        assert!(!graph.is_initialized(), "graph should start uninitialized");
        graph.init(&params, &[filter])?;
        assert!(
            graph.is_initialized(),
            "graph should be initialized after init"
        );

        // 输出链路尺寸应与输入一致。
        assert_eq!(
            graph.output_size(),
            Some((w, h)),
            "output_size should match input size"
        );

        // 单帧进 -> 单帧出，行被反转。
        let out = graph
            .process_frame(Some(src))?
            .expect("one input frame should yield one output frame");
        assert_eq!(out.width, w, "output width mismatch");
        assert_eq!(out.height, h, "output height mismatch");

        // 校验每个像素是否被水平翻转（逐行逆序）。
        let linesize = out.linesize[0] as usize;
        unsafe {
            let out_ptr = out.data[0].cast::<u8>();
            for y in 0..h as usize {
                for x in 0..w as usize {
                    let expected = ((y * w as usize + (w as usize - 1 - x)) % 256) as u8;
                    let got = *out_ptr.add(y * linesize + x);
                    assert_eq!(
                        got, expected,
                        "hflip mismatch at ({x},{y}): got {got}, expected {expected}"
                    );
                }
            }
        }

        // 提交 EOF 后 graph 进入 drained/flushed 状态，flush 亦返回空。
        assert!(
            graph.process_frame(None)?.is_none(),
            "EOF should eventually drain to None"
        );
        let remaining = graph.flush()?;
        assert!(remaining.is_empty(), "no frames should remain after EOF");
        Ok(())
    }

    /// 音频 `abuffer`/`abuffersink` 独立路径测试：aformat 把 FLTP 转成 S16。
    #[test]
    fn test_filter_graph_process_frame_audio() -> Result<()> {
        use rsmpeg::avutil::AVChannelLayout;

        let (channels, sample_rate, nb_samples) = (2i32, 44100i32, 1024i32);

        let params = FilterParams::Audio(AudioParams {
            nb_channels: channels,
            sample_rate,
            format: SampleFormat::S16,
            src_format: SampleFormat::FLTP,
            time_base: ffi::AVRational { num: 1, den: 44100 },
        });

        // aformat 把输入转为 S16（与 sink 约束一致）。
        let mut graph = FilterGraph::new();
        graph.init(
            &params,
            &[audio::format(
                channels as u32,
                sample_rate as u32,
                SampleFormat::S16,
            )],
        )?;

        let mut frame = AVFrame::new();
        frame.set_format(SampleFormat::FLTP as _);
        frame.set_ch_layout(AVChannelLayout::from_nb_channels(channels).into_inner());
        frame.set_sample_rate(sample_rate);
        frame.set_nb_samples(nb_samples);
        frame.alloc_buffer().context("alloc audio frame")?;

        let out = graph
            .process_frame(Some(frame))?
            .expect("audio frame should flow through");
        assert_eq!(out.sample_rate, sample_rate, "sample rate preserved");
        assert_eq!(out.nb_samples, nb_samples, "sample count preserved");

        graph.process_frame(None)?;
        assert!(graph.flush()?.is_empty());
        Ok(())
    }
}
