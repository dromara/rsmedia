use crate::error::{Context, Result, RsmediaError};
use crate::fmt::FrameFormat;
use crate::options::Metadata;
use crate::pixel::PixelFormat;
use crate::{MediaType, SampleFormat, imgutils, time};

use rsmpeg::avutil::{AVChannelLayout, AVFrame};
use rsmpeg::ffi;

use yuv::{
    BufferStoreMut, YuvConversionMode, YuvPlanarImage, YuvPlanarImageMut, YuvRange,
    YuvStandardMatrix,
};

pub trait MediaFrameType:
    'static
    + Clone
    + Copy
    + Send
    + Sync
    + Default
    + PartialOrd
    + num_traits::Zero
    + num_traits::NumCast
    + num_traits::NumAssign
{
}

impl MediaFrameType for i8 {}
impl MediaFrameType for u8 {}
impl MediaFrameType for i16 {}
impl MediaFrameType for u16 {}
impl MediaFrameType for i32 {}
impl MediaFrameType for u32 {}
impl MediaFrameType for i64 {}
impl MediaFrameType for u64 {}
impl MediaFrameType for f32 {}
impl MediaFrameType for f64 {}

/// A frame array is the `ndarray` version of `AVFrame`
/// It is 3-dimensional array with dims `(H, W, C)` and type byte.
///
/// # Field coverage
///
/// Every value-carrying field of `AVFrame` is mirrored here, so
/// [`from_avframe`](MediaFrame::from_avframe) and [`to_avframe`](MediaFrame::to_avframe)
/// form a lossless round trip for the modelled fields.
///
/// The `AVFrame` fields that are *not* mirrored are the ones that cannot be carried as
/// values: `data` / `linesize` / `extended_data` are what [`data`](Self::data) packs
/// into an `ndarray`, `buf` / `extended_buf` / `nb_extended_buf` / `opaque` /
/// `opaque_ref` / `private_ref` are ownership handles, and `hw_frames_ctx` describes a
/// hardware frame pool that has no meaning once the samples are copied into host
/// memory. They are intentionally excluded rather than missing.
///
/// # Parameters
///
/// * `T` - The underlying data type for samples/pixels:
///   * For video: typically `u8`
///   * For audio: `i16`, `i32`, `f32`, or `f64`
///
/// # Safety
///
/// The type parameter `T` must match the frame's format:
/// * For RGB24: `u8`
/// * For FLTP: `f32`
#[derive(Debug, Clone)]
pub struct MediaFrame<T> {
    /// Presentation timestamp, in `time_base` units.
    ///
    /// `AV_NOPTS_VALUE` means "not set": the encoder then assigns pts automatically
    /// (video: one frame per `1/fps` tick; audio: sample-position counting), so a
    /// freshly created frame does not have to carry a hand-computed pts.
    pub pts: i64,
    /// Decode timestamp copied from the source packet, in `time_base` units.
    ///
    /// Named after `AVFrame.pkt_dts`; `AV_NOPTS_VALUE` when unset — the default an
    /// `AVFrame` carries — matching [`best_effort_timestamp`](Self::best_effort_timestamp).
    pub pkt_dts: i64,
    /// Frame duration in `time_base` units; `0` when unknown or unset.
    ///
    /// This is the canonical duration and the field written back to
    /// `AVFrame.duration`; [`pkt_duration`](Self::pkt_duration) mirrors it.
    pub duration: i64,
    /// Mirror of [`duration`](Self::duration), kept for the pre-7.0
    /// `AVFrame.pkt_duration` name. FFmpeg 7 dropped that AVFrame field and only
    /// `AVFrame.duration` remains, so both fields here always hold the same value.
    pub pkt_duration: i64,
    /// 像素/采样格式（统一表示）。
    /// Video: [`FrameFormat::Pixel`]（含 [`PixelFormat`]）
    /// Audio: [`FrameFormat::Sample`]（含 [`SampleFormat`]）
    pub format: FrameFormat,
    /// 通用音频/视频数据（ndarray 存储，C 连续布局）。
    /// Video: `[height, width, channels]`
    /// Audio: `[frames, nb_samples, nb_channels]`
    pub data: ndarray::Array3<T>,
    /// 时间基。时间戳、时长等字段均按此换算物理时间。
    /// Video: `1 / frame_rate`
    /// Audio: `1 / sample_rate`
    pub time_base: ffi::AVRational,
    /// 媒体类型（仅 Video / Audio 二者之一）。
    /// only for Video / Audio: [`MediaType`]
    pub media_type: MediaType,
    // Video
    /// 仅视频字段：图像宽度（像素）。
    pub width: usize,
    /// 仅视频字段：图像高度（像素）。
    pub height: usize,
    /// 仅视频字段：图像类型（I/P/B 帧等，`AVPictureType`）。
    pub pict_type: ffi::AVPictureType,
    // Audio
    /// 仅音频字段：采样率（Hz）。
    pub sample_rate: u32,
    /// 仅音频字段：本帧采样数（每通道）。
    pub nb_samples: u32,
    /// 仅音频字段：声道数。
    pub nb_channels: u32,
    /// 是否为关键帧（来自 AV_FRAME_FLAG_KEY）。
    pub key_frame: bool,
    /// 帧标志（AV_FRAME_FLAG_* 组合）。
    pub flags: i32,
    /// 编码质量（1 ~ FF_LAMBDA_MAX，越小越好；未设置时默认 0）。
    pub quality: i32,
    /// 应重复的场数（interlace 相关，通常为 0）。
    pub repeat_pict: i32,
    /// YUV colorspace (`AVColorSpace`, e.g. BT709); named after `AVFrame.colorspace`.
    ///
    /// Note the asymmetry across FFmpeg structs: `AVFrame` spells it `colorspace`
    /// while `AVCodecParameters` spells it `color_space` — each mirror here follows
    /// the struct it wraps.
    pub colorspace: ffi::AVColorSpace,
    /// 色彩原色（`AVColorPrimaries`）。
    pub color_primaries: ffi::AVColorPrimaries,
    /// 色彩传输特性（`AVColorTransferCharacteristic`）。
    pub color_trc: ffi::AVColorTransferCharacteristic,
    /// 色彩采样范围（`AVColorRange`，MPEG/JPEG）。
    pub color_range: ffi::AVColorRange,
    /// Chroma sample location (`AVChromaLocation`): where the chroma samples sit
    /// relative to the luma grid. `AVCHROMA_LOC_UNSPECIFIED` when unknown.
    pub chroma_location: ffi::AVChromaLocation,
    /// 像素宽高比（视频帧的 sample_aspect_ratio，0/1 表示未知）。
    pub sample_aspect_ratio: ffi::AVRational,
    /// Cropping rectangle in pixels: the coded picture has `crop_top` / `crop_bottom`
    /// rows and `crop_left` / `crop_right` columns discarded to obtain the region
    /// intended for presentation. All zero when the whole coded picture is shown.
    pub crop_top: usize,
    /// Discarded rows at the bottom of the coded picture; see [`Self::crop_top`].
    pub crop_bottom: usize,
    /// Discarded columns on the left of the coded picture; see [`Self::crop_top`].
    pub crop_left: usize,
    /// Discarded columns on the right of the coded picture; see [`Self::crop_top`].
    pub crop_right: usize,
    /// How the alpha channel is to be interpreted (`AVAlphaMode`).
    ///
    /// FFmpeg 8+ only: the field does not exist on 6/7, where alpha is unambiguous.
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    pub alpha_mode: ffi::AVAlphaMode,
    /// 最佳努力时间戳（解码器启发式估计，同 time_base 单位）。
    pub best_effort_timestamp: i64,
    /// Decoder error flags (`FF_DECODE_ERROR_*`): non-zero when the decoder produced
    /// the frame but the bitstream was damaged. `0` on a clean decode.
    pub decode_error_flags: i32,
    /// Frame-level key/value metadata, copied from `AVFrame.metadata`.
    ///
    /// A plain string map ([`Metadata`]) instead of the FFI `AVDictionary`: FFmpeg
    /// represents an empty dictionary as a null pointer, which is exactly what an
    /// empty map means.
    pub metadata: Metadata,
    /// Frame side data, copied out of `AVFrame.side_data` into owned buffers.
    ///
    /// Side data carries auxiliary per-frame payloads (HDR mastering metadata,
    /// display matrices, motion vectors, ...). Each entry keeps its
    /// `AVFrameSideDataType` discriminant and raw payload, so side-data types this
    /// crate does not know about still survive a round trip unchanged.
    pub side_data: Vec<FrameSideData>,
}

/// One entry of [`MediaFrame::side_data`], copied into owned memory.
///
/// The owned counterpart of `AVFrameSideData`. `AVFrameSideData.buf` (the owning
/// buffer reference) is deliberately not carried — [`data`](Self::data) is already a
/// full copy of the payload.
#[derive(Debug, Clone)]
pub struct FrameSideData {
    /// Side-data type (`AVFrameSideDataType`), e.g. `AV_FRAME_DATA_DISPLAYMATRIX`.
    pub type_: ffi::AVFrameSideDataType,
    /// Raw payload, exactly `AVFrameSideData.size` bytes.
    pub data: Vec<u8>,
    /// Entry-level metadata; empty when the entry carries no dictionary.
    pub metadata: Metadata,
}

impl<T: MediaFrameType> Default for MediaFrame<T> {
    /// 返回一个字段均为中性默认值的空帧；具体构造器（[`Self::new_video`] / [`Self::new_audio`]）
    /// 通过 struct-update 语法只覆盖本方相关的字段，从而消除重复的默认初始化。
    fn default() -> Self {
        Self {
            pts: ffi::AV_NOPTS_VALUE,
            pkt_dts: ffi::AV_NOPTS_VALUE,
            duration: 0,
            pkt_duration: 0,
            // 占位格式，具体构造器会覆盖；默认中性值用于避免越界访问。
            format: FrameFormat::Pixel(PixelFormat::NONE),
            data: Default::default(),
            time_base: time::new_rational(0, 1),
            media_type: MediaType::DATA,
            width: 0,
            height: 0,
            pict_type: ffi::AV_PICTURE_TYPE_NONE,
            sample_rate: 0,
            nb_samples: 0,
            nb_channels: 0,
            key_frame: false,
            flags: 0,
            quality: 0,
            repeat_pict: 0,
            // 色彩属性默认标记为“未知”（UNSPECIFIED/RANGE_UNSPECIFIED=0），
            // 避免把 0 误当成 AV_COL_SPC_RGB 写入 AVFrame，干扰滤镜/编码器的色彩判定。
            colorspace: ffi::AVCOL_SPC_UNSPECIFIED,
            color_primaries: ffi::AVCOL_PRI_UNSPECIFIED,
            color_trc: ffi::AVCOL_TRC_UNSPECIFIED,
            color_range: ffi::AVCOL_RANGE_UNSPECIFIED,
            chroma_location: ffi::AVCHROMA_LOC_UNSPECIFIED,
            sample_aspect_ratio: time::new_rational(0, 1),
            crop_top: 0,
            crop_bottom: 0,
            crop_left: 0,
            crop_right: 0,
            #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
            alpha_mode: ffi::AVALPHA_MODE_UNSPECIFIED,
            best_effort_timestamp: ffi::AV_NOPTS_VALUE,
            decode_error_flags: 0,
            metadata: Metadata::new(),
            side_data: Vec::new(),
        }
    }
}

impl<T> MediaFrame<T>
where
    T: MediaFrameType,
{
    /// 创建视频帧
    ///
    /// `data` 的 C 维度必须与格式的分量数一致：packed 8bit 格式为
    /// `packed_channels()`（GRAY8=1 / YUYV422、UYVY422=2 / RGB24、BGR24=3 /
    /// RGBA 族=4），YUV420P 为 3（U/V 以 2x2 块代表值存储）。
    pub fn new_video(
        width: usize,
        height: usize,
        format: PixelFormat,
        time_base: ffi::AVRational,
        data: ndarray::Array3<T>,
    ) -> Result<Self> {
        let (h, w, c) = data.dim();
        let expected_c = if format == PixelFormat::YUV420P {
            3
        } else {
            format.packed_channels().ok_or_else(|| {
                RsmediaError::custom(format!(
                    "Unsupported pixel format for ndarray: {format:?}, only YUV420P and \
                         packed 8-bit formats are supported"
                ))
            })?
        };
        if h != height || w != width || c != expected_c {
            return Err(RsmediaError::custom(format!(
                "Invalid dimensions: expected ({height}, {width}, {expected_c}), got ({h}, {w}, {c})"
            )));
        }

        Ok(Self {
            width,
            height,
            data,
            time_base,
            media_type: MediaType::VIDEO,
            format: FrameFormat::Pixel(format),
            ..Self::default()
        })
    }

    /// 创建视频帧（零初始化）
    pub fn new_video_frame(
        width: usize,
        height: usize,
        format: PixelFormat,
        time_base: ffi::AVRational,
    ) -> Result<Self>
    where
        T: num_traits::Zero,
    {
        let expected_c = if format == PixelFormat::YUV420P {
            3
        } else {
            format.packed_channels().ok_or_else(|| {
                RsmediaError::custom(format!(
                    "Unsupported pixel format for ndarray: {format:?}, only YUV420P and \
                         packed 8-bit formats are supported"
                ))
            })?
        };
        let data = ndarray::Array3::<T>::zeros((height, width, expected_c));
        Self::new_video(width, height, format, time_base, data)
    }

    /// 创建音频帧
    pub fn new_audio(
        format: SampleFormat,
        nb_channels: u32,
        nb_samples: u32,
        sample_rate: u32,
        time_base: ffi::AVRational,
        data: ndarray::Array3<T>,
    ) -> Result<Self> {
        let (frames, samples, ch) = data.dim();
        if frames != 1 || samples != nb_samples as usize || ch != nb_channels as usize {
            return Err(RsmediaError::custom(format!(
                "Invalid dimensions: expected (1, {nb_samples}, {nb_channels}), got ({frames}, {samples}, {ch})"
            )));
        }

        Ok(Self {
            format: FrameFormat::Sample(format),
            data,
            time_base,
            sample_rate,
            nb_samples,
            nb_channels,
            media_type: MediaType::AUDIO,
            ..Self::default()
        })
    }

    pub fn new_audio_frame(
        format: SampleFormat,
        nb_channels: u32,
        nb_samples: u32,
        sample_rate: u32,
        time_base: ffi::AVRational,
    ) -> Result<Self>
    where
        T: num_traits::Zero,
    {
        let data = ndarray::Array3::<T>::zeros((1, nb_samples as usize, nb_channels as usize));
        Self::new_audio(
            format,
            nb_channels,
            nb_samples,
            sample_rate,
            time_base,
            data,
        )
    }

    pub fn set_pts(&mut self, pts: i64) {
        self.pts = pts;
    }

    /// Returns the frame's unified format: [`FrameFormat::Pixel`] for video,
    /// [`FrameFormat::Sample`] for audio; `None` for other media types.
    #[inline]
    pub fn format(&self) -> Option<FrameFormat> {
        (self.media_type == MediaType::VIDEO || self.media_type == MediaType::AUDIO)
            .then_some(self.format)
    }

    pub fn set_pkt_dts(&mut self, pkt_dts: i64) {
        self.pkt_dts = pkt_dts;
    }

    pub fn set_time_base(&mut self, time_base: ffi::AVRational) {
        self.time_base = time_base;
    }

    pub fn set_sample_rate(&mut self, sample_rate: u32) {
        self.sample_rate = sample_rate;
    }

    pub fn from_avframe(frame: &AVFrame) -> Result<Self> {
        if frame.data[0].is_null() {
            return Err(RsmediaError::custom("Invalid frame data"));
        }

        let (width, height) = (frame.width as usize, frame.height as usize);
        let pts = frame.pts;
        let pkt_dts = frame.pkt_dts;
        let format = frame.format;
        let duration = frame.duration;
        // AVFrame 的 time_base 常未被解码器填充（默认 0/0）。无效时：
        // - 音频可由采样率推断为 1/sample_rate
        // - 视频无法从帧内推断帧率，告警并提示调用方设置
        let time_base = if frame.time_base.num == 0 || frame.time_base.den == 0 {
            if frame.nb_samples > 0 && frame.sample_rate > 0 {
                time::new_rational(1, frame.sample_rate)
            } else {
                log::warn!(
                    "AVFrame has no valid time_base ({:?}); call MediaFrame::set_time_base",
                    frame.time_base
                );
                frame.time_base
            }
        } else {
            frame.time_base
        };

        // 判断依据：nb_samples 是音频帧专属字段（视频帧恒为 0），
        // 是最可靠的音频信号；width/height 是视频帧的固有属性。
        if frame.nb_samples > 0 {
            // Audio frame
            let mut m = Self {
                format: FrameFormat::Sample(SampleFormat::from(format)),
                pts,
                pkt_dts,
                duration,
                pkt_duration: duration,
                time_base,
                data: audio_data(frame)?,
                media_type: MediaType::AUDIO,
                sample_rate: frame.sample_rate as u32,
                nb_samples: frame.nb_samples as u32,
                nb_channels: frame.ch_layout.nb_channels as u32,
                ..Self::default()
            };
            m.copy_avframe_meta(frame);
            Ok(m)
        } else if width > 0 && height > 0 {
            // Video frame
            let mut m = Self {
                width,
                height,
                pts,
                pkt_dts,
                format: FrameFormat::Pixel(PixelFormat::from(format)),
                duration,
                time_base,
                data: video_data(frame)?,
                media_type: MediaType::VIDEO,
                pict_type: frame.pict_type,
                pkt_duration: duration,
                ..Self::default()
            };
            m.copy_avframe_meta(frame);
            Ok(m)
        } else {
            Err(RsmediaError::custom("Unsupported frame format"))
        }
    }

    /// 从 `AVFrame` 拷贝与编解码/色彩相关的元数据字段（两个构造分支完全一致的部分）。
    ///
    /// The write counterpart is [`write_metadata`](Self::write_metadata); the two must
    /// stay in sync field by field.
    fn copy_avframe_meta(&mut self, frame: &AVFrame) {
        self.key_frame = frame.flags & ffi::AV_FRAME_FLAG_KEY as i32 != 0;
        self.flags = frame.flags;
        self.quality = frame.quality;
        self.repeat_pict = frame.repeat_pict;
        self.colorspace = frame.colorspace;
        self.color_primaries = frame.color_primaries;
        self.color_trc = frame.color_trc;
        self.color_range = frame.color_range;
        self.chroma_location = frame.chroma_location;
        self.sample_aspect_ratio = frame.sample_aspect_ratio;
        self.crop_top = frame.crop_top;
        self.crop_bottom = frame.crop_bottom;
        self.crop_left = frame.crop_left;
        self.crop_right = frame.crop_right;
        #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
        {
            self.alpha_mode = frame.alpha_mode;
        }
        self.best_effort_timestamp = frame.best_effort_timestamp;
        self.decode_error_flags = frame.decode_error_flags;
        // SAFETY: `frame.metadata` is valid for as long as `frame` is borrowed.
        self.metadata = unsafe { Metadata::from_raw_dict(frame.metadata) };
        self.side_data = side_data_from_avframe(frame);
    }

    /// 将 `MediaFrame` 的元数据字段写回 `AVFrame`，与 [`copy_avframe_meta`](Self::copy_avframe_meta)
    /// 构成对称的读写对——新增字段时两处需同步维护。
    ///
    /// 相比 rsmpeg 的 setter，这里通过 owned 句柄的裸指针写入 setter 无法覆盖的字段
    /// （`flags`/`quality`/`repeat_pict`/色彩元数据/`pkt_dts` 等），生命周期安全。
    fn write_metadata(&self, frame: &mut AVFrame) {
        unsafe {
            let raw = frame.as_mut_ptr();
            (*raw).flags = self.flags;
            (*raw).quality = self.quality;
            (*raw).repeat_pict = self.repeat_pict;
            (*raw).colorspace = self.colorspace;
            (*raw).color_primaries = self.color_primaries;
            (*raw).color_trc = self.color_trc;
            (*raw).color_range = self.color_range;
            (*raw).chroma_location = self.chroma_location;
            (*raw).sample_aspect_ratio = self.sample_aspect_ratio;
            (*raw).crop_top = self.crop_top;
            (*raw).crop_bottom = self.crop_bottom;
            (*raw).crop_left = self.crop_left;
            (*raw).crop_right = self.crop_right;
            #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
            {
                (*raw).alpha_mode = self.alpha_mode;
            }
            // `duration` is the canonical field; `pkt_duration` is only a mirror, see
            // the field docs.
            (*raw).duration = self.duration;
            // `AVFrame::new()` leaves `pkt_dts` at `AV_NOPTS_VALUE`, so this is the
            // only place the frame's decode timestamp can come from.
            (*raw).pkt_dts = self.pkt_dts;
            (*raw).best_effort_timestamp = self.best_effort_timestamp;
            (*raw).decode_error_flags = self.decode_error_flags;
            // SAFETY: `(*raw).metadata` is a live dictionary slot owned by `frame`.
            self.metadata.write_into_raw_dict(&mut (*raw).metadata);
        }
        write_side_data(frame, &self.side_data);
    }

    /// 仅拷贝标量元数据（不含大块 `data`），用于产出基于当前帧元数据的转换结果。
    /// 避免 `self.clone()` 连同一整块图像/音频缓冲一起复制，减少转换的中间分配。
    fn meta_only(&self) -> Self {
        Self {
            pts: self.pts,
            pkt_dts: self.pkt_dts,
            duration: self.duration,
            format: self.format, // 调用方随后按需覆盖
            data: Default::default(),
            time_base: self.time_base,
            media_type: self.media_type,
            width: self.width,
            height: self.height,
            pict_type: self.pict_type,
            sample_rate: self.sample_rate,
            nb_samples: self.nb_samples,
            nb_channels: self.nb_channels,
            key_frame: self.key_frame,
            flags: self.flags,
            quality: self.quality,
            repeat_pict: self.repeat_pict,
            colorspace: self.colorspace,
            color_primaries: self.color_primaries,
            color_trc: self.color_trc,
            color_range: self.color_range,
            chroma_location: self.chroma_location,
            sample_aspect_ratio: self.sample_aspect_ratio,
            crop_top: self.crop_top,
            crop_bottom: self.crop_bottom,
            crop_left: self.crop_left,
            crop_right: self.crop_right,
            #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
            alpha_mode: self.alpha_mode,
            pkt_duration: self.pkt_duration,
            best_effort_timestamp: self.best_effort_timestamp,
            decode_error_flags: self.decode_error_flags,
            metadata: self.metadata.clone(),
            side_data: self.side_data.clone(),
        }
    }

    /// 转换为新AVFrame
    pub fn to_avframe(&self) -> Result<AVFrame> {
        let mut frame = AVFrame::new();
        if self.media_type == MediaType::VIDEO {
            // video frame
            frame.set_width(self.width as i32);
            frame.set_height(self.height as i32);
            frame.set_format(i32::from(self.format));
            frame.set_pict_type(self.pict_type);
            fill_video_data(&mut frame, &self.data)?;
        } else {
            // audio frame
            frame.set_format(i32::from(self.format));
            frame.set_nb_samples(self.nb_samples as i32);
            frame.set_sample_rate(self.sample_rate as i32);
            frame.set_ch_layout(
                AVChannelLayout::from_nb_channels(self.nb_channels as i32).into_inner(),
            );
            fill_audio_data(&mut frame, &self.data)?;
        };

        // 写回 setter 无法覆盖的元数据（与 copy_avframe_meta 对称）。
        self.write_metadata(&mut frame);
        frame.set_pts(self.pts);

        // 统一口径：无论视频/音频都优先使用 `time_base`；仅在未设置时才按
        // 音频采样率推导 `1/sample_rate`，与 `from_avframe` 的推断规则一致。
        let time_base = if self.time_base.num > 0 && self.time_base.den > 0 {
            self.time_base
        } else if self.media_type == MediaType::AUDIO && self.sample_rate > 0 {
            time::new_rational(1, self.sample_rate as i32)
        } else {
            self.time_base
        };
        frame.set_time_base(time_base);
        Ok(frame)
    }

    ////////////////////////////////////////////////////////////////////////////////////
    ///////////////////////////// convert //////////////////////////////////////////////
    ////////////////////////////////////////////////////////////////////////////////////

    /// 校验当前帧为视频帧，且像素格式为 `expected`；否则返回可读的错误信息。
    fn check_video_format(&self, expected: FrameFormat, expected_desc: &str) -> Result<()> {
        if self.media_type != MediaType::VIDEO {
            return Err(RsmediaError::custom("Only video frames are supported"));
        }
        if self.format != expected {
            let got = match self.format {
                FrameFormat::Pixel(p) => p.get_pix_fmt_name(),
                FrameFormat::Sample(_) => "<audio format>",
            };
            return Err(RsmediaError::custom(format!(
                "Expected {expected_desc} format, got {got}"
            )));
        }
        Ok(())
    }

    /// 校验当前帧是否为可转换的 RGB24 视频帧。
    fn check_rgb24_supported(&self) -> Result<()> {
        self.check_video_format(FrameFormat::Pixel(PixelFormat::RGB24), "RGB24")
    }

    /// 选择标准色彩矩阵：优先读取帧携带的 `colorspace` 元数据，未标记时
    /// 回退到按分辨率启发式（SD -> BT.601 / HD -> BT.709 / UHD -> BT.2020）
    fn auto_colorspace(&self) -> YuvStandardMatrix {
        let colorspace = self.colorspace;
        if colorspace == ffi::AVCOL_SPC_BT709 {
            return YuvStandardMatrix::Bt709;
        }
        if colorspace == ffi::AVCOL_SPC_BT2020_NCL || colorspace == ffi::AVCOL_SPC_BT2020_CL {
            return YuvStandardMatrix::Bt2020;
        }
        // BT470BG/SMPTE170M/BT470_6/FCC/SMPTE240M 等标准 601 或 near-601
        if colorspace == ffi::AVCOL_SPC_BT470BG
            || colorspace == ffi::AVCOL_SPC_SMPTE170M
            || colorspace == ffi::AVCOL_SPC_FCC
            || colorspace == ffi::AVCOL_SPC_SMPTE240M
        {
            return YuvStandardMatrix::Bt601;
        }

        let _ = colorspace; // UNSPECIFIED / RGB / YCoCg / ICTCP 等无 YUV 矩阵意义，回落分辨率
        let height = self.height;
        if height < 720 {
            YuvStandardMatrix::Bt601
        } else if height < 1080 {
            YuvStandardMatrix::Bt709
        } else {
            YuvStandardMatrix::Bt2020
        }
    }

    /// 将 `(height, width, 3)` 的 RGB 数据展平为 `[R,G,B]` 字节序列。
    /// 连续布局直接批量拷贝，非连续布局逐像素回退，保证两者产物一致。
    fn to_rgb_bytes(&self) -> Vec<u8> {
        let (height, width, _) = self.data.dim();
        let mut rgb = Vec::with_capacity(width * height * 3);
        if let Some(slice) = self.data.as_standard_layout().as_slice() {
            rgb.extend(
                slice
                    .iter()
                    .map(|&v| num_traits::cast::<T, u8>(v).unwrap_or(0)),
            );
        } else {
            for y in 0..height {
                for x in 0..width {
                    rgb.extend_from_slice(&[
                        num_traits::cast::<T, u8>(self.data[[y, x, 0]]).unwrap_or(0),
                        num_traits::cast::<T, u8>(self.data[[y, x, 1]]).unwrap_or(0),
                        num_traits::cast::<T, u8>(self.data[[y, x, 2]]).unwrap_or(0),
                    ]);
                }
            }
        }
        rgb
    }

    pub fn convert_rgb_to_yuv(&self) -> Result<Self> {
        self.check_rgb24_supported()?;
        self.convert_rgb_to_yuv_with_matrix(self.auto_colorspace())
    }

    /// 用**指定**的色彩矩阵将 RGB24 帧转换为 YUV420P。
    ///
    /// 相比自动按分辨率判断的 [`convert_rgb_to_yuv`](Self::convert_rgb_to_yuv)，
    /// 此方法允许用户显式选择 BT.601 / BT.709 / BT.2020，用于需要精确控制
    /// 色彩矩阵的专业场景（例如与源视频的色彩标准保持一致）。
    ///
    /// # Examples
    ///
    /// ```
    /// # use rsmedia::MediaFrame;
    /// # fn d(mut f: MediaFrame<u8>) -> rsmedia::Result<()> {
    /// let yuv = f.convert_rgb_to_yuv_with_matrix(yuv::YuvStandardMatrix::Bt709)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn convert_rgb_to_yuv_with_matrix(&self, colorspace: YuvStandardMatrix) -> Result<Self> {
        self.check_rgb24_supported()?;

        // YUV420P 是 2x2 色度下采样，宽高必须为偶数。
        // 8bit / 16bit 两条路径在此处统一校验，避免各自分支产生不一致的奇尺寸语义。
        if !self.width.is_multiple_of(2) || !self.height.is_multiple_of(2) {
            return Err(RsmediaError::custom(format!(
                "RGB24 -> YUV420P requires even dimensions, got {}x{}",
                self.width, self.height
            )));
        }

        // 大精度（16bit+）路径：u8 的 `to_rgb_bytes` 会截断低位，改用 u16 满幅计算。
        if std::mem::size_of::<T>() > 1 {
            let (height, width, _) = self.data.dim();
            let mut rgb16 = Vec::with_capacity(self.data.len());
            for v in self.data.iter() {
                rgb16.push(num_traits::cast::<T, u16>(*v).unwrap_or(0));
            }
            let yuv16 = rgb_to_yuv420_16bit(&rgb16, width, height, colorspace)?;
            let mut res = self.meta_only();
            res.format = FrameFormat::Pixel(PixelFormat::YUV420P);
            // u16 平面转回 T（u16/u32/f32 等），保持与调用方类型一致
            res.data = ndarray::Array3::from_shape_fn((height, width, 3), |idx| {
                let v = yuv16[idx];
                num_traits::cast::<u16, T>(v).unwrap_or(T::zero())
            });
            return Ok(res);
        }

        let height = self.height;
        let width = self.width;

        // 1. RGB数据准备，保持完整的色彩范围
        let rgb_bytes = self.to_rgb_bytes();

        // 2. 创建YUV平面
        let y_stride = width;
        let uv_stride = width / 2;
        let mut y_plane = vec![0u8; y_stride * height];
        let mut u_plane = vec![0u8; uv_stride * (height / 2)];
        let mut v_plane = vec![0u8; uv_stride * (height / 2)];

        // 3. 创建YUV图像
        let mut planar_image = YuvPlanarImageMut {
            y_plane: BufferStoreMut::Borrowed(&mut y_plane),
            y_stride: y_stride as u32,
            u_plane: BufferStoreMut::Borrowed(&mut u_plane),
            u_stride: uv_stride as u32,
            v_plane: BufferStoreMut::Borrowed(&mut v_plane),
            v_stride: uv_stride as u32,
            width: width as u32,
            height: height as u32,
        };

        // 4. 使用指定的色彩矩阵进行转换（Full Range）
        yuv::rgb_to_yuv420(
            &mut planar_image,
            &rgb_bytes,
            (width * 3) as u32,
            YuvRange::Full,
            colorspace,
            YuvConversionMode::Professional,
        )
        .map_err(|e| RsmediaError::custom(format!("convert rgb24 to yuv420p error:{e}")))?;

        // 6. 构建YUV数据
        let mut yuv_data = ndarray::Array3::<T>::zeros((height, width, 3));

        // 复制Y平面
        for h in 0..height {
            let y_offset = h * y_stride;
            for w in 0..width {
                yuv_data[[h, w, 0]] = num_traits::cast(y_plane[y_offset + w]).unwrap_or(T::zero());
            }
        }

        // 复制UV平面（2x2 块内 4 个 Y 像素共享同一 UV 值；宽高已保证为偶数）
        for h_uv in 0..height / 2 {
            let u_offset = h_uv * uv_stride;
            let h_y = h_uv * 2;

            for w_uv in 0..width / 2 {
                let u_val = num_traits::cast(u_plane[u_offset + w_uv]).unwrap_or(T::zero());
                let v_val = num_traits::cast(v_plane[u_offset + w_uv]).unwrap_or(T::zero());
                let w_y = w_uv * 2;

                yuv_data[[h_y, w_y, 1]] = u_val;
                yuv_data[[h_y, w_y, 2]] = v_val;
                yuv_data[[h_y, w_y + 1, 1]] = u_val;
                yuv_data[[h_y, w_y + 1, 2]] = v_val;
                yuv_data[[h_y + 1, w_y, 1]] = u_val;
                yuv_data[[h_y + 1, w_y, 2]] = v_val;
                yuv_data[[h_y + 1, w_y + 1, 1]] = u_val;
                yuv_data[[h_y + 1, w_y + 1, 2]] = v_val;
            }
        }

        let mut res = self.meta_only();
        res.format = FrameFormat::Pixel(PixelFormat::YUV420P);
        res.data = yuv_data;
        Ok(res)
    }

    pub fn convert_yuv_to_rgb(&self) -> Result<Self> {
        self.check_video_format(FrameFormat::Pixel(PixelFormat::YUV420P), "YUV420P")?;

        // 大精度（16bit+）路径：直接以 u16 满幅做 YUV->RGB，避免经 u8 截断。
        if std::mem::size_of::<T>() > 1 {
            let (_height, width, _) = self.data.dim();
            let colorspace = self.auto_colorspace();
            let yuv16 = ndarray::Array3::from_shape_fn(self.data.dim(), |idx| {
                num_traits::cast::<T, u16>(self.data[idx]).unwrap_or(0)
            });
            let rgb16 = yuv420_to_rgb_16bit(&yuv16, colorspace)?;
            let data = ndarray::Array3::from_shape_fn(self.data.dim(), |(h, w, c)| {
                num_traits::cast::<u16, T>(rgb16[(h * width + w) * 3 + c]).unwrap_or(T::zero())
            });
            let mut res = self.meta_only();
            res.format = FrameFormat::Pixel(PixelFormat::RGB24);
            res.data = data;
            return Ok(res);
        }

        let height = self.height;
        let width = self.width;

        // 1. 准备YUV数据
        let y_stride = width;
        let uv_stride = width / 2;
        let mut y_plane = vec![0u8; y_stride * height];
        let mut u_plane = vec![0u8; uv_stride * (height / 2)];
        let mut v_plane = vec![0u8; uv_stride * (height / 2)];

        // 复制Y平面数据
        for h in 0..height {
            for w in 0..width {
                y_plane[h * y_stride + w] = num_traits::cast(self.data[[h, w, 0]]).unwrap_or(0);
            }
        }

        // 复制UV平面数据
        for h in 0..height / 2 {
            for w in 0..width / 2 {
                let pos = h * uv_stride + w;
                u_plane[pos] = num_traits::cast(self.data[[h * 2, w * 2, 1]]).unwrap_or(128);
                v_plane[pos] = num_traits::cast(self.data[[h * 2, w * 2, 2]]).unwrap_or(128);
            }
        }

        // 2. 创建YUV图像
        let planar_image = YuvPlanarImage {
            y_plane: &y_plane,
            y_stride: y_stride as u32,
            u_plane: &u_plane,
            u_stride: uv_stride as u32,
            v_plane: &v_plane,
            v_stride: uv_stride as u32,
            width: width as u32,
            height: height as u32,
        };

        // 3. 准备RGB输出
        let mut rgb_bytes = vec![0u8; width * height * 3];

        // 4. 选择转换参数
        let colorspace = self.auto_colorspace();

        // 5. 使用Full Range进行转换
        yuv::yuv420_to_rgb(
            &planar_image,
            &mut rgb_bytes,
            (width * 3) as u32,
            YuvRange::Full,
            colorspace,
        )
        .map_err(|e| RsmediaError::custom(format!("convert yuv420p to rgb24 error:{e}")))?;

        // 6. 构建RGB数据
        let mut rgb_data = ndarray::Array3::<T>::zeros((height, width, 3));
        for h in 0..height {
            for w in 0..width {
                let idx = (h * width + w) * 3;
                rgb_data[[h, w, 0]] = num_traits::cast(rgb_bytes[idx]).unwrap_or(T::zero());
                rgb_data[[h, w, 1]] = num_traits::cast(rgb_bytes[idx + 1]).unwrap_or(T::zero());
                rgb_data[[h, w, 2]] = num_traits::cast(rgb_bytes[idx + 2]).unwrap_or(T::zero());
            }
        }

        let mut res = self.clone();
        res.format = FrameFormat::Pixel(PixelFormat::RGB24);
        res.data = rgb_data;
        Ok(res)
    }
}

impl MediaFrame<u8> {
    /// 将 RGB24 视频帧转换为 `image::DynamicImage`。
    ///
    /// 仅支持 [`PixelFormat::RGB24`] 视频帧；其他格式或媒体类型返回错误。
    pub fn to_dynamic_image(&self) -> Result<image::DynamicImage> {
        self.check_rgb24_supported()?;
        let (_, width, _) = self.data.dim();
        let rgb = self.to_rgb_bytes();
        image::RgbImage::from_raw(width as u32, self.height as u32, rgb)
            .map(image::DynamicImage::ImageRgb8)
            .ok_or_else(|| RsmediaError::custom("Failed to build image from RGB24 data"))
    }

    /// 从 `image::DynamicImage` 创建 RGB24 视频帧。
    ///
    /// 任意色彩模式（RGB / RGBA / 灰度等）都会统一转为 RGB8。
    pub fn from_dynamic_image(
        img: &image::DynamicImage,
        time_base: ffi::AVRational,
    ) -> Result<Self> {
        let rgb = img.to_rgb8();
        let (width, height) = rgb.dimensions();
        let data =
            ndarray::Array3::from_shape_vec((height as usize, width as usize, 3), rgb.into_raw())
                .map_err(|e| {
                RsmediaError::custom(format!("Failed to build ndarray from image: {e}"))
            })?;
        Self::new_video(
            width as usize,
            height as usize,
            PixelFormat::RGB24,
            time_base,
            data,
        )
    }
}

/// 由色彩矩阵返回色相系数三元组 (Kr, Kg, Kb)。
/// 供 u16 大精度 RGB<->YUV 路径使用（`yuv` crate 的公开转换仅支持 8bit）。
fn yuv_primaries(matrix: YuvStandardMatrix) -> (f32, f32, f32) {
    match matrix {
        YuvStandardMatrix::Bt601 => (0.299, 0.587, 0.114),
        YuvStandardMatrix::Bt709 => (0.2126, 0.7152, 0.0722),
        YuvStandardMatrix::Bt2020 => (0.2627, 0.6780, 0.0593),
        YuvStandardMatrix::Smpte240 => (0.212, 0.701, 0.087),
        YuvStandardMatrix::Bt470_6 => (0.299, 0.587, 0.114),
        YuvStandardMatrix::Fcc => (0.310, 0.589, 0.101),
        YuvStandardMatrix::Custom(kr, kb) => (kr, 1.0 - kr - kb, kb),
    }
}

/// 16bit RGB24 -> YUV420P（Full Range，2x2 间组抽样）。
///
/// `yuv` crate 的 `rgb_to_yuv420` 仅接受 u8，会对 u16 截断；此实现将色域矩阵在
/// f32 中精确计算并按 16bit 保存，保留低 8 位精度（支持 10-bit/12-bit 内容）。
fn rgb_to_yuv420_16bit(
    rgb16: &[u16],
    width: usize,
    height: usize,
    matrix: YuvStandardMatrix,
) -> Result<ndarray::Array3<u16>> {
    use ndarray::Array3;
    if rgb16.len() < width * height * 3 {
        return Err(RsmediaError::custom(
            "RGB data too short for 16-bit conversion",
        ));
    }
    if !width.is_multiple_of(2) || !height.is_multiple_of(2) {
        return Err(RsmediaError::custom(format!(
            "YUV420P requires even dimensions, got {width}x{height}"
        )));
    }

    let (kr, _kg, kb) = yuv_primaries(matrix);
    let kg = 1.0 - kr - kb;
    let denom_y = 2.0 * (1.0 - kb);
    let denom_v = 2.0 * (1.0 - kr);
    let scale = 65535_f32;
    let center = scale / 2.0;

    // 先在 1x1 内计算 Y、半尺寸 U/V，再按块复制以保持代表值语义。
    let mut y_plane = vec![0u16; width * height];
    let (uv_w, uv_h) = (width / 2, height / 2);
    let mut u_plane = vec![0u16; uv_w * uv_h];
    let mut v_plane = vec![0u16; uv_w * uv_h];

    for h in 0..height {
        for w in 0..width {
            let i = (h * width + w) * 3;
            let r = rgb16[i] as f32 / scale;
            let g = rgb16[i + 1] as f32 / scale;
            let b = rgb16[i + 2] as f32 / scale;
            let yv = kr.mul_add(r, kg.mul_add(g, kb * b));
            y_plane[h * width + w] = (yv * scale).round() as u16;
            if h % 2 == 0 && w % 2 == 0 {
                let uv_idx = (h / 2) * uv_w + (w / 2);
                u_plane[uv_idx] = (((b - yv) / denom_y) * scale + center).round() as u16;
                v_plane[uv_idx] = (((r - yv) / denom_v) * scale + center).round() as u16;
            }
        }
    }

    // 重建 [H, W, 3] ndarray，Y 满分辨率、U/V 为 2x2 块代表值
    let mut out = Array3::<u16>::zeros((height, width, 3));
    for h in 0..height {
        for w in 0..width {
            out[[h, w, 0]] = y_plane[h * width + w];
            out[[h, w, 1]] = u_plane[(h / 2) * uv_w + (w / 2)];
            out[[h, w, 2]] = v_plane[(h / 2) * uv_w + (w / 2)];
        }
    }
    Ok(out)
}

/// 将 16-bit YUV420P（[H,W,3] 的 u16 数据）转换为 RGB24 满幅平面。
fn yuv420_to_rgb_16bit(yuv: &ndarray::Array3<u16>, matrix: YuvStandardMatrix) -> Result<Vec<u16>> {
    let height = yuv.shape()[0];
    let width = yuv.shape()[1];
    let (kr, _kg, kb) = yuv_primaries(matrix);
    let scale = 65535_f32;
    let center = scale / 2.0;
    let mut out = Vec::with_capacity(width * height * 3);

    for h in 0..height {
        for w in 0..width {
            let y = yuv[[h, w, 0]] as f32 / scale;
            let cb = (yuv[[h, w, 1]] as f32 - center) / scale;
            let cr = (yuv[[h, w, 2]] as f32 - center) / scale;
            let r = y + 2.0 * (1.0 - kr) * cr;
            let b = y + 2.0 * (1.0 - kb) * cb;
            let g = y - kr * r - kb * b;
            out.push((r.clamp(0.0, 1.0) * scale).round() as u16);
            out.push((g.clamp(0.0, 1.0) * scale).round() as u16);
            out.push((b.clamp(0.0, 1.0) * scale).round() as u16);
        }
    }
    Ok(out)
}

/// 验证帧格式和类型大小的匹配关系
fn validate_format_type_size<T>(format: i32, expected_size: usize) -> Result<()> {
    let type_size = std::mem::size_of::<T>();
    if type_size != expected_size {
        return Err(RsmediaError::custom(format!(
            "format:{format}, expected {expected_size}, got {type_size}"
        )));
    }
    Ok(())
}

/// 返回指定采样格式的单样本字节数；不支持的格式返回错误。
fn audio_sample_size(format: i32) -> Result<usize> {
    match format {
        ffi::AV_SAMPLE_FMT_U8 | ffi::AV_SAMPLE_FMT_U8P => Ok(1),
        ffi::AV_SAMPLE_FMT_S16 | ffi::AV_SAMPLE_FMT_S16P => Ok(2),
        ffi::AV_SAMPLE_FMT_S32 | ffi::AV_SAMPLE_FMT_S32P => Ok(4),
        ffi::AV_SAMPLE_FMT_FLT | ffi::AV_SAMPLE_FMT_FLTP => Ok(4),
        ffi::AV_SAMPLE_FMT_DBL | ffi::AV_SAMPLE_FMT_DBLP => Ok(8),
        ffi::AV_SAMPLE_FMT_S64 | ffi::AV_SAMPLE_FMT_S64P => Ok(8),
        _ => Err(RsmediaError::custom("Unsupported sample format")),
    }
}

/// Copies `AVFrame.side_data` into owned [`FrameSideData`] entries.
fn side_data_from_avframe(frame: &AVFrame) -> Vec<FrameSideData> {
    let count = frame.nb_side_data.max(0) as usize;
    if count == 0 || frame.side_data.is_null() {
        return Vec::new();
    }
    // SAFETY: FFmpeg guarantees `side_data` points at `nb_side_data` valid pointers.
    let entries = unsafe { std::slice::from_raw_parts(frame.side_data, count) };
    entries
        .iter()
        .filter_map(|entry| {
            let entry = unsafe { entry.as_ref()? };
            let mut data = vec![0u8; entry.size];
            if entry.size > 0 && !entry.data.is_null() {
                // SAFETY: the side-data payload is `size` bytes long.
                unsafe {
                    std::ptr::copy_nonoverlapping(entry.data, data.as_mut_ptr(), entry.size);
                }
            }
            Some(FrameSideData {
                type_: entry.type_,
                data,
                // SAFETY: `entry.metadata` is valid for as long as the borrowed
                // `AVFrameSideData` (i.e. this call).
                metadata: unsafe { Metadata::from_raw_dict(entry.metadata) },
            })
        })
        .collect()
}

/// Writes owned [`FrameSideData`] entries into an `AVFrame`.
///
/// The target frame is always freshly allocated by [`MediaFrame::to_avframe`], so it
/// carries no pre-existing side data and needs no removal pass.
fn write_side_data(frame: &mut AVFrame, entries: &[FrameSideData]) {
    for entry in entries {
        if entry.data.is_empty() {
            continue;
        }
        // SAFETY: `frame` is a live AVFrame we own and the frame has no side data yet,
        // so this neither aliases nor overwrites existing entries.
        let raw = unsafe {
            ffi::av_frame_new_side_data(frame.as_mut_ptr(), entry.type_, entry.data.len())
        };
        if raw.is_null() {
            log::warn!(
                "Failed to allocate side data of type {} ({} bytes)",
                entry.type_,
                entry.data.len()
            );
            continue;
        }
        // SAFETY: `av_frame_new_side_data` allocated exactly `entry.data.len()` bytes,
        // and `(*raw).metadata` is a live dictionary slot owned by that entry.
        unsafe {
            std::ptr::copy_nonoverlapping(entry.data.as_ptr(), (*raw).data, entry.data.len());
            entry.metadata.write_into_raw_dict(&mut (*raw).metadata);
        }
    }
}

/// packed（单平面、8bit/分量）像素格式的每像素分量数（ndarray 的 C 维度）。
///
/// 语义与判定见 [`PixelFormat::packed_channels`]（单一事实来源）；
/// 返回 `None` 表示非 packed 8bit 格式（planar / 半平面 / 位流 / 硬件格式等），
/// 需走 YUV420P 专用分支或经 swscale 转换后再入 ndarray。
fn packed_channels(format: i32) -> Option<usize> {
    PixelFormat::from(format).packed_channels()
}

/// ndarray => AVFrame:
/// 对 U 和 V 进行下采样，恢复到 YUV420P 格式所需的较低分辨率
/// 减少数据的采样率，降低分辨率或数据量。
/// 在 YUV 视频格式中，通常对色度信息（U 和 V 分量）进行下采样，因为人眼对亮度信息比色度信息更敏感。
fn fill_video_data<T>(frame: &mut AVFrame, data: &ndarray::Array3<T>) -> Result<()>
where
    T: MediaFrameType,
{
    let (height, width, channel) = data.dim();

    // packed 8bit 格式：channel 必须与该格式的每像素分量数一致
    let expected_channels = packed_channels(frame.format);
    if let Some(ch) = expected_channels {
        if channel != ch {
            return Err(RsmediaError::custom(format!(
                "Pixel format expects {ch} channels, data has {channel}"
            )));
        }
    } else if channel != 3 {
        // YUV420P 专用分支以 [H, W, 3] 存储（U/V 为 2x2 块代表值）
        return Err(RsmediaError::custom("Only support 3-channel video"));
    }

    // packed 8bit / YUV420P 的样本均为 8bit；类型大小不匹配时，下面的 to_u8() 会越界/溢出。
    if matches!(
        frame.format,
        ffi::AV_PIX_FMT_RGB24 | ffi::AV_PIX_FMT_YUV420P
    ) || expected_channels.is_some()
    {
        validate_format_type_size::<T>(frame.format, 1)?;
    }

    // 分配视频缓冲区
    frame
        .alloc_buffer()
        .context("Failed to allocate video buffer")?;

    if let Some(ch) = expected_channels {
        // packed 单平面通用填充：按行写入，正确处理 linesize 对齐 padding。
        // 说明：`fill_frame_from_buffer` 期望的源布局来自 `av_image_fill_arrays(align=1)`，
        // 对偶数/奇偶数宽度 + YUV422 类的 packed 格式，其递推行宽可能与我们压出的紧凑字节
        // 不一致，故这里退化为按目标 `frame.linesize[0]` 逐行拷贝，语义最直接、可预测。
        let dst = frame.data[0];
        let dst_linesize = frame.linesize[0] as usize;
        let row_bytes = width * ch;
        let expected = width * height * ch;
        if data.is_standard_layout()
            && let Some(slice) = data.as_slice()
        {
            if slice.len() < expected {
                return Err(RsmediaError::custom(format!(
                    "Insufficient packed data: {} < {expected}",
                    slice.len()
                )));
            }
            unsafe {
                for y in 0..height {
                    let src_row = (slice.as_ptr() as *const u8).add(y * row_bytes);
                    let dst_row = dst.add(y * dst_linesize);
                    std::ptr::copy_nonoverlapping(src_row, dst_row, row_bytes);
                }
            }
            return Ok(());
        }
        // 非连续布局：逐元素按行写入目标带 padding 的行
        for y in 0..height {
            let dst_row = unsafe { dst.add(y * dst_linesize) };
            for x in 0..width {
                for c in 0..ch {
                    unsafe {
                        *dst_row.add(x * ch + c) = data[[y, x, c]].to_u8().unwrap();
                    }
                }
            }
        }
        return Ok(());
    }

    match frame.format {
        ffi::AV_PIX_FMT_YUV420P => {
            if width % 2 != 0 || height % 2 != 0 {
                return Err(RsmediaError::custom(format!(
                    "YUV420P requires even dimensions, got {width}x{height}"
                )));
            }
            // 提取 Y 平面数据
            let mut y_data = Vec::with_capacity(width * height);
            for y in 0..height {
                for x in 0..width {
                    y_data.push(data[[y, x, 0]].to_u8().unwrap());
                }
            }

            // U和V平面 - 从上采样数据中提取
            let uv_height = height / 2;
            let uv_width = width / 2;
            let mut u_data = Vec::with_capacity(uv_width * uv_height);
            let mut v_data = Vec::with_capacity(uv_width * uv_height);

            for y in 0..uv_height {
                for x in 0..uv_width {
                    // 注意： U 和 V 数据已经是下采样后的尺寸
                    u_data.push(data[[y * 2, x * 2, 1]].to_u8().unwrap());
                    v_data.push(data[[y * 2, x * 2, 2]].to_u8().unwrap());
                }
            }

            // 填充 Y、U、V 平面
            imgutils::fill_plane_from_buffer(frame, 0, y_data, width)?;
            imgutils::fill_plane_from_buffer(frame, 1, u_data, uv_width)?;
            imgutils::fill_plane_from_buffer(frame, 2, v_data, uv_width)?;

            Ok(())
        }

        _ => Err(RsmediaError::custom(format!(
            "Unsupported to_frame video format: {frame:?}"
        ))),
    }
}

/// 填充音频数据到AVFrame
fn fill_audio_data<T>(frame: &mut AVFrame, data: &ndarray::Array3<T>) -> Result<()>
where
    T: MediaFrameType,
{
    let (frames, samples, channels) = data.dim();
    if frames != 1 {
        return Err(RsmediaError::custom("Batch audio not supported"));
    }

    // 校验 T 的大小与帧实际采样格式元素大小一致，避免因大小不符造成越界写
    let sample_size = audio_sample_size(frame.format)?;
    validate_format_type_size::<T>(frame.format, sample_size)?;

    // 分配视频缓冲区
    frame
        .alloc_buffer()
        .context("Failed to allocate audio buffer")?;

    if let Some(buffer) = data.as_standard_layout().as_slice() {
        if buffer.len() != samples * channels {
            return Err(RsmediaError::custom(format!(
                "Audio data length {} != samples * channels {}*{}",
                buffer.len(),
                samples,
                channels
            )));
        }
        unsafe {
            if SampleFormat::from(frame.format).is_planar() {
                // 平面布局：每个声道单独存储
                for ch in 0..channels {
                    let dst = std::slice::from_raw_parts_mut(frame.data[ch] as *mut T, samples);
                    for s in 0..samples {
                        dst[s] = buffer[s * channels + ch];
                    }
                }
            } else {
                // 交错布局：单块内存存储所有声道
                let dst =
                    std::slice::from_raw_parts_mut(frame.data[0] as *mut T, samples * channels);
                dst.copy_from_slice(buffer);
            }
        }
        Ok(())
    } else {
        Err(RsmediaError::custom("Non-contiguous audio data"))
    }
}

/// AVFrame => ndarray
/// 对 U 和 V 进行上采样，使其与 Y 具有相同分辨率
/// 增加数据的采样率，提高分辨率。
/// 在处理已经下采样的数据时，有时需要将其恢复到原始分辨率。
fn video_data<T>(frame: &AVFrame) -> Result<ndarray::Array3<T>>
where
    T: MediaFrameType,
{
    let (height, width) = (frame.height as usize, frame.width as usize);

    // packed 单平面通用读取：[H, W, C]，复用 imgutils 平面读取（去行 padding）
    //（validate 已确保 T 为 8bit，packed 单平面字节数恰为 width*height*channels）
    if let Some(ch) = packed_channels(frame.format) {
        validate_format_type_size::<T>(frame.format, 1)?;

        let buffer = imgutils::get_plane_buffer(frame, 0)?;
        let expected = width * height * ch;
        if buffer.len() < expected {
            return Err(RsmediaError::custom(format!(
                "Insufficient packed buffer: {} < {expected}",
                buffer.len()
            )));
        }
        let mut data_t = Vec::with_capacity(expected);
        for &b in buffer.iter().take(expected) {
            data_t.push(num_traits::cast::<u8, T>(b).unwrap_or(T::zero()));
        }
        return ndarray::Array3::from_shape_vec((height, width, ch), data_t)
            .map_err(|_| RsmediaError::custom("Packed data shape mismatch"));
    }

    match frame.format {
        ffi::AV_PIX_FMT_YUV420P => {
            validate_format_type_size::<T>(frame.format, 1)?;
            // YUV420P 的色度是亮度采样的 1/4，宽高必须为偶数，否则 UV 平面无法完整上采样
            if width % 2 != 0 || height % 2 != 0 {
                return Err(RsmediaError::custom(format!(
                    "YUV420P requires even dimensions, got {width}x{height}"
                )));
            }

            let mut array = ndarray::Array3::<T>::default((height, width, 3));

            // 逐平面取出去 padding 的紧凑数据，再组织为 [H, W, 3]
            let y_plane = imgutils::get_plane_buffer(frame, 0)?;
            let u_plane = imgutils::get_plane_buffer(frame, 1)?;
            let v_plane = imgutils::get_plane_buffer(frame, 2)?;

            let uv_h = height / 2;
            let uv_w = width / 2;
            if y_plane.len() < height * width
                || u_plane.len() < uv_h * uv_w
                || v_plane.len() < uv_h * uv_w
            {
                return Err(RsmediaError::custom("YUV420P plane buffer too small"));
            }

            // 复制 Y 平面（满分辨率）
            for y in 0..height {
                for x in 0..width {
                    array[[y, x, 0]] =
                        num_traits::cast::<u8, T>(y_plane[y * width + x]).unwrap_or(T::zero());
                }
            }

            // 复制 UV 平面并 2x2 上采样（每点复制到 4 个像素）
            for y in 0..uv_h {
                for x in 0..uv_w {
                    let u = num_traits::cast::<u8, T>(u_plane[y * uv_w + x]).unwrap_or(T::zero());
                    let v = num_traits::cast::<u8, T>(v_plane[y * uv_w + x]).unwrap_or(T::zero());
                    for (dy, dx) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                        array[[y * 2 + dy, x * 2 + dx, 1]] = u;
                        array[[y * 2 + dy, x * 2 + dx, 2]] = v;
                    }
                }
            }

            Ok(array)
        }

        _ => Err(RsmediaError::custom(format!(
            "Unsupported from_frame video format: {frame:?}"
        ))),
    }
}

/// 音频数据处理
fn audio_data<T>(frame: &AVFrame) -> Result<ndarray::Array3<T>>
where
    T: MediaFrameType,
{
    // 类型大小验证
    let sample_size = audio_sample_size(frame.format)?;
    validate_format_type_size::<T>(frame.format, sample_size)?;

    // check
    if frame.data[0].is_null() {
        return Err(RsmediaError::custom("Frame data is null"));
    }

    let channels = frame.ch_layout.nb_channels as usize;
    let samples = frame.nb_samples as usize;
    // 按 [1, samples, channels]（samples 外层、channels 内层）组织交错顺序
    let mut buffer = vec![T::zero(); samples * channels];

    if SampleFormat::from(frame.format).is_planar() {
        // 平面格式 (FLTP)：每通道一个连续平面。
        // frame.data[0]=>[L0][L1]..., frame.data[1]=>[R0][R1]...
        for (ch, plane) in frame.data.iter().enumerate().take(channels) {
            if plane.is_null() {
                return Err(RsmediaError::custom(format!(
                    "Channel {ch} data pointer is null"
                )));
            }
            // 每通道 samples 个连续样本，直接以其交织写入交错 buffer，避免逐样本下标 `push`。
            let plane_data = unsafe { std::slice::from_raw_parts(*plane as *const T, samples) };
            for (s, &v) in plane_data.iter().enumerate() {
                buffer[s * channels + ch] = v;
            }
        }
    } else {
        // 交错格式布局 (FLT)：所有声道交错，直接整体复制
        // frame.data[0] -> [L0][R0][L1][R1]...
        unsafe {
            let data = std::slice::from_raw_parts(frame.data[0] as *const T, samples * channels);
            buffer.copy_from_slice(data);
        }
    }

    // 按 [1, samples, channels] 组织音频数据
    ndarray::Array3::from_shape_vec((1, samples, channels), buffer)
        .map_err(|_| RsmediaError::custom("Audio data shape mismatch"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colors::Color;
    use std::time::Duration;

    /// 用 `Color::from_rgb` 生成渐变测试图案并填充 RGB24 帧。
    /// 返回 r/g/b 三个平面，方便调用方做断言。
    fn fill_rgb_data(
        frame: &mut MediaFrame<u8>,
        width: usize,
        height: usize,
    ) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let mut r = vec![0u8; width * height];
        let mut g = vec![0u8; width * height];
        let mut b = vec![0u8; width * height];

        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                let c = Color::from_rgb(
                    ((x as f32 / width as f32) * 255.0) as u8,
                    ((y as f32 / height as f32) * 255.0) as u8,
                    (((x + y) as f32 / (width + height) as f32) * 255.0) as u8,
                );
                r[idx] = c.r();
                g[idx] = c.g();
                b[idx] = c.b();
                frame.data[[y, x, 0]] = c.r();
                frame.data[[y, x, 1]] = c.g();
                frame.data[[y, x, 2]] = c.b();
            }
        }
        (r, g, b)
    }

    const TEST_WIDTH: usize = 320;
    const TEST_HEIGHT: usize = 240;
    const TIME_BASE: ffi::AVRational = ffi::AVRational { num: 1, den: 30 }; // 30 fps

    #[test]
    fn test_frame_data_access() -> Result<()> {
        let mut frame = MediaFrame::<u8>::new_video_frame(
            TEST_WIDTH,
            TEST_HEIGHT,
            PixelFormat::RGB24,
            TIME_BASE,
        )?;

        // 测试数据访问和修改
        let (r, g, b) = fill_rgb_data(&mut frame, TEST_WIDTH, TEST_HEIGHT);

        // 验证数据正确性
        for y in 0..TEST_HEIGHT {
            for x in 0..TEST_WIDTH {
                let idx = y * TEST_WIDTH + x;
                assert_eq!(frame.data[[y, x, 0]], r[idx]);
                assert_eq!(frame.data[[y, x, 1]], g[idx]);
                assert_eq!(frame.data[[y, x, 2]], b[idx]);
            }
        }

        Ok(())
    }

    #[test]
    fn test_different_pixel_types() -> Result<()> {
        // 测试不同像素类型
        let frame_u8 = MediaFrame::<u8>::new_video_frame(
            TEST_WIDTH,
            TEST_HEIGHT,
            PixelFormat::RGB24,
            TIME_BASE,
        )?;
        assert_eq!(std::mem::size_of_val(&frame_u8.data[[0, 0, 0]]), 1);

        let frame_u16 = MediaFrame::<u16>::new_video_frame(
            TEST_WIDTH,
            TEST_HEIGHT,
            PixelFormat::RGB24,
            TIME_BASE,
        )?;
        assert_eq!(std::mem::size_of_val(&frame_u16.data[[0, 0, 0]]), 2);

        let frame_f32 = MediaFrame::<f32>::new_video_frame(
            TEST_WIDTH,
            TEST_HEIGHT,
            PixelFormat::RGB24,
            TIME_BASE,
        )?;
        assert_eq!(std::mem::size_of_val(&frame_f32.data[[0, 0, 0]]), 4);

        Ok(())
    }

    #[test]
    fn test_frame_timestamps() -> Result<()> {
        let fps = TIME_BASE.den as f64 / TIME_BASE.num as f64;
        let frame_duration = Duration::from_secs_f64(1.0 / fps);

        let mut frames = Vec::new();
        for i in 0..5 {
            let mut frame = MediaFrame::<u8>::new_video_frame(
                TEST_WIDTH,
                TEST_HEIGHT,
                PixelFormat::RGB24,
                TIME_BASE,
            )?;
            frame.set_pts(i as i64);
            frames.push(frame);
        }

        // 验证时间戳的正确性
        for (i, frame) in frames.iter().enumerate() {
            let expected_time = frame_duration * i as u32;
            let actual_time = Duration::from_secs_f64(
                frame.pts as f64 * frame.time_base.num as f64 / frame.time_base.den as f64,
            );
            assert!((actual_time - expected_time).as_secs_f32().abs() < 0.01);
        }

        Ok(())
    }

    #[test]
    fn test_create_rgb24_frame() -> Result<()> {
        let width = 640;
        let height = 360;
        let time_base = ffi::AVRational { num: 1, den: 30 }; // 30 fps

        let mut frame =
            MediaFrame::<u8>::new_video_frame(width, height, PixelFormat::RGB24, time_base)?;

        // 验证元数据与数组布局
        assert_eq!(frame.data.shape(), &[height, width, 3]);
        assert_eq!(frame.width, width);
        assert_eq!(frame.height, height);
        assert_eq!(frame.format, FrameFormat::Pixel(PixelFormat::RGB24));
        assert!(frame.data.is_standard_layout(), "RGB24 应为行主序连续布局");

        // 填充并读回验证
        for y in 0..height {
            for x in 0..width {
                frame.data[[y, x, 0]] = (x % 255) as u8;
                frame.data[[y, x, 1]] = (y % 255) as u8;
                frame.data[[y, x, 2]] = ((x + y) % 255) as u8;
            }
        }
        for y in 0..height {
            for x in 0..width {
                assert_eq!(frame.data[[y, x, 0]], (x % 255) as u8);
                assert_eq!(frame.data[[y, x, 1]], (y % 255) as u8);
                assert_eq!(frame.data[[y, x, 2]], ((x + y) % 255) as u8);
            }
        }

        Ok(())
    }

    #[test]
    fn test_create_audio_frame() -> Result<()> {
        let samples = 1024;
        let channels = 2;
        let sample_rate = 44100;
        let time_base = ffi::AVRational {
            num: 1,
            den: sample_rate as i32,
        };

        // 创建空的音频帧 (Float Planar format)
        let mut frame = MediaFrame::<f32>::new_audio_frame(
            SampleFormat::FLTP,
            channels,
            samples,
            sample_rate,
            time_base,
        )?;

        if frame.data.is_empty() {
            return Err(RsmediaError::custom("Frame data pointer is null"));
        }

        // 填充一些测试数据
        for s in 0..samples as usize {
            for ch in 0..channels as usize {
                // 生成简单的正弦波
                let t = s as f32 / sample_rate as f32;
                // 不同通道使用不同频率
                let freq = 440.0 * (ch + 1) as f32;
                frame.data[[0, s, ch]] = (2.0 * std::f32::consts::PI * freq * t).sin();
            }
        }

        // 验证
        assert_eq!(frame.nb_samples, samples);
        assert_eq!(frame.nb_channels, channels);
        assert_eq!(frame.sample_rate, sample_rate);
        assert_eq!(frame.format, FrameFormat::Sample(SampleFormat::FLTP));

        Ok(())
    }

    #[test]
    fn test_format_getter() -> Result<()> {
        // 视频帧
        let video = MediaFrame::<u8>::new_video_frame(
            TEST_WIDTH,
            TEST_HEIGHT,
            PixelFormat::RGB24,
            TIME_BASE,
        )?;
        match video.format() {
            Some(FrameFormat::Pixel(PixelFormat::RGB24)) => {}
            other => panic!("video format = {other:?}"),
        }

        // 音频帧
        let audio =
            MediaFrame::<f32>::new_audio_frame(SampleFormat::FLTP, 2, 16, 48000, TIME_BASE)?;
        match audio.format() {
            Some(FrameFormat::Sample(SampleFormat::FLTP)) => {}
            other => panic!("audio format = {other:?}"),
        }

        Ok(())
    }
}
