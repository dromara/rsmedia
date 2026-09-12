use crate::error::{Result, RsmediaError};
use crate::fmt::FrameFormat;
use crate::hwaccel::HWDeviceType;
use crate::io::{Reader, Writer};
use crate::strutils;
use crate::{Metadata, PixelFormat, SampleFormat};

use rsmpeg::avcodec::{AVCodec, AVCodecParameters};
use rsmpeg::avformat::AVStream;
use rsmpeg::avutil::{self, AVChannelLayout};
use rsmpeg::ffi;

use std::fmt::{Display, Formatter};
use std::ops::Deref;

// 由单源表生成枚举与双向映射：判别值即 FFmpeg 常量值，
// 未知/版本差异的 `AVMEDIA_TYPE_*` 回退为 `UNKNOWN`（而非 panic）。
// 枚举 doc 写在宏调用括号内（`#[$em]` 转发到生成的枚举）——
// 挂在宏调用外部的 doc 注释 rustdoc 不认，会触发 unused_doc_comments 警告。
ffi_enum_wrap_from!(
    /// Media type (FFmpeg `AVMEDIA_TYPE_*`): the classification of a stream.
    ///
    /// Generated from one `variant => constant` table with a two-way `From`. A value the table
    /// does not list panics rather than degrading to `UNKNOWN`, so a stream type this crate does
    /// not model is reported immediately instead of being silently treated as unknown — the
    /// listed `AVMEDIA_TYPE_UNKNOWN` still converts to `UNKNOWN` as usual.
    MediaType => ffi::AVMediaType,
    repr = i32,
    fallback = panic {
        UNKNOWN => ffi::AVMEDIA_TYPE_UNKNOWN;
        VIDEO => ffi::AVMEDIA_TYPE_VIDEO;
        AUDIO => ffi::AVMEDIA_TYPE_AUDIO;
        DATA => ffi::AVMEDIA_TYPE_DATA;
        SUBTITLE => ffi::AVMEDIA_TYPE_SUBTITLE;
        ATTACHMENT => ffi::AVMEDIA_TYPE_ATTACHMENT;
    }
);

impl MediaType {
    pub fn get_media_name(&self) -> String {
        avutil::get_media_type_string(*self as _).map_or("Unknown".to_string(), |s| {
            strutils::cstr_to_string(s).unwrap()
        })
    }
}

impl Display for MediaType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.get_media_name())
    }
}

/// Holds transferable stream information. This can be used to duplicate stream settings for the
/// purpose of transmuxing or transcoding.
#[derive(Clone)]
pub struct StreamInfo {
    /// id
    pub id: i32,
    /// Stream index
    pub index: usize,
    /// Media type video/audio/subtitle
    pub media_type: MediaType,
    /// Stream codec `ffi::AVCodecID`
    pub codec_id: u32,
    /// Codec Additional Info
    pub codec_tag: u32,
    /// 统一格式表示：Video 为 [`FrameFormat::Pixel`]（[`PixelFormat`]），
    /// Audio 为 [`FrameFormat::Sample`]（[`SampleFormat`]）；
    /// 字幕/数据等无格式概念的流回退为 [`FrameFormat::Pixel`]([`PixelFormat::NONE`])。
    pub format: FrameFormat,
    /// Number of bits per sample or zero if unknown for the given codec.
    pub bits_per_sample: i32,
    /// Only return non-zero if the bits per sample is exactly correct, not an approximation.
    pub exact_bits_per_sample: i32,
    /// the number of bits actually used for storing the pixel information,
    /// that is padding bits are not counted.
    pub bits_per_pixel: i32,
    /// the number of bits per pixel for the pixel format
    /// including any padding or unused bits.
    pub padded_bits_per_pixel: i32,

    /// time_base of stream
    pub time_base: ffi::AVRational,
    /// Stream Duration
    pub duration: i64,
    /// Start time
    pub start_time: i64,
    /// Number of frames
    pub nb_frames: i64,
    /// Bit rate
    pub bit_rate: i64,
    /// combination of AV_DISPOSITION_*
    pub disposition: i32,
    /// codec profile
    pub profile: i32,
    /// codec level, eg. 3.1, 4.1 etc.
    pub level: i32,

    // Video parameters
    /// Video width
    pub width: i32,
    /// Video height
    pub height: i32,
    /// Video frame rate FPS
    pub frame_rate: ffi::AVRational,
    pub avg_frame_rate: ffi::AVRational,
    pub real_frame_rate: ffi::AVRational,
    /// Number of bits in timestamps. Used for wrapping control.
    pub pts_wrap_bits: i32,
    /// video_delay
    pub video_delay: i32,
    /// Video sample aspect ratio
    pub sample_aspect_ratio: ffi::AVRational,
    /// Display aspect ratio
    pub display_aspect_ratio: ffi::AVRational,
    /// Video color space, eg: ffi::AVCOL_SPC_*
    pub color_space: ffi::AVColorSpace,
    /// Video color range, eg: ffi::AVCOL_RANGE_*
    pub color_range: ffi::AVColorRange,
    /// Video color primaries, eg: ffi::AVCOL_PRI_*
    pub color_primaries: ffi::AVColorPrimaries,
    /// Video color transfer, eg: ffi::AVCOL_TRC_*
    pub color_transfer: ffi::AVColorTransferCharacteristic,
    /// Location of chroma samples, eg: ffi::AVCHROMA_LOC_*
    pub chroma_location: ffi::AVChromaLocation,
    /// Video field order
    pub field_order: ffi::AVFieldOrder,
    /// Video rotation
    pub rotation: f64,

    // Audio parameters
    /// Audio sample rate
    pub sample_rate: i32,
    /// Audio Channel layout
    pub channel_layout: AVChannelLayout,
    /// Audio frame size
    pub frame_size: i32,
    /// Audio block align
    pub block_align: i32,
    /// Initial padding
    pub initial_padding: i32,
    /// Trailing padding
    pub trailing_padding: i32,
    /// Seek preroll
    pub seek_preroll: i32,
    /// The number of bits per code sample
    pub bits_per_coded_sample: i32,
    /// Raw Sample Bit Depth
    pub bits_per_raw_sample: i32,
    /// number of bytes per sample
    pub bytes_per_sample: Option<usize>,

    // extra
    pub extra_data: Option<Vec<u8>>,
    pub metadata: Metadata,
    /// **owned 快照**：构建本 `StreamInfo` 时通过 `avcodec_parameters_copy`
    /// 深拷贝得到的 codec 参数，生命周期完全独立于源 reader/writer，可由
    /// [`Self::into_parts`] 取出透传给 mux。
    ///
    /// rsmpeg 的 [`AVCodecParameters`] 在 `Drop` 时调用
    /// `avcodec_parameters_free` 释放，故不会泄漏，也不存在 use-after-free。
    pub codec_parameters: AVCodecParameters,
}

impl StreamInfo {
    /// Fetch stream information from a reader by stream index.
    ///
    /// # Arguments
    ///
    /// * `reader` - Reader to find stream information from.
    /// * `stream_index` - Index of stream in reader.
    pub fn from_reader<R: Reader>(reader: &R, stream_index: usize) -> Result<Self> {
        let stream = reader
            .input()
            .streams()
            .get(stream_index)
            .ok_or(RsmediaError::custom(format!(
                "reader stream: {stream_index} not found!"
            )))?;

        Self::from_stream(stream)
    }

    pub fn from_writer<W: Writer>(writer: &W, stream_index: usize) -> Result<Self> {
        let stream = writer
            .output()
            .streams()
            .get(stream_index)
            .ok_or(RsmediaError::custom(format!(
                "writer stream: {stream_index} not found!"
            )))?;

        Self::from_stream(stream)
    }

    pub fn from_stream(stream: &AVStream) -> Result<Self> {
        let codecpar = stream.codecpar();
        let codec_type = codecpar.codec_type();
        let metadata = stream
            .metadata()
            .map_or_else(Metadata::new, |d| Metadata::from_dict(&d));
        // 统一格式：视频 → 像素格式，音频 → 采样格式，其他 → NONE 占位
        let format = if codec_type.is_video() {
            FrameFormat::Pixel(PixelFormat::from(codecpar.format))
        } else if codec_type.is_audio() {
            FrameFormat::Sample(SampleFormat::from(codecpar.format))
        } else {
            FrameFormat::Pixel(PixelFormat::NONE)
        };

        let bytes_per_sample = format.into_sample().and_then(|s| s.get_bytes_per_sample());

        // descriptor() 返回 Result，未知格式时返回错误而非 panic
        let pix_fmt_desc = if codec_type.is_video() {
            Some(PixelFormat::from(codecpar.format).descriptor()?)
        } else {
            None
        };

        let (bits_per_sample, exact_bits_per_sample, bits_per_pixel, padded_bits_per_pixel) = unsafe {
            let bits_sample = ffi::av_get_bits_per_sample(codecpar.codec_id);
            let exact_bits_sample = ffi::av_get_exact_bits_per_sample(codecpar.codec_id);
            let (bits_pixel, padded_bits_pixel) = if let Some(pix_fmt_desc) = pix_fmt_desc {
                (
                    ffi::av_get_bits_per_pixel(pix_fmt_desc.deref()),
                    ffi::av_get_padded_bits_per_pixel(pix_fmt_desc.deref()),
                )
            } else {
                (0, 0)
            };
            (
                bits_sample,
                exact_bits_sample,
                bits_pixel,
                padded_bits_pixel,
            )
        };

        Ok(Self {
            id: stream.id,
            index: stream.index as usize,
            media_type: MediaType::from(codecpar.codec_type),
            #[allow(clippy::unnecessary_cast)]
            codec_id: codecpar.codec_id as u32,
            codec_tag: codecpar.codec_tag,
            format,
            bits_per_sample,
            exact_bits_per_sample,
            bits_per_pixel,
            padded_bits_per_pixel,
            time_base: stream.time_base,
            duration: stream.duration,
            start_time: stream.start_time,
            nb_frames: stream.nb_frames,
            disposition: stream.disposition,
            profile: codecpar.profile,
            level: codecpar.level,
            // Video
            width: codecpar.width,
            height: codecpar.height,
            bit_rate: codecpar.bit_rate,
            frame_rate: codecpar.framerate,
            avg_frame_rate: stream.avg_frame_rate,
            real_frame_rate: stream.r_frame_rate,
            pts_wrap_bits: stream.pts_wrap_bits,
            video_delay: codecpar.video_delay,
            sample_aspect_ratio: codecpar.sample_aspect_ratio,
            display_aspect_ratio: Self::compute_display_aspect_ratio(
                codecpar.sample_aspect_ratio,
                codecpar.width,
                codecpar.height,
            ),
            color_space: codecpar.color_space,
            color_range: codecpar.color_range,
            color_transfer: codecpar.color_trc,
            color_primaries: codecpar.color_primaries,
            chroma_location: codecpar.chroma_location,
            field_order: codecpar.field_order,
            rotation: Self::get_stream_display_rotation(stream, &metadata),
            // Audio
            sample_rate: codecpar.sample_rate,
            channel_layout: codecpar.ch_layout().clone(),
            frame_size: codecpar.frame_size,
            block_align: codecpar.block_align,
            initial_padding: codecpar.initial_padding,
            trailing_padding: codecpar.trailing_padding,
            seek_preroll: codecpar.seek_preroll,
            bits_per_coded_sample: codecpar.bits_per_coded_sample,
            bits_per_raw_sample: codecpar.bits_per_raw_sample,
            bytes_per_sample,
            // extra
            metadata,
            extra_data: Self::get_extra_data(stream),
            // owned 深拷贝快照：`AVCodecParameters::clone` 内部经
            // `avcodec_parameters_copy` 拷贝，Drop 时释放，生命周期独立。
            codec_parameters: stream.codecpar().clone(),
        })
    }

    /// 计算显示宽高比 DAR = SAR × (width / height)。
    ///
    /// SAR 未知（0/1，FFmpeg 惯例按方形像素处理）时退化为 width/height。
    /// 用 `av_reduce` 规约分数（与 FFmpeg 内部一致），避免溢出且得到最简比。
    fn compute_display_aspect_ratio(
        sample_aspect_ratio: ffi::AVRational,
        width: i32,
        height: i32,
    ) -> ffi::AVRational {
        if width <= 0 || height <= 0 {
            return ffi::AVRational { num: 0, den: 1 };
        }
        // SAR 未知/非法时按方形像素（1/1）处理
        let (sar_num, sar_den) = if sample_aspect_ratio.num <= 0 || sample_aspect_ratio.den <= 0 {
            (1, 1)
        } else {
            (
                sample_aspect_ratio.num as i64,
                sample_aspect_ratio.den as i64,
            )
        };
        let mut num: i32 = 0;
        let mut den: i32 = 1;
        unsafe {
            ffi::av_reduce(
                &mut num,
                &mut den,
                sar_num * width as i64,
                sar_den * height as i64,
                i32::MAX as i64,
            );
        }
        ffi::AVRational { num, den }
    }

    /// 读取视频流旋转角度（度，顺时针）。
    ///
    /// 优先级：
    /// 1. **side_data display matrix**（`AV_PKT_DATA_DISPLAYMATRIX`，FFmpeg 6+
    ///    移到 `codecpar.coded_side_data`）：手机拍摄视频的标准存储方式，
    ///    `av_display_rotation_get` 返回**逆时针**角度，取负转为顺时针语义。
    /// 2. `rotate` metadata 标签：旧版 mov/mp4 demuxer 的写入方式（新版
    ///    demuxer 会同时写入 side_data，两者一致时优先 side_data）。
    fn get_stream_display_rotation(stream: &AVStream, map: &Metadata) -> f64 {
        // 1. side_data display matrix（codecpar.coded_side_data，FFmpeg 6+）
        let codecpar = stream.codecpar();
        let nb_side_data = codecpar.nb_coded_side_data.max(0) as usize;
        if nb_side_data > 0 && !codecpar.coded_side_data.is_null() {
            unsafe {
                let entries = std::slice::from_raw_parts(codecpar.coded_side_data, nb_side_data);
                for entry in entries {
                    if entry.type_ == ffi::AV_PKT_DATA_DISPLAYMATRIX && entry.size >= 9 * 4 {
                        // av_display_rotation_get 返回逆时针角度，取负为顺时针
                        return -ffi::av_display_rotation_get(entry.data as *const i32);
                    }
                }
            }
        }

        // 2. rotate metadata 标签（旧 demuxer 回退）
        map.get("rotate")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0)
    }

    fn get_extra_data(stream: &AVStream) -> Option<Vec<u8>> {
        let codecpar = stream.codecpar();
        if codecpar.extradata_size > 0 && !codecpar.extradata.is_null() {
            let extra_data = unsafe {
                std::slice::from_raw_parts(
                    codecpar.extradata as *const _,
                    codecpar.extradata_size as usize,
                )
            };
            Some(extra_data.to_vec())
        } else {
            None
        }
    }

    /// Turn information back into parts for usage.
    ///
    /// Note: Consumes stream information object.
    ///
    /// # Return value
    ///
    /// A tuple consisting of:
    /// * The stream index.
    /// * Owned codec parameters snapshot.
    /// * Original stream time base.
    pub fn into_parts(self) -> (usize, AVCodecParameters, ffi::AVRational) {
        (self.index, self.codec_parameters, self.time_base)
    }

    /// find codec name, if have hw_device_type, will use hw accelerated codec name
    /// if not, will use current stream codec name
    ///
    /// 硬件解码器名先经 `find_decoder_by_name` 验证存在（表项可能因 FFmpeg
    /// 版本/编译选项不存在，如 ffmpeg6 无 `*_vulkan` 解码器），不存在时
    /// 回退到通用软件解码器名。
    pub fn find_decoder_name(&self, hw_device_type: Option<HWDeviceType>) -> Option<String> {
        let codec_id = self.codec_id as ffi::AVCodecID;
        let codec_name = strutils::cstr_to_string_lossy(AVCodec::find_decoder(codec_id)?.name());
        let hw_codec_name = hw_device_type
            .and_then(|hw| hw_decoder_name(hw, codec_id))
            .filter(|name| {
                let exists =
                    AVCodec::find_decoder_by_name(&strutils::str_to_cstring(name)).is_some();
                if !exists {
                    log::debug!(
                        "HW decoder '{name}' not registered in this FFmpeg build, \
                         falling back to software decoder '{codec_name}'"
                    );
                }
                exists
            });
        Some(hw_codec_name.unwrap_or(codec_name))
    }

    /// find encoder name, if we have hw_device_type, will use hw accelerated codec name
    /// if not, will use current stream codec name
    ///
    /// 与 [`Self::find_decoder_name`] 对称：硬件编码器名同样经验证存在后才使用。
    pub fn find_encoder_name(&self, hw_device_type: Option<HWDeviceType>) -> Option<String> {
        let codec_id = self.codec_id as ffi::AVCodecID;
        let codec_name = strutils::cstr_to_string_lossy(AVCodec::find_encoder(codec_id)?.name());
        let hw_codec_name = hw_device_type
            .and_then(|hw| hw_encoder_name(hw, codec_id))
            .filter(|name| {
                let exists =
                    AVCodec::find_encoder_by_name(&strutils::str_to_cstring(name)).is_some();
                if !exists {
                    log::debug!(
                        "HW encoder '{name}' not registered in this FFmpeg build, \
                         falling back to software encoder '{codec_name}'"
                    );
                }
                exists
            });
        Some(hw_codec_name.unwrap_or(codec_name))
    }
}

/// 返回解码器对应的硬件加速编解码器名称（如 `h264_cuvid`），当前设备类型
/// 不支持该编码时返回 `None`（调用方随后回退到软件解码器）。
fn hw_decoder_name(hw_type: HWDeviceType, codec_id: ffi::AVCodecID) -> Option<String> {
    match hw_type {
        HWDeviceType::CUDA => match codec_id {
            ffi::AV_CODEC_ID_H264 => Some("h264_cuvid".to_string()),
            ffi::AV_CODEC_ID_HEVC => Some("hevc_cuvid".to_string()),
            ffi::AV_CODEC_ID_MPEG1VIDEO => Some("mpeg1_cuvid".to_string()),
            ffi::AV_CODEC_ID_MPEG2VIDEO => Some("mpeg2_cuvid".to_string()),
            ffi::AV_CODEC_ID_MPEG4 => Some("mpeg4_cuvid".to_string()),
            ffi::AV_CODEC_ID_VC1 => Some("vc1_cuvid".to_string()),
            ffi::AV_CODEC_ID_VP8 => Some("vp8_cuvid".to_string()),
            ffi::AV_CODEC_ID_VP9 => Some("vp9_cuvid".to_string()),
            ffi::AV_CODEC_ID_AV1 => Some("av1_cuvid".to_string()),
            ffi::AV_CODEC_ID_MJPEG => Some("mjpeg_cuvid".to_string()),
            _ => None,
        },
        HWDeviceType::QSV => match codec_id {
            ffi::AV_CODEC_ID_H264 => Some("h264_qsv".to_string()),
            ffi::AV_CODEC_ID_HEVC => Some("hevc_qsv".to_string()),
            ffi::AV_CODEC_ID_MPEG2VIDEO => Some("mpeg2_qsv".to_string()),
            ffi::AV_CODEC_ID_VC1 => Some("vc1_qsv".to_string()),
            ffi::AV_CODEC_ID_VP8 => Some("vp8_qsv".to_string()),
            ffi::AV_CODEC_ID_VP9 => Some("vp9_qsv".to_string()),
            ffi::AV_CODEC_ID_AV1 => Some("av1_qsv".to_string()),
            ffi::AV_CODEC_ID_MJPEG => Some("mjpeg_qsv".to_string()),
            _ => None,
        },
        HWDeviceType::VULKAN => match codec_id {
            ffi::AV_CODEC_ID_H264 => Some("h264_vulkan".to_string()),
            ffi::AV_CODEC_ID_HEVC => Some("hevc_vulkan".to_string()),
            ffi::AV_CODEC_ID_AV1 => Some("av1_vulkan".to_string()),
            _ => None,
        },
        _ => None,
    }
}

/// 硬件编码器对应的硬件编解码器名称（如 `h264_nvenc`），设备类型不支持该
/// 编码时返回 `None`（调用方随后回退到软件编码器）。
fn hw_encoder_name(hw_type: HWDeviceType, codec_id: ffi::AVCodecID) -> Option<String> {
    match hw_type {
        HWDeviceType::CUDA => match codec_id {
            ffi::AV_CODEC_ID_H264 => Some("h264_nvenc".to_string()),
            ffi::AV_CODEC_ID_HEVC => Some("hevc_nvenc".to_string()),
            ffi::AV_CODEC_ID_AV1 => Some("av1_nvenc".to_string()),
            _ => None,
        },
        HWDeviceType::QSV => match codec_id {
            ffi::AV_CODEC_ID_H264 => Some("h264_qsv".to_string()),
            ffi::AV_CODEC_ID_HEVC => Some("hevc_qsv".to_string()),
            ffi::AV_CODEC_ID_MPEG2VIDEO => Some("mpeg2_qsv".to_string()),
            ffi::AV_CODEC_ID_VP9 => Some("vp9_qsv".to_string()),
            ffi::AV_CODEC_ID_AV1 => Some("av1_qsv".to_string()),
            ffi::AV_CODEC_ID_MJPEG => Some("mjpeg_qsv".to_string()),
            _ => None,
        },
        HWDeviceType::VAAPI => match codec_id {
            ffi::AV_CODEC_ID_H264 => Some("h264_vaapi".to_string()),
            ffi::AV_CODEC_ID_HEVC => Some("hevc_vaapi".to_string()),
            ffi::AV_CODEC_ID_MPEG2VIDEO => Some("mpeg2_vaapi".to_string()),
            ffi::AV_CODEC_ID_VP8 => Some("vp8_vaapi".to_string()),
            ffi::AV_CODEC_ID_VP9 => Some("vp9_vaapi".to_string()),
            ffi::AV_CODEC_ID_AV1 => Some("av1_vaapi".to_string()),
            ffi::AV_CODEC_ID_MJPEG => Some("mjpeg_vaapi".to_string()),
            _ => None,
        },
        HWDeviceType::VIDEOTOOLBOX => match codec_id {
            ffi::AV_CODEC_ID_H264 => Some("h264_videotoolbox".to_string()),
            ffi::AV_CODEC_ID_HEVC => Some("hevc_videotoolbox".to_string()),
            ffi::AV_CODEC_ID_PRORES => Some("prores_videotoolbox".to_string()),
            _ => None,
        },
        HWDeviceType::VULKAN => match codec_id {
            ffi::AV_CODEC_ID_H264 => Some("h264_vulkan".to_string()),
            ffi::AV_CODEC_ID_HEVC => Some("hevc_vulkan".to_string()),
            _ => None,
        },
        // Windows：D3D11VA 设备类型承载 AMD AMF 编码器
        //（AMF 无独立 hwcontext，挂在 d3d11va 下）。
        HWDeviceType::D3D11VA => match codec_id {
            ffi::AV_CODEC_ID_H264 => Some("h264_amf".to_string()),
            ffi::AV_CODEC_ID_HEVC => Some("hevc_amf".to_string()),
            ffi::AV_CODEC_ID_AV1 => Some("av1_amf".to_string()),
            _ => None,
        },
        _ => None,
    }
}

impl std::fmt::Debug for StreamInfo {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        let codec_name = unsafe {
            let codec_id = self.codec_id as ffi::AVCodecID;
            strutils::c_char_to_str(ffi::avcodec_get_name(codec_id))
        };
        let format = match self.format {
            FrameFormat::Pixel(p) => p.get_pix_fmt_name().to_owned(),
            FrameFormat::Sample(s) => s.get_sample_fmt_name(),
        };
        let stream_type = self.media_type.get_media_name();
        write!(
            f,
            "{} #{}: codec={}, format={}, size={}x{}, fps={:?}, bit_rate={}, sample_rate={}, nb_channels={}, video_delay={}",
            stream_type,
            self.index,
            codec_name,
            format,
            self.width,
            self.height,
            self.avg_frame_rate,
            self.bit_rate,
            self.sample_rate,
            self.channel_layout.nb_channels,
            self.video_delay,
        )
    }
}

impl Display for StreamInfo {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

// `StreamInfo` 完全持有自身数据（`codec_parameters` 为深拷贝的 owned 快照），
// 不引用任何外部 reader/writer 的生命周期，可安全跨线程传递与共享。
// 由于 rsmpeg 的 `AVCodecParameters` 仅实现了 `Send` 而未实现 `Sync`，
// `StreamInfo` 无法自动推导 `Sync`，此处手动补上（比较 `&self` 只读访问
// 快照字段，无数据竞争）。
unsafe impl Send for StreamInfo {}
unsafe impl Sync for StreamInfo {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::StreamReader;

    /// DAR = SAR × (W/H)，各退化路径与规约均正确。
    #[test]
    fn test_compute_display_aspect_ratio() {
        let ar = |num: i32, den: i32| ffi::AVRational { num, den };
        let eq_ar = |a: ffi::AVRational, b: ffi::AVRational| a.num == b.num && a.den == b.den;

        // SAR 未知（0/1）→ 方形像素，DAR = W/H
        assert!(eq_ar(
            StreamInfo::compute_display_aspect_ratio(ar(0, 1), 1920, 1080),
            ar(16, 9)
        ));
        // SAR 1/1 → DAR = W/H
        assert!(eq_ar(
            StreamInfo::compute_display_aspect_ratio(ar(1, 1), 1920, 1080),
            ar(16, 9)
        ));
        // 变形宽银幕：SAR 2/1 + 720x576(5/4) → DAR = 5/2
        assert!(eq_ar(
            StreamInfo::compute_display_aspect_ratio(ar(2, 1), 720, 576),
            ar(5, 2)
        ));
        // 分数需规约：SAR 118/81 + 1920x1080 → 118*16/(81*9) = 1888/729（已最简）
        assert!(eq_ar(
            StreamInfo::compute_display_aspect_ratio(ar(118, 81), 1920, 1080),
            ar(1888, 729)
        ));
        // 非视频流（W/H 为 0）→ 0/1
        assert!(eq_ar(
            StreamInfo::compute_display_aspect_ratio(ar(1, 1), 0, 0),
            ar(0, 1)
        ));
    }

    /// VAAPI 必须回退到通用软件解码器（`h264_vaapi` 是编码器名，
    /// 若返回会导致 Demuxer 构建失败）。
    #[test]
    fn test_find_decoder_name_vaapi_falls_back() {
        let reader =
            StreamReader::new(std::path::Path::new("assets/mp4.mp4")).expect("open test asset");
        let info = StreamInfo::from_reader(&reader, 0).expect("read stream 0");
        assert_eq!(info.media_type, MediaType::VIDEO);

        let generic = info.find_decoder_name(None).expect("generic decoder");
        let vaapi = info
            .find_decoder_name(Some(HWDeviceType::VAAPI))
            .expect("vaapi lookup");
        assert_eq!(
            vaapi, generic,
            "VAAPI must fall back to the generic decoder"
        );
        assert_ne!(vaapi, "h264_vaapi");
    }

    /// 存在性验证：表中列出但当前 FFmpeg 构建未注册的硬件解码器（如
    /// ffmpeg6 无 `*_vulkan`）应回退到软件解码器，而不是返回无效名字。
    #[test]
    fn test_find_decoder_name_unregistered_hw_falls_back() -> Result<()> {
        let reader = StreamReader::new("assets/mp4.mp4")?;
        let info = StreamInfo::from_reader(&reader, 0)?;

        for hw in [HWDeviceType::CUDA, HWDeviceType::VULKAN, HWDeviceType::QSV] {
            let name = info
                .find_decoder_name(Some(hw))
                .ok_or_else(|| RsmediaError::custom(format!("lookup for {hw:?} failed")))?;
            let registered = if let Some(codec) =
                AVCodec::find_decoder_by_name(&strutils::str_to_cstring(&name))
            {
                strutils::cstr_to_string(codec.name()).unwrap() == name
            } else {
                false
            };
            assert!(
                registered,
                "find_decoder_name({hw:?}) returned '{name}' which is not registered"
            );
        }
        Ok(())
    }
}
