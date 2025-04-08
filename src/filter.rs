use crate::{MediaType, PixelFormat, SampleFormat};

use rsmpeg::avfilter::{AVFilter, AVFilterContextMut, AVFilterGraph, AVFilterInOut};
use rsmpeg::avutil::{AVChannelLayout, AVFrame};
use rsmpeg::ffi;

use anyhow::{Context, Result};
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

/// 过滤器工厂 - 用于创建具体的过滤器描述
pub struct FilterFactory;

impl FilterFactory {
    /// 创建缩放过滤器
    pub fn new_scale_filter(width: i32, height: i32, pix_fmt: PixelFormat) -> Filter {
        Filter::new(
            "scale",
            MediaType::VIDEO,
            format!(
                "scale=w={}:h={},format={}",
                width,
                height,
                pix_fmt.get_pix_fmt_name()
            ),
        )
    }

    /// 创建裁剪过滤器
    pub fn new_crop_filter(x: i32, y: i32, width: i32, height: i32) -> Filter {
        Filter::new(
            "crop",
            MediaType::VIDEO,
            format!("crop=x={}:y={}:w={}:h={}", x, y, width, height),
        )
    }

    /// 绘制文字叠加过滤器
    pub fn new_drawtext_filter(text: &str, x: i32, y: i32, size: i32, color: &str) -> Filter {
        Filter::new(
            "drawtext",
            MediaType::VIDEO,
            format!(
                "drawtext=text='{}':x={}:y={}:fontsize={}:fontcolor={}",
                text, x, y, size, color
            ),
        )
    }

    /// 画矩形框
    pub fn new_drawbox_filter(
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        color: &str,
        thickness: i32,
    ) -> Filter {
        Filter::new(
            "drawbox",
            MediaType::VIDEO,
            format!(
                "drawbox=x={}:y={}:w={}:h={}:color={}:t={}",
                x, y, w, h, color, thickness
            ),
        )
    }

    /// 创建锐化过滤器
    pub fn new_denoise_filter(strength: f32) -> Filter {
        Filter::new(
            "denoise",
            MediaType::VIDEO,
            format!("hqdn3d={}:{}:{}:{}", strength, strength, strength, strength),
        )
    }

    /// 创建视频淡入淡出过滤器
    pub fn new_fade_filter(fade_in_frames: i32, fade_out_frames: i32, total_frames: i32) -> Filter {
        Filter::new(
            "fade",
            MediaType::VIDEO,
            format!(
                "fade=t=in:st=0:d={},fade=t=out,st={},d={}",
                fade_in_frames,
                total_frames - fade_out_frames,
                fade_out_frames
            ),
        )
    }

    /// <https://ffmpeg.org/ffmpeg-filters.html#transpose-1>
    /// transpose - 用于快速 90°/180°/270° 视频画面旋转、水平翻转或镜像翻转（无插值，高性能）
    /// mode: 0=逆时针90度/垂直翻转, 1=顺时针90度, 2=逆时针90度, 3=顺时针90度/垂直翻转
    pub fn new_transpose_filter(mode: i32) -> Filter {
        Filter::new("transpose", MediaType::VIDEO, format!("transpose={}", mode))
    }

    /// rotate - 任意角度旋转滤镜（使用浮点弧度，支持动画）
    /// 注意：性能较低，可能有插值模糊；用于精准旋转或动态旋转场景
    pub fn new_rotate_filter(angle: i32) -> Filter {
        // Ffmpeg 中的角度使用弧度而非度数，因此需要转换
        Filter::new(
            "rotate",
            MediaType::VIDEO,
            format!("rotate={}*PI/180", angle),
        )
    }

    /// 修改时间戳表达式（加速、减速、对齐等）
    /// 典型值：setpts=0.5*PTS（2倍速），1.5*PTS（慢放）
    pub fn new_setpts_filter(expr: &str) -> Filter {
        Filter::new("setpts", MediaType::VIDEO, format!("setpts={}", expr))
    }

    ////////////////////////////////////////////////////////////////////////////////////
    ////////////////////////////////////////////////////////////////////////////////////

    /// 创建音频重采样过滤器
    pub fn new_resample_filter(sample_rate: i32) -> Filter {
        Filter::new(
            "resample",
            MediaType::AUDIO,
            format!("aresample=osr={}:async=0", sample_rate),
        )
    }

    /// 创建音频压缩器过滤器
    pub fn new_compressor_filter(threshold: f32, ratio: f32) -> Filter {
        Filter::new(
            "compressor",
            MediaType::AUDIO,
            format!(
                "acompressor=threshold={}:ratio={}:attack=200:release=1000",
                threshold, ratio
            ),
        )
    }

    /// 创建音频均衡器过滤器
    pub fn new_loudnorm_filter(target_level: f32) -> Filter {
        Filter::new(
            "loudnorm",
            MediaType::AUDIO,
            format!("loudnorm=I={}:TP=-1.5:LRA=11", target_level),
        )
    }

    /// 创建音量调整过滤器
    pub fn new_volume_filter(volume: f32) -> Filter {
        Filter::new(
            "volume",
            MediaType::AUDIO,
            format!("volume=volume={}", volume),
        )
    }

    /// 创建单频段均衡器过滤器
    pub fn new_single_equalizer_filter(frequency: i32, gain: f32, width: i32) -> Filter {
        Filter::new(
            "equalizer",
            MediaType::AUDIO,
            format!(
                "equalizer=f={}:width_type=h:width={}:g={}",
                frequency, width, gain
            ),
        )
    }

    /// 创建多频段均衡器过滤器 (bass, mid, treble)
    pub fn new_three_band_equalizer_filter(
        bass_gain: f32,
        mid_gain: f32,
        treble_gain: f32,
    ) -> Filter {
        Filter::new(
            "firequalizer",
            MediaType::AUDIO,
            format!(
                "firequalizer=gain='if(lt(f,200),{},if(gt(f,5000),{},{}))':scale=log",
                bass_gain, treble_gain, mid_gain
            ),
        )
    }

    /// 创建自定义频率响应均衡器过滤器
    pub fn new_custom_equalizer_filter(gain_expression: &str) -> Filter {
        Filter::new(
            "firequalizer",
            MediaType::AUDIO,
            format!("firequalizer=gain='{}':scale=log", gain_expression),
        )
    }

    /// 创建FFT降噪过滤器
    pub fn new_fft_denoise_filter(noise_reduction: i32, noise_floor: i32) -> Filter {
        Filter::new(
            "afftdn",
            MediaType::AUDIO,
            format!("afftdn=nr={}:nf={}:nt=w", noise_reduction, noise_floor),
        )
    }

    /// 创建高级FFT降噪过滤器
    pub fn new_advanced_fft_denoise_filter(
        noise_reduction: i32,
        noise_floor: i32,
        noise_type: &str,
        time_smoothing: f32,
    ) -> Filter {
        Filter::new(
            "afftdn",
            MediaType::AUDIO,
            format!(
                "afftdn=nr={}:nf={}:nt={}:tr={}",
                noise_reduction, noise_floor, noise_type, time_smoothing
            ),
        )
    }

    /// 创建自适应非局部均值降噪过滤器
    pub fn new_anlm_denoise_filter(strength: i32, patch_size: i32, search_range: i32) -> Filter {
        Filter::new(
            "anlmdn",
            MediaType::AUDIO,
            format!("anlmdn=s={}:p={}:r={}", strength, patch_size, search_range),
        )
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

/// 过滤器图表 - 包含所有过滤器配置
pub struct FilterGraph {
    graph: AVFilterGraph,
    initialized: AtomicBool,
}

impl FilterGraph {
    pub fn new() -> Self {
        Self {
            graph: AVFilterGraph::new(),
            initialized: AtomicBool::new(false),
        }
    }

    /// 初始化过滤器图表
    pub fn init(&mut self, params: &FilterParams, filters: &[Filter]) -> Result<()> {
        if self.initialized.load(DEFAULT_ORDERING) {
            return Err(anyhow::anyhow!("Filter graph already initialized"));
        }

        // 验证过滤器类型匹配
        for filter in filters {
            if filter.media_type() != params.media_type() {
                return Err(anyhow::anyhow!(
                    "Filter media type mismatch: expected {:?}, got {:?}",
                    params.media_type(),
                    filter.media_type()
                ));
            }
        }

        // 构建过滤器链
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

    // Setup video filters
    fn setup_video_filters(&mut self, params: &VideoParams, spec: String) -> Result<()> {
        let args = {
            let args = format!(
                "video_size={}x{}:pix_fmt={}:time_base={}/{}:pixel_aspect={}/{}:frame_rate={}/{}",
                params.width,
                params.height,
                ffi::AVPixelFormat::from(params.format),
                params.time_base.num,
                params.time_base.den,
                params.pixel_aspect.num,
                params.pixel_aspect.den,
                params.frame_rate.num,
                params.frame_rate.den,
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

    // Setup audio filters
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
        sink_ctx.opt_set_bin(c"channel_layouts", &channel_desc)?;
        sink_ctx.opt_set_bin(c"sample_rates", &params.sample_rate)?;

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
        if !self.initialized.load(DEFAULT_ORDERING) {
            return Err(anyhow::anyhow!("Filter graph not initialized"));
        }

        {
            // Get source context and send the frame
            let mut src_ctx = self.get_src_context()?;
            src_ctx.buffersrc_add_frame(frame, None)?;
        } // src_ctx is dropped here, releasing the mutable borrow

        // safely get a new mutable borrow for sink_ctx
        let mut sink_ctx = self.get_sink_context()?;

        // 获取处理后的帧
        match sink_ctx.buffersink_get_frame(None) {
            Ok(frame) => Ok(Some(frame)),
            Err(rsmpeg::error::RsmpegError::BufferSinkDrainError)
            | Err(rsmpeg::error::RsmpegError::BufferSinkEofError) => Ok(None),
            Err(e) => Err(anyhow::anyhow!("Failed to get frame from filter: {}", e)),
        }
    }

    /// 刷新过滤器链
    pub fn flush(&mut self) -> Result<Vec<AVFrame>> {
        if !self.initialized.load(DEFAULT_ORDERING) {
            return Err(anyhow::anyhow!("Filter graph not initialized"));
        }

        let mut frames = Vec::new();

        loop {
            match self.process_frame(None) {
                Ok(Some(frame)) => frames.push(frame),
                Ok(None) => break,
                Err(e) => return Err(e),
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
            "FilterGraph nb_filters:{}, initialized:{}",
            self.graph.nb_filters,
            self.initialized.load(DEFAULT_ORDERING)
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
    config: FilterConfig,
    graph: FilterGraph,
}

impl FilterContext {
    /// 为指定流添加过滤器
    pub fn new(config: FilterConfig) -> Result<FilterContext> {
        log::debug!("new filter context:{:?}", config);

        // 创建并初始化过滤器图表
        let mut graph = FilterGraph::new();
        graph.init(&config.params, &config.filters)?;

        Ok(FilterContext { config, graph })
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
            .field("config", &self.config)
            .field("graph", &self.graph)
            .finish()
    }
}
