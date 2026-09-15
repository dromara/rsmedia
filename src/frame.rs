//! Audio/video frame model.
//!
//! [`MediaFrame`] is the single frame type for both media kinds: it carries the
//! frame's metadata (timestamps, colour properties, side data, ...) next to its
//! samples, and [`FrameData`] stores those samples. [`FrameData`] splits by
//! *memory layout* — interleaved or planar — rather than by media kind, so audio
//! and video share both variants and there is no video-specific or
//! audio-specific sample structure.
//!
//! The shape a given format requires comes from
//! [`PixelFormat::data_layout`](crate::pixel::PixelFormat::data_layout) and
//! [`SampleFormat::data_layout`], which derive it from FFmpeg's format
//! descriptors instead of naming formats. Every sample-copy routine below is
//! therefore written once and serves packed and planar, audio and video alike.

use crate::error::{Context, Result, RsmediaError};
use crate::fmt::{DataLayout, FrameFormat, SampleFormat};
use crate::options::Metadata;
use crate::pixel::PixelFormat;
use crate::{MediaType, time};

use ndarray::{Array2, Array3, ArrayView2, ArrayViewMut2};
use rsmpeg::avutil::{AVChannelLayout, AVFrame};
use rsmpeg::ffi;

use yuv::{
    BufferStoreMut, YuvConversionMode, YuvPlanarImage, YuvPlanarImageMut, YuvRange,
    YuvStandardMatrix,
};

/// Element types a [`FrameData`] plane may hold.
///
/// The bound is what the sample helpers rely on: signed/unsigned integers and
/// floats that can be cloned, defaulted, compared and cast between one another.
pub trait ElementType:
    'static
    + Send
    + Sync
    + bytemuck::Pod
    + std::fmt::Debug
    + num_traits::Zero
    + num_traits::NumCast
    + num_traits::NumAssign
{
}

impl ElementType for u8 {}
impl ElementType for u16 {}
impl ElementType for u32 {}
impl ElementType for i16 {}
impl ElementType for i32 {}
impl ElementType for f32 {}
impl ElementType for f64 {}

/// The samples of a [`MediaFrame`], in one of the two layouts samples are
/// stored in.
///
/// The variants split by **memory layout, not media kind** — audio and video
/// share both of them:
///
/// | content | variant | array shape |
/// |---|---|---|
/// | packed video (`RGB24`, `YUYV422`, ...) | [`Packed`](Self::Packed) | `(height, width, elements_per_pixel)` |
/// | packed audio (`S16`, `FLT`, ...) | [`Packed`](Self::Packed) | `(1, nb_samples, nb_channels)` |
/// | planar video (`YUV420P`, `NV12`, ...) | [`Planar`](Self::Planar) | one `(rows, cols)` per plane |
/// | planar audio (`FLTP`, `S32P`, ...) | [`Planar`](Self::Planar) | one `(1, nb_samples)` per channel |
///
/// Which variant and which shapes a format needs is decided by
/// [`PixelFormat::data_layout`](crate::pixel::PixelFormat::data_layout) and
/// [`SampleFormat::data_layout`]; [`MediaFrame`] validates against that, so
/// callers never have to assemble the rules themselves.
///
/// # Invariants
///
/// * A plane is a row-major `rows x cols` block; an interleaved layout folds its
///   component axis into the columns, so it is exactly a one-plane frame.
/// * Audio layouts are always one row tall. The row stride therefore never
///   applies to audio, which is what lets one copy routine serve every case.
///
/// # Access
///
/// Access is explicit — there is no indexing shorthand, because the two variants
/// mean different things. Use [`as_packed`](Self::as_packed) /
/// [`as_planes`](Self::as_planes) to pick a variant, and
/// [`plane`](Self::plane) / [`map_planes`](Self::map_planes) to work with the
/// plane abstraction both variants share.
///
/// ```
/// # use ndarray::{Array2, Array3};
/// # use rsmedia::FrameData;
/// let mut packed = FrameData::from(Array3::<u8>::zeros((4, 6, 3)));
/// packed.as_packed_mut().unwrap()[[1, 2, 0]] = 7;   // pixel (2, 1), component 0
/// assert_eq!(packed.as_planes(), None);
///
/// let mut planar = FrameData::from(vec![Array2::<u8>::zeros((4, 6)), Array2::<u8>::zeros((2, 3))]);
/// planar.as_planes_mut().unwrap()[1][[0, 0]] = 9;
/// assert_eq!(planar.as_packed(), None);
/// assert_eq!(planar.plane(1).unwrap()[[0, 0]], 9);  // plane 1 is the first chroma plane
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum FrameData<T> {
    /// Interleaved samples in a single row-major array.
    Packed(Array3<T>),
    /// One row-major array per plane.
    Planar(Vec<Array2<T>>),
}

impl<T> FrameData<T> {
    /// The interleaved array, or `None` for a planar frame.
    pub fn as_packed(&self) -> Option<&Array3<T>> {
        match self {
            Self::Packed(array) => Some(array),
            Self::Planar(_) => None,
        }
    }

    /// The interleaved array, mutably.
    pub fn as_packed_mut(&mut self) -> Option<&mut Array3<T>> {
        match self {
            Self::Packed(array) => Some(array),
            Self::Planar(_) => None,
        }
    }

    /// The planes, or `None` for an interleaved frame.
    pub fn as_planes(&self) -> Option<&[Array2<T>]> {
        match self {
            Self::Packed(_) => None,
            Self::Planar(planes) => Some(planes),
        }
    }

    /// The planes, mutably.
    pub fn as_planes_mut(&mut self) -> Option<&mut [Array2<T>]> {
        match self {
            Self::Packed(_) => None,
            Self::Planar(planes) => Some(planes),
        }
    }

    /// How many arrays this frame is stored as: `1` when interleaved.
    pub fn num_planes(&self) -> usize {
        match self {
            Self::Packed(_) => 1,
            Self::Planar(planes) => planes.len(),
        }
    }

    /// Plane `plane` as a flat `rows x (cols x components)` view.
    ///
    /// This is the shape a row-wise copy needs, and it is the same for every
    /// layout and media kind. Borrowing a plane requires the underlying array to
    /// be contiguous, so the two ways this can fail are reported separately
    /// instead of collapsing into one `None`:
    ///
    /// * there is no such plane (an interleaved frame has exactly one, index 0);
    /// * the plane's array is not contiguous, so no flat view exists — use
    ///   [`plane_samples`](Self::plane_samples) when a copy is acceptable.
    pub fn plane(&self, plane: usize) -> Result<ArrayView2<'_, T>> {
        match self {
            Self::Packed(array) => {
                if plane != 0 {
                    return Err(no_such_plane(plane, 1));
                }
                let (rows, cols, components) = array.dim();
                let flat = array
                    .as_slice()
                    .ok_or_else(|| not_contiguous(plane, "interleaved"))?;
                ArrayView2::from_shape((rows, cols * components), flat)
                    .map_err(|error| RsmediaError::msg(format!("Plane {plane}: {error}")))
            }
            Self::Planar(planes) => planes
                .get(plane)
                .map(|plane| plane.view())
                .ok_or_else(|| no_such_plane(plane, planes.len())),
        }
    }

    /// Plane `plane` as a flat mutable view; see [`plane`](Self::plane).
    pub fn plane_mut(&mut self, plane: usize) -> Result<ArrayViewMut2<'_, T>> {
        match self {
            Self::Packed(array) => {
                if plane != 0 {
                    return Err(no_such_plane(plane, 1));
                }
                let (rows, cols, components) = array.dim();
                let flat = array
                    .as_slice_mut()
                    .ok_or_else(|| not_contiguous(plane, "interleaved"))?;
                ArrayViewMut2::from_shape((rows, cols * components), flat)
                    .map_err(|error| RsmediaError::msg(format!("Plane {plane}: {error}")))
            }
            Self::Planar(planes) => {
                let count = planes.len();
                planes
                    .get_mut(plane)
                    .map(|plane| plane.view_mut())
                    .ok_or_else(|| no_such_plane(plane, count))
            }
        }
    }

    /// Every plane's shape, as [`DataLayout::shapes`] reports it.
    pub fn shapes(&self) -> Vec<(usize, usize)> {
        match self {
            Self::Packed(array) => {
                let (rows, cols, _) = array.dim();
                vec![(rows, cols)]
            }
            Self::Planar(planes) => planes.iter().map(|plane| plane.dim()).collect(),
        }
    }

    /// Total number of samples across every plane.
    pub fn len(&self) -> usize {
        match self {
            Self::Packed(array) => array.len(),
            Self::Planar(planes) => planes.iter().map(|plane| plane.len()).sum(),
        }
    }

    /// `true` when no plane holds a single sample.
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Packed(array) => array.is_empty(),
            Self::Planar(planes) => planes.iter().all(|plane| plane.is_empty()),
        }
    }

    /// Whether this data has exactly the plane count and plane shapes `layout`
    /// asks for.
    pub fn matches(&self, layout: &DataLayout) -> bool {
        match (self, layout) {
            (
                Self::Packed(array),
                DataLayout::Interleaved {
                    rows,
                    cols,
                    components,
                },
            ) => array.dim() == (*rows, *cols, *components),
            (Self::Planar(planes), DataLayout::Planar(shapes)) => {
                planes.len() == shapes.len()
                    && planes
                        .iter()
                        .zip(shapes)
                        .all(|(plane, &shape)| plane.dim() == shape)
            }
            _ => false,
        }
    }
}

impl<T: ElementType> FrameData<T> {
    /// Zero-initialised samples shaped exactly as `layout` requires.
    pub fn zeros(layout: &DataLayout) -> Self {
        match layout {
            DataLayout::Interleaved {
                rows,
                cols,
                components,
            } => Self::Packed(Array3::zeros((*rows, *cols, *components))),
            DataLayout::Planar(shapes) => Self::Planar(
                shapes
                    .iter()
                    .map(|&(rows, cols)| Array2::zeros((rows, cols)))
                    .collect(),
            ),
        }
    }

    /// Rebuilds the samples by mapping every plane, keeping the layout: an
    /// interleaved frame stays interleaved, a planar frame stays planar.
    ///
    /// `f` gets the plane index and the plane as a flat
    /// `rows x (cols x components)` view — the same extent for every layout and
    /// media kind — and returns its replacement. Plane-wise processing (tone
    /// mapping, per-plane resampling, custom conversions) is therefore written
    /// once instead of once per layout.
    pub fn map_planes<U: ElementType>(
        &self,
        mut f: impl FnMut(usize, ArrayView2<'_, T>) -> Result<Array2<U>>,
    ) -> Result<FrameData<U>> {
        match self {
            Self::Packed(array) => {
                let (rows, cols, components) = array.dim();
                let view = ArrayView2::from_shape(
                    (rows, cols * components),
                    array
                        .as_slice()
                        .ok_or_else(|| RsmediaError::msg("Interleaved frame must be contiguous"))?,
                )
                .map_err(|e| RsmediaError::msg(format!("Failed to view interleaved plane: {e}")))?;
                let mapped = f(0, view)?;
                let samples: Vec<U> = mapped.iter().cloned().collect();
                Ok(FrameData::Packed(
                    Array3::from_shape_vec((rows, cols, components), samples).map_err(|e| {
                        RsmediaError::msg(format!("Failed to rebuild interleaved frame: {e}"))
                    })?,
                ))
            }
            Self::Planar(planes) => {
                let mut mapped = Vec::with_capacity(planes.len());
                for (index, plane) in planes.iter().enumerate() {
                    mapped.push(f(index, plane.view())?);
                }
                Ok(FrameData::Planar(mapped))
            }
        }
    }

    /// Reads plane `plane` as a contiguous `Vec<U>`, casting every sample.
    ///
    /// Unlike [`plane`](Self::plane) this **tolerates a non-contiguous array**:
    /// the samples are copied out anyway, and `as_standard_layout` materialises
    /// them in row-major order on demand.
    pub fn plane_samples<U: ElementType>(&self, plane: usize) -> Result<Vec<U>> {
        let samples: Vec<T> = match self {
            Self::Packed(array) => {
                if plane != 0 {
                    return Err(no_such_plane(plane, 1));
                }
                array.as_standard_layout().iter().copied().collect()
            }
            Self::Planar(planes) => planes
                .get(plane)
                .ok_or_else(|| no_such_plane(plane, planes.len()))?
                .as_standard_layout()
                .iter()
                .copied()
                .collect(),
        };
        Ok(samples
            .into_iter()
            .map(|value| num_traits::cast::<T, U>(value).unwrap_or(U::zero()))
            .collect())
    }
}

/// The error for a plane index a layout does not have.
fn no_such_plane(plane: usize, count: usize) -> RsmediaError {
    RsmediaError::msg(format!("Frame has no plane {plane}: it has {count}"))
}

/// The error for borrowing a plane whose array is not stored contiguously.
fn not_contiguous(plane: usize, layout: &str) -> RsmediaError {
    RsmediaError::msg(format!(
        "Plane {plane} of a {layout} frame is not contiguous, so it has no flat view; \
         use `plane_samples` (which copies) or make the array standard layout"
    ))
}

/// Converts a packed `RGB24` frame into a planar `YUV420P` frame.
///
/// The size comes from the packed array, which `RGB24`'s layout fixes at
/// `(height, width, 3)`, and both dimensions must be even because YUV420P
/// chroma is a 2x2 downsample. `matrix` is the luma/chroma matrix to use;
/// [`MediaFrame::convert_rgb24_to_yuv420p`] picks one from the frame's colour
/// metadata.
///
/// Samples are read as `u8`, which is what both of these formats mean — an 8-bit
/// component. A 9..16-bit picture is a *different* pixel format (e.g.
/// `YUV420P10LE`), and goes through the scaler instead.
fn rgb24_to_yuv420p<T: ElementType>(
    data: &FrameData<T>,
    matrix: YuvStandardMatrix,
) -> Result<FrameData<T>> {
    let (height, width) = rgb24_extent(data)?;
    if !width.is_multiple_of(2) || !height.is_multiple_of(2) {
        return Err(RsmediaError::msg(format!(
            "RGB24 -> YUV420P requires even dimensions, got {width}x{height}"
        )));
    }

    let (uv_width, uv_height) = (width / 2, height / 2);
    let rgb = data.plane_samples::<u8>(0)?;
    let mut y_plane = vec![0u8; width * height];
    let mut u_plane = vec![0u8; uv_width * uv_height];
    let mut v_plane = vec![0u8; uv_width * uv_height];

    {
        let mut planar = YuvPlanarImageMut {
            y_plane: BufferStoreMut::Borrowed(&mut y_plane),
            y_stride: width as u32,
            u_plane: BufferStoreMut::Borrowed(&mut u_plane),
            u_stride: uv_width as u32,
            v_plane: BufferStoreMut::Borrowed(&mut v_plane),
            v_stride: uv_width as u32,
            width: width as u32,
            height: height as u32,
        };
        yuv::rgb_to_yuv420(
            &mut planar,
            &rgb,
            (width * 3) as u32,
            YuvRange::Full,
            matrix,
            YuvConversionMode::Professional,
        )
        .map_err(|e| RsmediaError::msg(format!("convert rgb24 to yuv420p error:{e}")))?;
    }

    Ok(FrameData::Planar(vec![
        plane_from(y_plane, height, width)?,
        plane_from(u_plane, uv_height, uv_width)?,
        plane_from(v_plane, uv_height, uv_width)?,
    ]))
}

/// Converts a planar `YUV420P` frame into a packed `RGB24` frame.
///
/// The size comes from the luma plane, whose `(height, width)` shape the
/// `YUV420P` layout fixes.
fn yuv420p_to_rgb24<T: ElementType>(
    data: &FrameData<T>,
    matrix: YuvStandardMatrix,
) -> Result<FrameData<T>> {
    let (height, width) = yuv420p_extent(data)?;
    let (uv_width, uv_height) = (width / 2, height / 2);

    let y = data.plane_samples::<u8>(0)?;
    let u = data.plane_samples::<u8>(1)?;
    let v = data.plane_samples::<u8>(2)?;
    if y.len() < width * height || u.len() < uv_width * uv_height || v.len() < uv_width * uv_height
    {
        return Err(RsmediaError::msg("YUV420P plane buffer too small"));
    }

    let planar = YuvPlanarImage {
        y_plane: &y,
        y_stride: width as u32,
        u_plane: &u,
        u_stride: uv_width as u32,
        v_plane: &v,
        v_stride: uv_width as u32,
        width: width as u32,
        height: height as u32,
    };
    let mut rgb = vec![0u8; width * height * 3];
    yuv::yuv420_to_rgb(
        &planar,
        &mut rgb,
        (width * 3) as u32,
        YuvRange::Full,
        matrix,
    )
    .map_err(|e| RsmediaError::msg(format!("convert yuv420p to rgb24 error:{e}")))?;

    Ok(FrameData::Packed(
        Array3::from_shape_vec((height, width, 3), cast_samples::<u8, T>(rgb))
            .map_err(|e| RsmediaError::msg(format!("Failed to build RGB24 frame: {e}")))?,
    ))
}

impl<T: Clone + num_traits::Zero> Default for FrameData<T> {
    fn default() -> Self {
        Self::Packed(Array3::zeros((0, 0, 0)))
    }
}

impl<T> From<Array3<T>> for FrameData<T> {
    /// A single interleaved array (packed video, or interleaved audio).
    fn from(array: Array3<T>) -> Self {
        Self::Packed(array)
    }
}

impl<T> From<Array2<T>> for FrameData<T> {
    /// A single plane of a planar format.
    fn from(array: Array2<T>) -> Self {
        Self::Planar(vec![array])
    }
}

impl<T> From<Vec<Array2<T>>> for FrameData<T> {
    /// One array per plane of a planar format.
    fn from(planes: Vec<Array2<T>>) -> Self {
        Self::Planar(planes)
    }
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

/// One decoded or to-be-encoded audio/video frame.
///
/// Metadata lives in the fields of this struct, samples in [`data`](Self::data);
/// both media kinds share the one type rather than splitting into a video and an
/// audio struct. A field that only one kind uses stays at its neutral default for
/// the other: `width` / `height` are zero on an audio frame, `sample_rate` /
/// `nb_samples` / `nb_channels` are zero on a video frame.
///
/// # Field coverage
///
/// Every value-carrying field of `AVFrame` is mirrored here, so
/// [`from_avframe`](MediaFrame::from_avframe) and [`to_avframe`](MediaFrame::to_avframe)
/// form a lossless round trip for the modelled fields.
///
/// The `AVFrame` fields that are *not* mirrored are the ones that cannot be carried as
/// values: `data` / `linesize` / `extended_data` are what [`data`](Self::data) packs
/// into planes, `buf` / `extended_buf` / `nb_extended_buf` / `opaque` /
/// `opaque_ref` / `private_ref` are ownership handles, and `hw_frames_ctx` describes a
/// hardware frame pool that has no meaning once the samples are copied into host
/// memory. They are intentionally excluded rather than missing.
///
/// # Parameters
///
/// * `T` - The underlying data type for samples/pixels. It must match the format's
///   element size: one byte per component for 8-bit formats such as `RGB24` /
///   `YUV420P`, two for 9..16-bit ones such as `YUV420P10LE`, and the sample size
///   for audio (`i16` for `S16`, `f32` for `FLTP`, ...).
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
    /// 帧的采样数据。布局随格式而变（见 [`FrameData`]），与音/视频无关。
    pub data: FrameData<T>,
    /// Time base: `pts`, durations and similar fields are counted in these units.
    ///
    /// Only two things set it: a frame copied out of an `AVFrame`, which carries
    /// the container stream time base (`1/15360` for mp4, say), and an explicit
    /// [`set_time_base`](Self::set_time_base) call. Audio frames get
    /// `1/sample_rate` from their constructor; **video frames stay unset (`0/1`)**,
    /// because a resolution carries no frame rate.
    ///
    /// Leaving it unset is safe. The encoder interprets pts in its *own* input time
    /// base (video `1/fps`, audio `1/sample_rate`) and overwrites this field as soon
    /// as it receives the frame; it only rescales when the frame carries a *valid
    /// and different* time base, which is the case of pts inherited from a decoder's
    /// container time base. So [`set_pts`](Self::set_pts) needs a matching
    /// `set_time_base` only when the pts is not already counted in the encoder's
    /// time base.
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
    /// 仅音频字段：采样率（Hz）—— 这批样本**实际**的采样率（源率）。
    ///
    /// 与编码器的目标率是两个量：二者不同时编码器会自动重采样，因此这里应填数据的真实
    /// 速率。`0` 表示未声明，编码器接收该帧时按自己的目标率补齐。
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

impl<T: ElementType> Default for MediaFrame<T> {
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
            data: FrameData::default(),
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
    T: ElementType,
{
    /// The data layout this frame's format requires at its size.
    ///
    /// Derived from the format ([`PixelFormat::data_layout`] /
    /// [`SampleFormat::data_layout`]), so it applies to video and audio alike.
    pub fn data_layout(&self) -> Result<DataLayout> {
        if self.media_type == MediaType::VIDEO {
            let format = self
                .format
                .into_pixel()
                .ok_or_else(|| RsmediaError::msg("Video frame needs a pixel format"))?;
            format.data_layout(self.width, self.height).ok_or_else(|| {
                RsmediaError::msg(format!(
                    "Pixel format {} cannot be stored as sample planes at {}x{}",
                    format.get_pix_fmt_name(),
                    self.width,
                    self.height
                ))
            })
        } else {
            let format = self
                .format
                .into_sample()
                .ok_or_else(|| RsmediaError::msg("Audio frame needs a sample format"))?;
            Ok(format.data_layout(self.nb_channels as usize, self.nb_samples as usize))
        }
    }

    /// 创建视频帧。
    ///
    /// `data` 的平面布局必须与 `format` 在 `width` x `height` 下的布局一致
    /// （见 [`PixelFormat::data_layout`]）：packed 格式为单个
    /// `(height, width, elements_per_pixel)` 数组，planar 格式为每平面一个数组。
    /// 传入 `ndarray::Array3` / `Vec<Array2>` 会分别视为交错 / 平面帧。
    ///
    /// 时间基保持未设置：分辨率里没有帧率信息，而编码器会用自己的输入时间基解释
    /// pts（见 [`time_base`](Self::time_base)）。要按自己的时间基表达 pts 时再调
    /// [`set_time_base`](Self::set_time_base)。
    pub fn new_video(
        width: usize,
        height: usize,
        format: PixelFormat,
        data: impl Into<FrameData<T>>,
    ) -> Result<Self> {
        Self {
            width,
            height,
            data: data.into(),
            media_type: MediaType::VIDEO,
            format: FrameFormat::Pixel(format),
            ..Self::default()
        }
        .validated()
    }

    /// 创建视频帧（各平面零初始化）。
    ///
    /// 只需 `width` / `height` / `format`；时间基的处理见 [`new_video`](Self::new_video)。
    pub fn new_video_frame(width: usize, height: usize, format: PixelFormat) -> Result<Self> {
        let layout = format.data_layout(width, height).ok_or_else(|| {
            RsmediaError::msg(format!(
                "Pixel format {} cannot be stored as sample planes at {width}x{height}",
                format.get_pix_fmt_name()
            ))
        })?;
        Self::new_video(width, height, format, FrameData::zeros(&layout))
    }

    /// 创建音频帧。
    ///
    /// `data` 的布局必须与 `format` 一致（见 [`SampleFormat::data_layout`]）：
    /// 平面采样格式每声道一个 `(1, nb_samples)` 平面，交错格式为单个
    /// `(1, nb_samples, nb_channels)` 数组。
    ///
    /// 时间基自动取 `1/sample_rate` —— 音频的固有时间基，不需要调用方传递
    /// （`sample_rate` 为 0 时保持未设置）。
    ///
    /// `sample_rate` 是**源率**（这批样本实际是多少 Hz），与编码器的目标率是两个量：
    /// 二者不同时编码器会自动重采样，所以应当填数据真实的采样率。确实不想声明时可传
    /// `0`，帧上留空（`time_base` 随之留空，见 [`time_base`](Self::time_base)），
    /// 编码器接收该帧时会按自己的目标率补齐。
    pub fn new_audio(
        format: SampleFormat,
        nb_channels: u32,
        nb_samples: u32,
        sample_rate: u32,
        data: impl Into<FrameData<T>>,
    ) -> Result<Self> {
        Self {
            format: FrameFormat::Sample(format),
            data: data.into(),
            time_base: if sample_rate > 0 {
                time::new_rational(1, sample_rate as i32)
            } else {
                time::new_rational(0, 1)
            },
            sample_rate,
            nb_samples,
            nb_channels,
            media_type: MediaType::AUDIO,
            ..Self::default()
        }
        .validated()
    }

    /// 创建音频帧（各平面零初始化）。
    ///
    /// 只需 `format` / `nb_channels` / `nb_samples` / `sample_rate`；时间基由
    /// `sample_rate` 推出，见 [`new_audio`](Self::new_audio)。
    pub fn new_audio_frame(
        format: SampleFormat,
        nb_channels: u32,
        nb_samples: u32,
        sample_rate: u32,
    ) -> Result<Self> {
        let layout = format.data_layout(nb_channels as usize, nb_samples as usize);
        Self::new_audio(
            format,
            nb_channels,
            nb_samples,
            sample_rate,
            FrameData::zeros(&layout),
        )
    }

    /// 校验 [`data`](Self::data) 的形状与格式要求的布局一致，且 `T` 的宽度与该格式
    /// 的每样本字节数一致。
    ///
    /// 多项都在**构造点**校验：形状不符、或元素宽度不符（例如把 `u8` 样本放进 10bit
    /// 格式）都会让跨 FFI 的拷贝越界，因此必须在能造出这种帧的地方就拒绝，而不是等到
    /// [`to_avframe`](Self::to_avframe)。
    fn validated(self) -> Result<Self> {
        // 音频的声道数/采样数必须为正。否则平面布局是空列表、`matches` 会接受，
        // 一个"没有声道"的帧就能一路走到 FFmpeg。视频侧的等价约束由
        // `PixelFormat::data_layout` 对 0 尺寸返回 `None` 覆盖，无需在此重复。
        if self.media_type == MediaType::AUDIO && (self.nb_channels == 0 || self.nb_samples == 0) {
            return Err(RsmediaError::msg(format!(
                "Audio frame needs a positive sample and channel count, got {} samples x {} channels",
                self.nb_samples, self.nb_channels
            )));
        }

        let layout = self.data_layout()?;
        if !self.data.matches(&layout) {
            return Err(RsmediaError::msg(format!(
                "Frame data does not match its format: expected planes {:?}, got {:?}",
                layout.shapes(),
                self.data.shapes()
            )));
        }
        validate_element_size::<T>(self.format, self.element_size()?)?;
        Ok(self)
    }

    /// 该帧格式的每样本字节数。
    ///
    /// 视频格式取每分量宽度（[`PixelFormat::bytes_per_component`]），音频格式取每采样点
    /// 宽度（[`SampleFormat::get_bytes_per_sample`]）。这是 `T` 必须精确匹配的尺寸，也是
    /// [`FrameData`] 的元素宽度与格式之间唯一的联系。
    ///
    /// 返回 `Err` 表示格式无法确定样本宽度（位流 / 调色板 / 硬件格式等）。
    fn element_size(&self) -> Result<usize> {
        if self.media_type == MediaType::VIDEO {
            self.format
                .into_pixel()
                .and_then(PixelFormat::bytes_per_component)
                .ok_or_else(|| RsmediaError::msg("Unsupported pixel format"))
        } else {
            self.format
                .into_sample()
                .and_then(|format| format.get_bytes_per_sample())
                .ok_or_else(|| RsmediaError::msg("Unsupported sample format"))
        }
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

    /// Copies an `AVFrame` into a frame with owned samples.
    ///
    /// The layout comes from FFmpeg: video planes are read at the size the pixel
    /// format's descriptor gives, dropping the row padding `linesize` may add,
    /// and audio planes are read contiguously.
    pub fn from_avframe(frame: &AVFrame) -> Result<Self> {
        if plane_ptr(frame, 0).is_null() {
            return Err(RsmediaError::msg("Invalid frame data"));
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
            let sample_format = SampleFormat::from(format);
            let frame_format = FrameFormat::Sample(sample_format);
            let element_bytes = sample_format
                .get_bytes_per_sample()
                .ok_or_else(|| RsmediaError::msg("Unsupported sample format"))?;
            validate_element_size::<T>(frame_format, element_bytes)?;

            let mut media = Self {
                format: frame_format,
                pts,
                pkt_dts,
                duration,
                pkt_duration: duration,
                time_base,
                data: FrameData::default(),
                media_type: MediaType::AUDIO,
                sample_rate: frame.sample_rate as u32,
                nb_samples: frame.nb_samples as u32,
                nb_channels: frame.ch_layout.nb_channels as u32,
                ..Self::default()
            };
            media.data = read_samples::<T>(frame, &media.data_layout()?)?;
            media.copy_avframe_meta(frame);
            Ok(media)
        } else if width > 0 && height > 0 {
            let pixel_format = PixelFormat::from(format);
            let frame_format = FrameFormat::Pixel(pixel_format);
            let element_bytes = pixel_format
                .bytes_per_component()
                .ok_or_else(|| RsmediaError::msg("Unsupported pixel format"))?;
            validate_element_size::<T>(frame_format, element_bytes)?;

            let mut media = Self {
                width,
                height,
                pts,
                pkt_dts,
                format: frame_format,
                duration,
                pkt_duration: duration,
                time_base,
                data: FrameData::default(),
                media_type: MediaType::VIDEO,
                pict_type: frame.pict_type,
                ..Self::default()
            };
            media.data = read_samples::<T>(frame, &media.data_layout()?)?;
            media.copy_avframe_meta(frame);
            Ok(media)
        } else {
            Err(RsmediaError::msg("Unsupported frame format"))
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
            // `key_frame` 是 `AV_FRAME_FLAG_KEY` 的便捷镜像（读入方向见
            // `copy_avframe_meta`），因此写出时也要让它生效：否则
            // `frame.key_frame = true` 会被静默丢弃，两个字段互相矛盾。
            let key = ffi::AV_FRAME_FLAG_KEY as i32;
            (*raw).flags = if self.key_frame {
                self.flags | key
            } else {
                self.flags & !key
            };
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

    /// A copy of this frame's metadata, carrying different samples.
    ///
    /// The destination format is the only metadata a conversion changes; every
    /// other field is carried over verbatim. The field list is written out
    /// exhaustively (rather than `..self.clone()`) so that adding a field to
    /// [`MediaFrame`] fails to compile here until it is considered — and the
    /// samples are *not* cloned, which is the point of the method.
    fn with_data(&self, data: FrameData<T>, format: PixelFormat) -> Self {
        let Self {
            pts,
            pkt_dts,
            duration,
            media_type,
            width,
            height,
            pict_type,
            sample_rate,
            nb_samples,
            nb_channels,
            key_frame,
            flags,
            quality,
            repeat_pict,
            colorspace,
            color_primaries,
            color_trc,
            color_range,
            chroma_location,
            sample_aspect_ratio,
            crop_top,
            crop_bottom,
            crop_left,
            crop_right,
            #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
            alpha_mode,
            pkt_duration,
            best_effort_timestamp,
            decode_error_flags,
            // 这两个由调用方给出，其余字段直接搬运。
            format: _,
            data: _,
            time_base,
            metadata,
            side_data,
        } = self;
        Self {
            pts: *pts,
            pkt_dts: *pkt_dts,
            duration: *duration,
            format: FrameFormat::Pixel(format),
            data,
            time_base: *time_base,
            media_type: *media_type,
            width: *width,
            height: *height,
            pict_type: *pict_type,
            sample_rate: *sample_rate,
            nb_samples: *nb_samples,
            nb_channels: *nb_channels,
            key_frame: *key_frame,
            flags: *flags,
            quality: *quality,
            repeat_pict: *repeat_pict,
            colorspace: *colorspace,
            color_primaries: *color_primaries,
            color_trc: *color_trc,
            color_range: *color_range,
            chroma_location: *chroma_location,
            sample_aspect_ratio: *sample_aspect_ratio,
            crop_top: *crop_top,
            crop_bottom: *crop_bottom,
            crop_left: *crop_left,
            crop_right: *crop_right,
            #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
            alpha_mode: *alpha_mode,
            pkt_duration: *pkt_duration,
            best_effort_timestamp: *best_effort_timestamp,
            decode_error_flags: *decode_error_flags,
            metadata: metadata.clone(),
            side_data: side_data.clone(),
        }
    }

    /// 转换为新 `AVFrame`：采样按布局拷进 FFmpeg 分配的带对齐缓冲。
    pub fn to_avframe(&self) -> Result<AVFrame> {
        let layout = self.data_layout()?;
        if !self.data.matches(&layout) {
            return Err(RsmediaError::msg(format!(
                "Frame data does not match its format: expected planes {:?}, got {:?}",
                layout.shapes(),
                self.data.shapes()
            )));
        }
        validate_element_size::<T>(self.format, self.element_size()?)?;

        let mut frame = AVFrame::new();
        frame.set_format(i32::from(self.format));
        if self.media_type == MediaType::VIDEO {
            frame.set_width(self.width as i32);
            frame.set_height(self.height as i32);
            frame.set_pict_type(self.pict_type);
        } else {
            frame.set_nb_samples(self.nb_samples as i32);
            frame.set_sample_rate(self.sample_rate as i32);
            frame.set_ch_layout(
                AVChannelLayout::from_nb_channels(self.nb_channels as i32).into_inner(),
            );
        }
        frame
            .alloc_buffer()
            .context("Failed to allocate frame buffer")?;
        write_samples(&mut frame, &self.data)?;

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

    /// 校验当前帧为视频帧、且像素格式恰为 `expected`；否则返回可读的错误信息。
    ///
    /// `expected_desc` 只用于错误消息（FFmpeg 的格式名，如 `RGB24`）。
    fn ensure_video_format(&self, expected: FrameFormat, expected_desc: &str) -> Result<()> {
        if self.media_type != MediaType::VIDEO {
            return Err(RsmediaError::msg("Only video frames are supported"));
        }
        if self.format != expected {
            let got = match self.format {
                FrameFormat::Pixel(p) => p.get_pix_fmt_name(),
                FrameFormat::Sample(_) => "<audio format>".to_string(),
            };
            return Err(RsmediaError::msg(format!(
                "Expected {expected_desc} format, got {got}"
            )));
        }
        Ok(())
    }

    /// 选择 RGB <-> YUV 转换用的标准色彩矩阵。
    ///
    /// 优先读帧携带的 `colorspace` 元数据；未标记时按分辨率启发式
    /// （SD -> BT.601 / HD -> BT.709 / UHD -> BT.2020），与 ffmpeg 的
    /// `sws_getCoefficients` 缺省行为一致。
    fn yuv_matrix(&self) -> YuvStandardMatrix {
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

    /// Converts an `RGB24` video frame to planar `YUV420P`.
    ///
    /// The YUV matrix comes from the frame's own colour metadata
    /// (`colorspace`, falling back to a resolution heuristic) — see
    /// [`convert_rgb24_to_yuv420p_with_matrix`](Self::convert_rgb24_to_yuv420p_with_matrix)
    /// to pin it. Samples are always full-range.
    ///
    /// This is the fast path for exactly this pair: it runs in the `yuv` crate
    /// rather than through a scaler, and sits next to
    /// [`convert_to`](Self::convert_to), which handles every other format pair.
    /// `BGR24`, `NV12`, ... are rejected — use `convert_to` for those.
    pub fn convert_rgb24_to_yuv420p(&self) -> Result<Self> {
        self.convert_rgb24_to_yuv420p_with_matrix(self.yuv_matrix())
    }

    /// [`convert_rgb24_to_yuv420p`](Self::convert_rgb24_to_yuv420p) with an
    /// explicit YUV matrix instead of the one derived from colour metadata.
    ///
    /// Pick BT.601 / BT.709 / BT.2020 explicitly to match a source video's own
    /// standard, rather than relying on the resolution heuristic.
    ///
    /// # Examples
    ///
    /// ```
    /// # use rsmedia::MediaFrame;
    /// # fn d(mut f: MediaFrame<u8>) -> rsmedia::Result<()> {
    /// let yuv = f.convert_rgb24_to_yuv420p_with_matrix(yuv::YuvStandardMatrix::Bt709)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn convert_rgb24_to_yuv420p_with_matrix(&self, matrix: YuvStandardMatrix) -> Result<Self> {
        self.ensure_video_format(FrameFormat::Pixel(PixelFormat::RGB24), "RGB24")?;
        let data = rgb24_to_yuv420p(&self.data, matrix)?;
        Ok(self.with_data(data, PixelFormat::YUV420P))
    }

    /// Converts a planar `YUV420P` video frame to packed `RGB24`.
    ///
    /// The inverse of [`convert_rgb24_to_yuv420p`](Self::convert_rgb24_to_yuv420p);
    /// chroma is read at its native (half) size and the matrix is derived from
    /// colour metadata.
    pub fn convert_yuv420p_to_rgb24(&self) -> Result<Self> {
        self.ensure_video_format(FrameFormat::Pixel(PixelFormat::YUV420P), "YUV420P")?;
        let data = yuv420p_to_rgb24(&self.data, self.yuv_matrix())?;
        Ok(self.with_data(data, PixelFormat::RGB24))
    }

    /// Converts this video frame to any pixel format FFmpeg's swscale can reach.
    ///
    /// This is the general conversion: it runs through swscale, so it covers
    /// every pair of pixel formats swscale supports — the whole YUV (planar and
    /// packed), NV, RGB/BGR and GRAY families, at 8 bit as well as 10/12/16 bit
    /// — and it honours the frame's colour metadata (`colorspace`,
    /// `color_range`, `color_primaries`, `color_trc`), exactly like a `scale`
    /// filter with matching input and output sizes. Formats that have no host
    /// samples (palette, bitstream, hardware) are rejected.
    ///
    /// `T` fixes the component width: pass an 8-bit destination format for
    /// [`MediaFrame<u8>`] and a 16-bit one for [`MediaFrame<u16>`]. Asking for a
    /// different width (e.g. `YUV420P10LE` out of a `MediaFrame<u8>`) is an error
    /// rather than a silent reinterpretation of the bytes.
    ///
    /// For the `RGB24` <-> `YUV420P` pair specifically,
    /// [`convert_rgb24_to_yuv420p`](Self::convert_rgb24_to_yuv420p) and
    /// [`convert_yuv420p_to_rgb24`](Self::convert_yuv420p_to_rgb24) do the same
    /// job without a scaler and let you pin the YUV matrix.
    ///
    /// # Examples
    ///
    /// ```
    /// use rsmedia::{MediaFrame, PixelFormat};
    /// # fn d(frame: MediaFrame<u8>) -> rsmedia::Result<()> {
    /// let nv12 = frame.convert_to(PixelFormat::NV12)?;
    /// assert_eq!(nv12.format().unwrap(), rsmedia::FrameFormat::Pixel(PixelFormat::NV12));
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Fails for audio frames, for a destination the source's element width or
    /// swscale cannot serve, and for `dst` formats with no sample planes.
    pub fn convert_to(&self, dst: PixelFormat) -> Result<Self> {
        if self.media_type != MediaType::VIDEO {
            return Err(RsmediaError::msg(
                "Only video frames have a pixel format to convert",
            ));
        }
        if self.format == FrameFormat::Pixel(dst) {
            return Ok(self.clone());
        }
        let dst_layout = dst.data_layout(self.width, self.height).ok_or_else(|| {
            RsmediaError::msg(format!(
                "Pixel format {} cannot be stored as sample planes at {}x{}",
                dst.get_pix_fmt_name(),
                self.width,
                self.height
            ))
        })?;
        // `read_samples::<T>` reads `size_of::<T>()`-wide elements, so a target
        // with a different component width must be rejected here rather than
        // reinterpreted: asking for `YUV420P10LE` out of a `MediaFrame<u8>`
        // would otherwise read half a plane of garbage.
        let element_bytes = dst.bytes_per_component().ok_or_else(|| {
            RsmediaError::msg(format!(
                "Pixel format {} has no per-component size",
                dst.get_pix_fmt_name()
            ))
        })?;
        validate_element_size::<T>(FrameFormat::Pixel(dst), element_bytes)?;

        let src = self.to_avframe()?;
        // 尺寸不变，只换格式：swscale 的同一上下文即可完成格式与色彩空间转换。
        let converted = crate::scale::Scaler::new().scale_frame(
            &src,
            self.width as i32,
            self.height as i32,
            dst,
        )?;
        let data = read_samples::<T>(&converted, &dst_layout)?;
        Ok(self.with_data(data, dst))
    }
}

impl MediaFrame<u8> {
    /// Converts this video frame into an [`image::DynamicImage`].
    ///
    /// Every pixel format FFmpeg can decode to RGB is accepted: `RGB24`, `RGBA`
    /// and `GRAY8` are built straight from the samples, anything else (YUV, BGR,
    /// NV, ...) goes through swscale first. That is
    /// [`imgutils::to_dynamic_image`](crate::imgutils::to_dynamic_image) on this
    /// frame's `AVFrame`, so the free function and this method cannot disagree
    /// about which formats they handle.
    pub fn to_dynamic_image(&self) -> Result<image::DynamicImage> {
        crate::imgutils::to_dynamic_image(&self.to_avframe()?)
    }

    /// Builds an RGB24 video frame from an [`image::DynamicImage`].
    ///
    /// Any colour mode (RGB / RGBA / grey, ...) is converted to RGB8 first, and the
    /// frame takes the image's own dimensions. Like every video frame it starts
    /// with no time base (see [`time_base`](MediaFrame::time_base)).
    pub fn from_dynamic_image(img: &image::DynamicImage) -> Result<Self> {
        let rgb = img.to_rgb8();
        let (width, height) = rgb.dimensions();
        let (width, height) = (width as usize, height as usize);
        let array = Array3::from_shape_vec((height, width, 3), rgb.into_raw())
            .map_err(|e| RsmediaError::msg(format!("Failed to build ndarray from image: {e}")))?;
        Self::new_video(width, height, PixelFormat::RGB24, array)
    }
}

/// `AVFrame` 第 `plane` 个数据平面的起始指针。
///
/// FFmpeg 对超过 8 个平面（多声道音频）的帧改用 `extended_data`，并保证它与内联的
/// `data` 数组在 8 个平面以内互为别名，因此统一从 `extended_data` 取。
fn plane_ptr(frame: &AVFrame, plane: usize) -> *mut u8 {
    let extended = frame.extended_data;
    if extended.is_null() {
        frame.data[plane]
    } else {
        // SAFETY: FFmpeg keeps `extended_data` pointing at one entry per plane.
        unsafe { *extended.add(plane) }
    }
}

/// Reads `rows` rows of `row_len` samples each out of one `AVFrame` plane.
///
/// `AVFrame.linesize[plane]` is the byte distance between consecutive rows, so
/// the read walks row by row and leaves behind the alignment padding FFmpeg
/// keeps at the end of a row. Audio planes are exactly one row tall, so the row
/// stride never comes into play for them — one routine therefore covers packed
/// and planar, video and audio. The stride is applied as a signed offset because
/// `linesize` is negative for bottom-up frames.
fn read_plane<T: ElementType>(
    frame: &AVFrame,
    plane: usize,
    rows: usize,
    row_len: usize,
) -> Result<Vec<T>> {
    let source = plane_ptr(frame, plane) as *const T;
    if source.is_null() {
        return Err(RsmediaError::msg(format!("Frame plane {plane} is null")));
    }
    let stride = frame.linesize[plane] as i64 / std::mem::size_of::<T>() as i64;

    let mut samples = Vec::with_capacity(rows * row_len);
    // SAFETY: FFmpeg allocated this plane for `linesize * rows` addressable bytes
    // and every read stays within `row_len` samples of its row start.
    unsafe {
        for row in 0..rows {
            let row_ptr = source.offset(row as isize * stride as isize);
            samples.extend_from_slice(std::slice::from_raw_parts(row_ptr, row_len));
        }
    }
    Ok(samples)
}

/// Writes `rows` rows of `row_len` samples each into one `AVFrame` plane.
///
/// The mirror of [`read_plane`], with the same one-row-tall shortcut for audio.
fn write_plane<T: ElementType>(
    frame: &mut AVFrame,
    plane: usize,
    rows: usize,
    row_len: usize,
    samples: &[T],
) -> Result<()> {
    if samples.len() < rows * row_len {
        return Err(RsmediaError::msg(format!(
            "Plane {plane} needs {} samples, got {}",
            rows * row_len,
            samples.len()
        )));
    }
    let destination = plane_ptr(frame, plane) as *mut T;
    let stride = frame.linesize[plane] as i64 / std::mem::size_of::<T>() as i64;

    // SAFETY: `alloc_buffer` sized this plane for `linesize * rows` writable bytes
    // and the frame outlives the copies; each write stays within `row_len`.
    unsafe {
        for row in 0..rows {
            std::ptr::copy_nonoverlapping(
                samples.as_ptr().add(row * row_len),
                destination.offset(row as isize * stride as isize),
                row_len,
            );
        }
    }
    Ok(())
}

/// Reads every plane of `layout` out of `frame` into a [`FrameData`].
fn read_samples<T: ElementType>(frame: &AVFrame, layout: &DataLayout) -> Result<FrameData<T>> {
    match layout {
        DataLayout::Interleaved {
            rows,
            cols,
            components,
        } => Ok(FrameData::Packed(
            Array3::from_shape_vec(
                (*rows, *cols, *components),
                read_plane::<T>(frame, 0, *rows, cols * components)?,
            )
            .map_err(|e| RsmediaError::msg(format!("Failed to build frame samples: {e}")))?,
        )),
        DataLayout::Planar(shapes) => {
            let mut planes = Vec::with_capacity(shapes.len());
            for (plane, &(rows, cols)) in shapes.iter().enumerate() {
                planes.push(
                    Array2::from_shape_vec(
                        (rows, cols),
                        read_plane::<T>(frame, plane, rows, cols)?,
                    )
                    .map_err(|e| RsmediaError::msg(format!("Failed to build frame plane: {e}")))?,
                );
            }
            Ok(FrameData::Planar(planes))
        }
    }
}

/// Writes every plane of `data` into an already allocated `frame`.
fn write_samples<T: ElementType>(frame: &mut AVFrame, data: &FrameData<T>) -> Result<()> {
    match data {
        FrameData::Packed(array) => {
            let (rows, cols, components) = array.dim();
            let samples = array
                .as_slice()
                .ok_or_else(|| RsmediaError::msg("Frame plane is not contiguous"))?;
            write_plane(frame, 0, rows, cols * components, samples)
        }
        FrameData::Planar(planes) => {
            for (plane, array) in planes.iter().enumerate() {
                let (rows, cols) = array.dim();
                let samples = array
                    .as_slice()
                    .ok_or_else(|| RsmediaError::msg("Frame plane is not contiguous"))?;
                write_plane(frame, plane, rows, cols, samples)?;
            }
            Ok(())
        }
    }
}

/// The `(height, width)` of a packed `RGB24` frame, validated against the layout
/// `RGB24` implies.
fn rgb24_extent<T>(data: &FrameData<T>) -> Result<(usize, usize)> {
    let packed = data.as_packed().ok_or_else(|| {
        RsmediaError::msg("RGB24 samples must be interleaved (packed), not planar")
    })?;
    let (height, width, _) = packed.dim();
    check_layout(data, PixelFormat::RGB24, width, height)?;
    Ok((height, width))
}

/// The `(height, width)` of a planar `YUV420P` frame, validated against the
/// layout `YUV420P` implies.
fn yuv420p_extent<T>(data: &FrameData<T>) -> Result<(usize, usize)> {
    let planes = data
        .as_planes()
        .ok_or_else(|| RsmediaError::msg("YUV420P samples must be planar, not interleaved"))?;
    let (height, width) = planes
        .first()
        .map(|plane| plane.dim())
        .ok_or_else(|| RsmediaError::msg("YUV420P frame has no luma plane"))?;
    check_layout(data, PixelFormat::YUV420P, width, height)?;
    Ok((height, width))
}

/// Checks `data` against the layout `format` requires at `width` x `height`.
fn check_layout<T>(
    data: &FrameData<T>,
    format: PixelFormat,
    width: usize,
    height: usize,
) -> Result<()> {
    let layout = format.data_layout(width, height).ok_or_else(|| {
        RsmediaError::msg(format!(
            "Pixel format {} has no data layout at {width}x{height}",
            format.get_pix_fmt_name()
        ))
    })?;
    if data.matches(&layout) {
        Ok(())
    } else {
        Err(RsmediaError::msg(format!(
            "{} expects planes {:?}, got {:?}",
            format.get_pix_fmt_name(),
            layout.shapes(),
            data.shapes()
        )))
    }
}

/// Builds a `(rows, cols)` plane from flat samples, converting `S` to `T`.
fn plane_from<S: ElementType, T: ElementType>(
    samples: Vec<S>,
    rows: usize,
    cols: usize,
) -> Result<Array2<T>> {
    Array2::from_shape_vec((rows, cols), cast_samples::<S, T>(samples))
        .map_err(|e| RsmediaError::msg(format!("Failed to build frame plane: {e}")))
}

/// Casts flat samples from `S` to `T`, falling back to zero for values the
/// conversion cannot represent.
fn cast_samples<S: ElementType, T: ElementType>(samples: Vec<S>) -> Vec<T> {
    samples
        .into_iter()
        .map(|value| num_traits::cast::<S, T>(value).unwrap_or(T::zero()))
        .collect()
}

/// 验证采样元素类型 `T` 的大小与格式要求的每样本字节数一致。
///
/// 两者不符时按 `T` 读写会越界（例如把 `u16` 样本写进 8bit 平面），因此这是
/// 所有跨 FFI 拷贝的前置条件。
fn validate_element_size<T>(format: FrameFormat, expected_size: usize) -> Result<()> {
    let type_size = std::mem::size_of::<T>();
    if type_size != expected_size {
        return Err(RsmediaError::msg(format!(
            "format:{format}, expected {expected_size}, got {type_size}"
        )));
    }
    Ok(())
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
        // 只跳过"既无载荷又无字典"的条目：`side_data_from_avframe` 会产出带字典的
        // 空载荷条目（FFmpeg 允许），丢掉它们会让 side data 无法往返。
        if entry.data.is_empty() && entry.metadata.is_empty() {
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
            if !entry.data.is_empty() {
                std::ptr::copy_nonoverlapping(entry.data.as_ptr(), (*raw).data, entry.data.len());
            }
            entry.metadata.write_into_raw_dict(&mut (*raw).metadata);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colors::Color;
    use std::time::Duration;

    const TEST_WIDTH: usize = 320;
    const TEST_HEIGHT: usize = 240;
    const TIME_BASE: ffi::AVRational = ffi::AVRational { num: 1, den: 30 }; // 30 fps

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

        let rgb = frame
            .data
            .as_packed_mut()
            .expect("RGB24 frames are interleaved");
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
                rgb[[y, x, 0]] = c.r();
                rgb[[y, x, 1]] = c.g();
                rgb[[y, x, 2]] = c.b();
            }
        }
        (r, g, b)
    }

    /// `PixelFormat::data_layout` 必须按描述符推导，而不是认识格式名。
    /// 这里锁定一批代表性格式的布局。
    #[test]
    fn test_pixel_format_data_layout() {
        let (width, height) = (64usize, 48usize);

        // 交错格式：单数组 `(height, width, 每像素元素数)`
        for (format, elements) in [
            (PixelFormat::GRAY8, 1),
            (PixelFormat::YUYV422, 2),
            (PixelFormat::UYVY422, 2),
            (PixelFormat::RGB24, 3),
            (PixelFormat::BGR24, 3),
            (PixelFormat::RGBA, 4),
            (PixelFormat::BGRA, 4),
        ] {
            assert_eq!(
                format.data_layout(width, height),
                Some(DataLayout::Interleaved {
                    rows: height,
                    cols: width,
                    components: elements,
                }),
                "{format:?}"
            );
        }

        // 平面格式：每分量一个平面，色度按 log2_chroma 右移
        assert_eq!(
            PixelFormat::YUV420P.data_layout(width, height),
            Some(DataLayout::Planar(vec![(48, 64), (24, 32), (24, 32)]))
        );
        assert_eq!(
            PixelFormat::YUV422P.data_layout(width, height),
            Some(DataLayout::Planar(vec![(48, 64), (48, 32), (48, 32)]))
        );
        assert_eq!(
            PixelFormat::YUV444P.data_layout(width, height),
            Some(DataLayout::Planar(vec![(48, 64), (48, 64), (48, 64)]))
        );
        // 平面 RGB：无色度抽样
        assert_eq!(
            PixelFormat::GBRP.data_layout(width, height),
            Some(DataLayout::Planar(vec![(48, 64), (48, 64), (48, 64)]))
        );
        // 半平面 NV12：色度平面 U/V 交错，故列数为 2 * ceil(w/2)
        assert_eq!(
            PixelFormat::NV12.data_layout(width, height),
            Some(DataLayout::Planar(vec![(48, 64), (24, 64)]))
        );
        // 10bit 平面：元素为 2 字节，形状仍是样本数
        assert_eq!(
            PixelFormat::YUV420P10LE.data_layout(width, height),
            Some(DataLayout::Planar(vec![(48, 64), (24, 32), (24, 32)]))
        );
        assert_eq!(PixelFormat::YUV420P10LE.bytes_per_component(), Some(2));
        assert_eq!(PixelFormat::YUV420P.bytes_per_component(), Some(1));

        // 奇数尺寸：色度按 ceil 右移，因此仍可表达
        assert_eq!(
            PixelFormat::YUV420P.data_layout(65, 49),
            Some(DataLayout::Planar(vec![(49, 65), (25, 33), (25, 33)]))
        );

        // 位流 / 调色板 / 硬件格式无法用整块样本数组表达
        assert_eq!(PixelFormat::MONOWHITE.data_layout(width, height), None);
        assert_eq!(PixelFormat::PAL8.data_layout(width, height), None);
        assert_eq!(PixelFormat::VAAPI.data_layout(width, height), None);
        assert_eq!(PixelFormat::NONE.data_layout(width, height), None);
        assert_eq!(PixelFormat::RGB24.data_layout(0, height), None);
    }

    /// 元素宽度不符的帧在**构造点**就被拒绝，而不是等到 `to_avframe`。
    ///
    /// 这条边界原先只在跨 FFI 拷贝时才检查，因此能造出一个永远无法编码的帧。
    #[test]
    fn test_element_width_checked_at_construction() {
        // S16 每样本 2 字节，u8 数据即使形状正确也不能构成 S16 帧。
        let audio = MediaFrame::<u8>::new_audio(
            SampleFormat::S16,
            2,
            8,
            44100,
            FrameData::from(Array3::<u8>::zeros((1, 8, 2))),
        );
        assert!(audio.is_err(), "u8 data must not build an S16 frame");

        // YUV420P10LE 的平面形状与 YUV420P 完全一致，只有元素宽度能区分两者，
        // 形状校验拦不住，必须靠宽度校验。
        let planes = vec![
            Array2::<u8>::zeros((4, 4)),
            Array2::<u8>::zeros((2, 2)),
            Array2::<u8>::zeros((2, 2)),
        ];
        let video =
            MediaFrame::<u8>::new_video(4, 4, PixelFormat::YUV420P10LE, FrameData::from(planes));
        assert!(video.is_err(), "u8 planes must not build a 10-bit frame");

        // 宽度匹配时照常通过：10bit 用 u16，8bit 用 u8。
        assert!(MediaFrame::<u16>::new_video_frame(4, 4, PixelFormat::YUV420P10LE).is_ok());
        assert!(MediaFrame::<u8>::new_video_frame(4, 4, PixelFormat::YUV420P).is_ok());
    }

    #[test]
    fn test_audio_data_layout() {
        // 平面格式：每声道一个 `(1, samples)` 平面
        assert_eq!(
            SampleFormat::FLTP.data_layout(2, 1024),
            DataLayout::Planar(vec![(1, 1024), (1, 1024)])
        );
        assert_eq!(SampleFormat::FLTP.data_layout(2, 1024).num_planes(), 2);

        // 交错格式：单个 `(1, samples, channels)` 数组
        assert_eq!(
            SampleFormat::S16.data_layout(2, 1024),
            DataLayout::Interleaved {
                rows: 1,
                cols: 1024,
                components: 2,
            }
        );
        assert_eq!(SampleFormat::S16.data_layout(2, 1024).num_planes(), 1);
    }

    #[test]
    fn test_frame_data_variants() -> Result<()> {
        let packed = FrameData::from(Array3::<u8>::zeros((4, 6, 3)));
        assert_eq!(packed.num_planes(), 1);
        assert_eq!(packed.as_packed().map(|a| a.dim()), Some((4, 6, 3)));
        assert!(packed.as_planes().is_none());
        // 交错帧的唯一平面把分量轴并入列；越界平面是"没有该平面"的错误
        assert_eq!(packed.plane(0).map(|p| p.dim()).ok(), Some((4, 18)));
        assert!(packed.plane(1).is_err());
        assert_eq!(packed.len(), 72);

        let planar = FrameData::from(vec![
            Array2::<u8>::zeros((4, 6)),
            Array2::<u8>::zeros((2, 3)),
        ]);
        assert_eq!(planar.num_planes(), 2);
        assert!(planar.as_packed().is_none());
        assert_eq!(planar.plane(1).map(|p| p.dim()).ok(), Some((2, 3)));
        assert!(planar.plane(2).is_err());
        assert_eq!(planar.len(), 30);
        assert_eq!(planar.shapes(), vec![(4, 6), (2, 3)]);

        // 空帧
        assert!(FrameData::<u8>::default().is_empty());

        Ok(())
    }

    #[test]
    fn test_frame_data_map_planes() -> Result<()> {
        // 交错帧经 map_planes 后仍是交错帧，形状不变
        let packed = FrameData::from(Array3::<u8>::from_elem((2, 3, 3), 4));
        let mapped =
            packed.map_planes(|_, plane| Ok(Array2::from_elem(plane.dim(), plane[[0, 0]] + 1)))?;
        assert_eq!(mapped.as_packed().map(|a| a.dim()), Some((2, 3, 3)));
        assert!(mapped.as_planes().is_none());
        assert_eq!(mapped.as_packed().unwrap()[[0, 0, 0]], 5);

        // 平面帧经 map_planes 后仍是平面帧
        let planar = FrameData::from(vec![
            Array2::<u8>::from_elem((2, 3), 1),
            Array2::<u8>::from_elem((1, 1), 2),
        ]);
        let mapped = planar.map_planes(|_, plane| Ok(plane.mapv(|v| v * 2)))?;
        assert_eq!(mapped.num_planes(), 2);
        assert_eq!(mapped.as_planes().unwrap()[0][[1, 2]], 2);

        Ok(())
    }

    #[test]
    fn test_frame_data_matches() -> Result<()> {
        let layout = DataLayout::Interleaved {
            rows: 4,
            cols: 6,
            components: 3,
        };
        assert!(FrameData::from(Array3::<u8>::zeros((4, 6, 3))).matches(&layout));
        assert!(!FrameData::from(Array3::<u8>::zeros((4, 6, 4))).matches(&layout));
        assert!(!FrameData::from(vec![Array2::<u8>::zeros((4, 6))]).matches(&layout));

        let layout = DataLayout::Planar(vec![(4, 6), (2, 3)]);
        assert!(
            FrameData::from(vec![
                Array2::<u8>::zeros((4, 6)),
                Array2::<u8>::zeros((2, 3))
            ])
            .matches(&layout)
        );
        assert!(!FrameData::from(vec![Array2::<u8>::zeros((4, 6))]).matches(&layout));

        Ok(())
    }

    #[test]
    fn test_frame_data_access() -> Result<()> {
        let mut frame =
            MediaFrame::<u8>::new_video_frame(TEST_WIDTH, TEST_HEIGHT, PixelFormat::RGB24)?;
        assert_eq!(frame.data.num_planes(), 1);
        assert_eq!(
            frame.data.as_packed().map(|a| a.dim()),
            Some((TEST_HEIGHT, TEST_WIDTH, 3))
        );

        // 测试数据访问和修改
        let (r, g, b) = fill_rgb_data(&mut frame, TEST_WIDTH, TEST_HEIGHT);

        // 验证数据正确性
        let rgb = frame.data.as_packed().unwrap();
        for y in 0..TEST_HEIGHT {
            for x in 0..TEST_WIDTH {
                let idx = y * TEST_WIDTH + x;
                assert_eq!(rgb[[y, x, 0]], r[idx]);
                assert_eq!(rgb[[y, x, 1]], g[idx]);
                assert_eq!(rgb[[y, x, 2]], b[idx]);
            }
        }

        Ok(())
    }

    #[test]
    fn test_different_pixel_types() -> Result<()> {
        // 元素类型是泛型参数，但必须与该格式的每样本字节数一致：
        // 8bit 格式配 u8、10bit 格式配 u16、浮点采样配 f32。
        let frame_u8 =
            MediaFrame::<u8>::new_video_frame(TEST_WIDTH, TEST_HEIGHT, PixelFormat::RGB24)?;
        assert_eq!(
            std::mem::size_of_val(&frame_u8.data.as_packed().unwrap()[[0, 0, 0]]),
            1
        );

        let frame_u16 =
            MediaFrame::<u16>::new_video_frame(TEST_WIDTH, TEST_HEIGHT, PixelFormat::YUV420P10LE)?;
        let luma = &frame_u16.data.as_planes().unwrap()[0];
        assert_eq!(std::mem::size_of_val(&luma[[0, 0]]), 2);

        let frame_f32 = MediaFrame::<f32>::new_audio_frame(SampleFormat::FLT, 2, 128, 44100)?;
        assert_eq!(
            std::mem::size_of_val(&frame_f32.data.as_packed().unwrap()[[0, 0, 0]]),
            4
        );

        Ok(())
    }

    /// 视频帧默认**不带**时间基：pts 的物理时间换算只在调用方显式声明时间基时才有
    /// 意义（`set_time_base` 的用途），否则编码器用自己的输入时间基解释 pts。
    #[test]
    fn test_frame_timestamps() -> Result<()> {
        let fps = TIME_BASE.den as f64 / TIME_BASE.num as f64;
        let frame_duration = Duration::from_secs_f64(1.0 / fps);

        let mut frames = Vec::new();
        for i in 0..5 {
            let mut frame =
                MediaFrame::<u8>::new_video_frame(TEST_WIDTH, TEST_HEIGHT, PixelFormat::RGB24)?;
            assert_eq!(
                frame.time_base.num, 0,
                "video frames start with no time base"
            );
            frame.set_time_base(TIME_BASE);
            frame.set_pts(i as i64);
            frames.push(frame);
        }

        // 验证时间戳的正确性（pts 按显式声明的 1/30 换算为物理时间）
        for (i, frame) in frames.iter().enumerate() {
            let expected_time = frame_duration * i as u32;
            let actual_time = Duration::from_secs_f64(
                frame.pts as f64 * frame.time_base.num as f64 / frame.time_base.den as f64,
            );
            assert!((actual_time - expected_time).as_secs_f32().abs() < 0.01);
        }

        Ok(())
    }

    /// `convert_to` covers the whole swscale format matrix, not just the
    /// `yuv`-crate's `RGB24` <-> `YUV420P` pair: every target keeps the geometry
    /// and reports the requested format, colour survives a round trip, and the
    /// two things it must *refuse* — an audio frame and a target with a
    /// different component width — are refused.
    #[test]
    fn test_convert_to_arbitrary_pixel_formats() -> Result<()> {
        let (width, height) = (16, 16);
        let mut frame = MediaFrame::<u8>::new_video_frame(width, height, PixelFormat::RGB24)?;
        let (r, g, b) = fill_rgb_data(&mut frame, width, height);

        // 8bit 家族的多种目标格式：几何不变、格式正确、布局自洽。
        for dst in [
            PixelFormat::RGB24,
            PixelFormat::BGR24,
            PixelFormat::RGBA,
            PixelFormat::YUV420P,
            PixelFormat::YUV422P,
            PixelFormat::YUV444P,
            PixelFormat::NV12,
            PixelFormat::GRAY8,
        ] {
            let converted = frame.convert_to(dst)?;
            assert_eq!(
                converted.format,
                FrameFormat::Pixel(dst),
                "target {dst:?} not reported back"
            );
            assert_eq!((converted.width, converted.height), (width, height));
            assert!(
                converted.data.matches(&converted.data_layout()?),
                "target {dst:?} produced a layout its own format disagrees with"
            );
        }

        // 经 YUV444P 回到 RGB24：色度未二次采样，误差应很小。
        let round_trip = frame
            .convert_to(PixelFormat::YUV444P)?
            .convert_to(PixelFormat::RGB24)?;
        let back = round_trip.data.as_packed().expect("RGB24 is interleaved");
        let max_error = (0..height)
            .flat_map(|y| (0..width).map(move |x| (y, x)))
            .flat_map(|(y, x)| {
                let idx = y * width + x;
                [
                    back[[y, x, 0]].abs_diff(r[idx]),
                    back[[y, x, 1]].abs_diff(g[idx]),
                    back[[y, x, 2]].abs_diff(b[idx]),
                ]
            })
            .max()
            .unwrap_or(0);
        assert!(
            max_error <= 2,
            "RGB24 -> YUV444P -> RGB24 drifted by {max_error} levels"
        );

        // 音频帧没有像素格式可换。
        let audio = MediaFrame::<f32>::new_audio_frame(SampleFormat::FLTP, 2, 128, 48_000)?;
        assert!(audio.convert_to(PixelFormat::RGB24).is_err());

        // 元素位宽不符：u8 帧不能产出 10bit 目标（否则会把半个平面读成垃圾）。
        let err = match frame.convert_to(PixelFormat::YUV420P10LE) {
            Ok(_) => panic!("a 10-bit target needs a 16-bit frame"),
            Err(e) => e,
        };
        assert!(
            err.to_string().contains("expected 2, got 1"),
            "unexpected error for a width mismatch: {err}"
        );

        // 已是指定格式时原样返回（无需转换）。
        let same = frame.convert_to(PixelFormat::RGB24)?;
        assert_eq!(same.data.as_packed().unwrap().dim(), (height, width, 3));

        Ok(())
    }

    #[test]
    fn test_create_rgb24_frame() -> Result<()> {
        let width = 640;
        let height = 360;

        let mut frame = MediaFrame::<u8>::new_video_frame(width, height, PixelFormat::RGB24)?;

        // 验证元数据与布局
        assert_eq!(
            frame.data.as_packed().expect("RGB24 is interleaved").dim(),
            (height, width, 3)
        );
        assert_eq!(frame.width, width);
        assert_eq!(frame.height, height);
        assert_eq!(frame.format, FrameFormat::Pixel(PixelFormat::RGB24));
        // 视频帧不带时间基：分辨率里没有帧率信息，编码器会用自身的输入时间基解释 pts。
        assert_eq!(
            frame.time_base.num, 0,
            "video frames start with no time base"
        );
        assert!(
            frame.data.as_packed().unwrap().is_standard_layout(),
            "RGB24 应为行主序连续布局"
        );

        // 填充并读回验证
        let rgb = frame.data.as_packed_mut().unwrap();
        for y in 0..height {
            for x in 0..width {
                rgb[[y, x, 0]] = (x % 255) as u8;
                rgb[[y, x, 1]] = (y % 255) as u8;
                rgb[[y, x, 2]] = ((x + y) % 255) as u8;
            }
        }
        let rgb = frame.data.as_packed().unwrap();
        for y in 0..height {
            for x in 0..width {
                assert_eq!(rgb[[y, x, 0]], (x % 255) as u8);
                assert_eq!(rgb[[y, x, 1]], (y % 255) as u8);
                assert_eq!(rgb[[y, x, 2]], ((x + y) % 255) as u8);
            }
        }

        Ok(())
    }

    #[test]
    fn test_create_yuv420p_planes() -> Result<()> {
        let frame =
            MediaFrame::<u8>::new_video_frame(TEST_WIDTH, TEST_HEIGHT, PixelFormat::YUV420P)?;

        // 平面原生尺寸：Y 满分辨率，U/V 各半尺寸（不复制成 2x2 块）
        let planes = frame.data.as_planes().expect("YUV420P is planar");
        assert_eq!(planes.len(), 3);
        assert_eq!(planes[0].dim(), (TEST_HEIGHT, TEST_WIDTH));
        assert_eq!(planes[1].dim(), (TEST_HEIGHT / 2, TEST_WIDTH / 2));
        assert_eq!(planes[2].dim(), (TEST_HEIGHT / 2, TEST_WIDTH / 2));
        assert_eq!(
            frame.data.len(),
            TEST_WIDTH * TEST_HEIGHT + TEST_WIDTH * TEST_HEIGHT / 2
        );

        Ok(())
    }

    #[test]
    fn test_video_layout_validation() -> Result<()> {
        // 分量数不对
        assert!(
            MediaFrame::<u8>::new_video(
                16,
                16,
                PixelFormat::RGB24,
                Array3::<u8>::zeros((16, 16, 4))
            )
            .is_err()
        );

        // 尺寸不对
        assert!(
            MediaFrame::<u8>::new_video(
                16,
                16,
                PixelFormat::RGB24,
                Array3::<u8>::zeros((16, 32, 3))
            )
            .is_err()
        );

        // 平面格式收到交错数据
        assert!(
            MediaFrame::<u8>::new_video(
                16,
                16,
                PixelFormat::YUV420P,
                Array3::<u8>::zeros((16, 16, 3))
            )
            .is_err()
        );

        // 交错格式收到平面数据
        assert!(
            MediaFrame::<u8>::new_video(
                16,
                16,
                PixelFormat::RGB24,
                vec![Array2::<u8>::zeros((16, 16))]
            )
            .is_err()
        );

        // 正确布局应通过
        assert!(
            MediaFrame::<u8>::new_video(
                16,
                16,
                PixelFormat::RGB24,
                Array3::<u8>::zeros((16, 16, 3))
            )
            .is_ok()
        );

        Ok(())
    }

    #[test]
    fn test_create_audio_frame() -> Result<()> {
        let samples = 1024;
        let channels = 2;
        let sample_rate = 44100;

        // FLTP 是平面格式：每个声道一个 `(1, nb_samples)` 平面
        let mut frame =
            MediaFrame::<f32>::new_audio_frame(SampleFormat::FLTP, channels, samples, sample_rate)?;

        // 音频帧的时间基由采样率推出，无需调用方传递。
        assert_eq!((frame.time_base.num, frame.time_base.den), (1, 44100));

        assert_eq!(frame.data.num_planes(), channels as usize);
        assert_eq!(
            frame.data.as_planes().unwrap()[0].dim(),
            (1, samples as usize)
        );
        assert!(!frame.data.is_empty());

        // 填充一些测试数据
        let planes = frame.data.as_planes_mut().unwrap();
        for (ch, plane) in planes.iter_mut().enumerate() {
            for s in 0..samples as usize {
                // 生成简单的正弦波，不同通道使用不同频率
                let t = s as f32 / sample_rate as f32;
                let freq = 440.0 * (ch + 1) as f32;
                plane[[0, s]] = (2.0 * std::f32::consts::PI * freq * t).sin();
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
    fn test_create_interleaved_audio_frame() -> Result<()> {
        // S16 是交错格式：单个 `(1, nb_samples, nb_channels)` 数组
        let frame = MediaFrame::<i16>::new_audio_frame(SampleFormat::S16, 2, 480, 48000)?;
        assert_eq!(
            frame.data.as_packed().expect("S16 is interleaved").dim(),
            (1, 480, 2)
        );
        assert!(frame.data.as_planes().is_none());

        Ok(())
    }

    #[test]
    fn test_audio_layout_validation() -> Result<()> {
        // FLTP 需要每声道一个平面，交错数组应被拒绝
        assert!(
            MediaFrame::<f32>::new_audio(
                SampleFormat::FLTP,
                2,
                16,
                48000,
                Array3::<f32>::zeros((1, 16, 2))
            )
            .is_err()
        );

        // 交错格式收到平面数据也不行
        assert!(
            MediaFrame::<f32>::new_audio(
                SampleFormat::FLT,
                2,
                16,
                48000,
                vec![Array2::<f32>::zeros((1, 16)), Array2::<f32>::zeros((1, 16))]
            )
            .is_err()
        );

        // 平面数与声道数不符
        assert!(
            MediaFrame::<f32>::new_audio(
                SampleFormat::FLTP,
                2,
                16,
                48000,
                vec![Array2::<f32>::zeros((1, 16))]
            )
            .is_err()
        );

        Ok(())
    }

    #[test]
    fn test_format_getter() -> Result<()> {
        // 视频帧
        let video = MediaFrame::<u8>::new_video_frame(TEST_WIDTH, TEST_HEIGHT, PixelFormat::RGB24)?;
        match video.format() {
            Some(FrameFormat::Pixel(PixelFormat::RGB24)) => {}
            other => panic!("video format = {other:?}"),
        }

        // 音频帧
        let audio = MediaFrame::<f32>::new_audio_frame(SampleFormat::FLTP, 2, 16, 48000)?;
        match audio.format() {
            Some(FrameFormat::Sample(SampleFormat::FLTP)) => {}
            other => panic!("audio format = {other:?}"),
        }

        Ok(())
    }
}
