// #![allow(dead_code)]
//
// use crate::{MediaType, PixelFormat, SampleFormat};
// use std::collections::HashMap;
//
// use rsmpeg::avfilter::{AVFilter, AVFilterContextMut, AVFilterGraph, AVFilterInOut};
// use rsmpeg::avutil::{AVChannelLayout, AVFrame};
// use rsmpeg::error::RsmpegError;
// use rsmpeg::ffi;
//
// use anyhow::{Context, Error, Result};
// use std::ffi::CString;
// use std::fmt::Formatter;
// use std::sync::Arc;
//
// #[derive(Debug)]
// pub struct Filter {
//     name: &'static str,
//     spec: String,
//     media_type: MediaType,
// }
//
// impl Filter {
//     pub fn new(name: &'static str, media_type: MediaType, spec: String) -> Self {
//         Self {
//             name,
//             media_type,
//             spec,
//         }
//     }
//
//     pub fn name(&self) -> &'static str {
//         self.name
//     }
//
//     pub fn media_type(&self) -> MediaType {
//         self.media_type
//     }
//
//     pub fn spec(&self) -> String {
//         self.spec.clone()
//     }
// }
//
// /// 过滤器工厂 - 用于创建具体的过滤器描述
// pub struct FilterFactory;
//
// impl FilterFactory {
//     /// 创建缩放过滤器
//     pub fn new_scale_filter(width: i32, height: i32, pix_fmt: PixelFormat) -> Filter {
//         Filter::new(
//             "scale",
//             MediaType::VIDEO,
//             format!(
//                 "scale={}:{},format={}",
//                 width,
//                 height,
//                 pix_fmt.get_pix_fmt_name()
//             ),
//         )
//     }
//
//     /// 创建旋转过滤器
//     pub fn new_rotate_filter(angle: i32) -> Filter {
//         Filter::new("rotate", MediaType::VIDEO, format!("rotate={}", angle))
//     }
//
//     /// 创建音频重采样过滤器
//     pub fn new_resample_filter(sample_rate: i32) -> Filter {
//         Filter::new(
//             "resample",
//             MediaType::AUDIO,
//             format!("aresample={}:async=0", sample_rate),
//         )
//     }
//
//     /// 创建音量调整过滤器
//     pub fn new_volume_filter(volume: f32) -> Filter {
//         Filter::new("volume", MediaType::AUDIO, format!("volume={}", volume))
//     }
// }
//
// /// 过滤器参数配置
// #[derive(Debug, Clone)]
// pub enum FilterParams {
//     Video(VideoParams),
//     Audio(AudioParams),
// }
//
// impl FilterParams {
//     pub fn media_type(&self) -> MediaType {
//         match self {
//             FilterParams::Video(_) => MediaType::VIDEO,
//             FilterParams::Audio(_) => MediaType::AUDIO,
//         }
//     }
// }
//
// /// 视频过滤器参数
// #[derive(Debug, Clone)]
// pub struct VideoParams {
//     pub width: i32,
//     pub height: i32,
//     pub format: PixelFormat,
//     pub time_base: ffi::AVRational,
//     pub frame_rate: ffi::AVRational,
//     pub pixel_aspect: ffi::AVRational,
// }
//
// /// 音频过滤器参数
// #[derive(Debug, Clone)]
// pub struct AudioParams {
//     pub nb_channels: u32,
//     pub sample_rate: i32,
//     pub format: SampleFormat,
//     pub time_base: ffi::AVRational,
// }
//
// /// 流过滤器上下文
// pub struct FilterContext<'graph> {
//     src_ctx: AVFilterContextMut<'graph>,
//     sink_ctx: AVFilterContextMut<'graph>,
// }
//
// impl FilterContext<'_> {
//     fn process_frame(&mut self, frame: Option<AVFrame>) -> Result<Option<AVFrame>> {
//         self.src_ctx.buffersrc_add_frame(frame, None)?;
//
//         match self.sink_ctx.buffersink_get_frame(None) {
//             Ok(frame) => Ok(Some(frame)),
//             Err(RsmpegError::BufferSinkDrainError) | Err(RsmpegError::BufferSinkEofError) => {
//                 Ok(None)
//             }
//             Err(e) => Err(Error::new(e).context("Get frame from buffer sink failed.")),
//         }
//     }
//
//     fn flush(&mut self) -> Result<Vec<AVFrame>> {
//         let mut frames = Vec::new();
//         loop {
//             match self.process_frame(None) {
//                 Ok(Some(frame)) => frames.push(frame),
//                 Ok(None) => break,
//                 Err(e) => return Err(e),
//             }
//         }
//         Ok(frames)
//     }
// }
//
// impl std::fmt::Debug for FilterContext<'_> {
//     fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//         write!(
//             f,
//             "FilterContext buffersrc_ctx:[type:{}, format:{}], buffersink_ctx:[type:{}, format:{}]",
//             self.src_ctx.get_type(),
//             self.src_ctx.get_format(),
//             self.sink_ctx.get_type(),
//             self.sink_ctx.get_format()
//         )
//     }
// }
//
// /// 过滤器图表上下文
// pub struct FilterGraphContext {
//     graph: Arc<AVFilterGraph>, // 使用 Arc 来共享所有权
// }
//
// impl FilterGraphContext {
//     pub fn new() -> Self {
//         Self {
//             graph: Arc::new(AVFilterGraph::new()),
//         }
//     }
// }
//
// impl<'graph> FilterGraphContext {
//     fn create_filter_context(
//         &'graph self,
//         stream_params: &FilterParams,
//         filters: &[Filter],
//     ) -> Result<FilterContext<'graph>> {
//         filters.iter().try_for_each(|f| {
//             if f.media_type() != stream_params.media_type() {
//                 return Err(anyhow::anyhow!(
//                     "Filter media type mismatch: expected {:?}, got {:?}",
//                     stream_params.media_type(),
//                     f.media_type()
//                 ));
//             }
//             Ok(())
//         })?;
//
//         let (mut src_ctx, mut sink_ctx) = match stream_params {
//             FilterParams::Video(p) => Self::create_video_filters(&self.graph, p)?,
//             FilterParams::Audio(p) => Self::create_audio_filters(&self.graph, p)?,
//         };
//
//         // Endpoints for the filter graph
//         //
//         // Yes the outputs' name is `in` -_-b
//         let outputs = AVFilterInOut::new(c"in", &mut src_ctx, 0);
//         let inputs = AVFilterInOut::new(c"out", &mut sink_ctx, 0);
//
//         let filter_spec = filters
//             .iter()
//             .map(|f| f.spec())
//             .collect::<Vec<_>>()
//             .join(",");
//
//         let spec_cstr = CString::new(filter_spec)?;
//
//         let (_in, _out) = self
//             .graph
//             .parse_ptr(&spec_cstr, Some(inputs), Some(outputs))?;
//         self.graph.config()?;
//
//         Ok(FilterContext { src_ctx, sink_ctx })
//     }
//
//     fn create_video_filters(
//         graph: &'graph AVFilterGraph,
//         params: &VideoParams,
//     ) -> Result<(AVFilterContextMut<'graph>, AVFilterContextMut<'graph>)> {
//         let args = {
//             let args = format!(
//                 "video_size={}x{}:pix_fmt={}:time_base={}/{}:pixel_aspect={}/{}:frame_rate={}/{}",
//                 params.width,
//                 params.height,
//                 params.format as i32,
//                 params.time_base.num,
//                 params.time_base.den,
//                 params.pixel_aspect.num,
//                 params.pixel_aspect.den,
//                 params.frame_rate.num,
//                 params.frame_rate.den,
//             );
//             CString::new(args)?
//         };
//
//         let buffersrc = AVFilter::get_by_name(c"video_buffer_src")
//             .ok_or_else(|| anyhow::anyhow!("Failed to get video buffer source filter"))?;
//         let buffersink = AVFilter::get_by_name(c"video_buffer_sink")
//             .ok_or_else(|| anyhow::anyhow!("Failed to get video buffer sink filter"))?;
//
//         let src_ctx = graph
//             .create_filter_context(&buffersrc, c"in", Some(&args))
//             .context("Failed to create video buffer source")?;
//
//         let mut sink_ctx = graph
//             .create_filter_context(&buffersink, c"out", None)
//             .context("Failed to create video buffer sink")?;
//
//         sink_ctx
//             .opt_set_bin(c"pix_fmts", &(params.format as i32))
//             .context("Failed to set video sink filter context pixel format")?;
//
//         Ok((src_ctx, sink_ctx))
//     }
//
//     fn create_audio_filters(
//         graph: &'graph AVFilterGraph,
//         params: &AudioParams,
//     ) -> Result<(AVFilterContextMut<'graph>, AVFilterContextMut<'graph>)> {
//         let channel_desc =
//             AVChannelLayout::from_nb_channels(params.nb_channels as i32).describe()?;
//
//         let args = {
//             let args = format!(
//                 "time_base={}/{}:sample_rate={}:sample_fmt={}:channel_layout={}",
//                 params.time_base.num,
//                 params.time_base.den,
//                 params.sample_rate,
//                 params.format.get_sample_fmt_name(),
//                 channel_desc.to_string_lossy(),
//             );
//             CString::new(args)?
//         };
//
//         let buffersrc = AVFilter::get_by_name(c"audio_buffer_src")
//             .ok_or_else(|| anyhow::anyhow!("Failed to get audio buffer source filter"))?;
//         let buffersink = AVFilter::get_by_name(c"audio_buffer_sink")
//             .ok_or_else(|| anyhow::anyhow!("Failed to get audio buffer sink filter"))?;
//
//         let src_ctx = graph
//             .create_filter_context(&buffersrc, c"in", Some(&args))
//             .context("Failed to create audio buffer source")?;
//
//         let mut sink_ctx = graph
//             .create_filter_context(&buffersink, c"out", None)
//             .context("Failed to create audio buffer sink")?;
//
//         sink_ctx.opt_set_bin(c"sample_fmts", &(params.format as i32))?;
//         sink_ctx.opt_set_bin(c"channel_layouts", &channel_desc)?;
//         sink_ctx.opt_set_bin(c"sample_rates", &params.sample_rate)?;
//
//         Ok((src_ctx, sink_ctx))
//     }
// }
//
// impl std::fmt::Debug for FilterGraphContext {
//     fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//         write!(f, "FilterGraphContext nb_filters:{}", self.graph.nb_filters)
//     }
// }
//
// #[derive(Debug)]
// pub struct StreamFilterWrapper<'graph> {
//     pub graph: Arc<FilterGraphContext>, // 使用 Arc 来共享所有权
//     pub filter: FilterContext<'graph>,
// }
//
// #[derive(Debug)]
// pub struct StreamFilterContext<'graph> {
//     pub filters: HashMap<usize, StreamFilterWrapper<'graph>>,
// }
//
// impl<'graph> StreamFilterContext<'graph> {
//     #[allow(clippy::new_without_default)]
//     pub fn new() -> Self {
//         Self {
//             filters: HashMap::new(),
//         }
//     }
//
//     pub fn add_filter(
//         &mut self,
//         stream_index: usize,
//         filter_params: &FilterParams,
//         filters: &[Filter],
//     ) -> Result<()> {
//         if self.filters.contains_key(&stream_index) {
//             return Err(anyhow::anyhow!(
//                 "Filter already exists for stream index {}",
//                 stream_index
//             ));
//         };
//
//         let graph = FilterGraphContext::new();
//
//         // 通过引用创建 filter context，确保生命周期正确
//         let filter_ctx = {
//             let graph_ref: &'graph FilterGraphContext = unsafe { std::mem::transmute(&graph) };
//             graph_ref.create_filter_context(filter_params, filters)?
//         };
//
//         self.filters.insert(
//             stream_index,
//             StreamFilterWrapper {
//                 graph: Arc::new(graph),
//                 filter: filter_ctx,
//             },
//         );
//         Ok(())
//     }
//
//     pub fn process_frame(
//         &mut self,
//         stream_index: usize,
//         frame: Option<AVFrame>,
//     ) -> Result<Option<AVFrame>> {
//         if let Some(wrapper) = self.filters.get_mut(&stream_index) {
//             let filter_ctx = &mut wrapper.filter;
//             return filter_ctx.process_frame(frame);
//         }
//         Ok(None)
//     }
//
//     pub fn flush(&mut self, stream_index: usize) -> Result<Vec<AVFrame>> {
//         if let Some(wrapper) = self.filters.get_mut(&stream_index) {
//             let filter_ctx = &mut wrapper.filter;
//             return filter_ctx.flush();
//         }
//         Ok(vec![])
//     }
// }
