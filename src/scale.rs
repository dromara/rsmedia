use crate::error::{Context, Result, RsmediaError};
use crate::{PixelFormat, imgutils};
use rsmpeg::avutil::AVBufferPool;

use rsmpeg::avutil::AVFrame;
use rsmpeg::ffi;
use rsmpeg::{UnsafeDerefMut, swscale::SwsContext};

// FFmpeg `SwsFlags` 定义参考: <https://ffmpeg.org/doxygen/trunk/group__libsws.html>
//
// 版本注记（依据 data/ffmpeg-*/binding.rs）：
// - 6/7：`SWS_*` 是裸 `u32` 常量；`SwsContext` 不透明，无字段可设。
// - 8：常量类型化为 `ffi::SwsFlags`；`SwsContext` 暴露 flags/threads/dither/
//   alpha_blend/intent，并新增 `SWS_STRICT`(1<<11)、`SWS_UNSTABLE`(1<<20)、
//   `SWS_DITHER_*`、`SWS_ALPHA_BLEND_*`、`SWS_INTENT_*`。
// - 9：再新增 `SwsContext.scaler`/`scaler_sub`/`backends` 与 `SWS_SCALE_*`、
//   `SWS_BACKEND_*`（`SWS_SCALE_*` 在 8 中完全不存在）。
//   SWS_STRICT         1 << 11   Return an error on underspecified conversions.
//   SWS_PRINT_INFO     1 << 12   Emit verbose log of scaling parameters.
//   SWS_FULL_CHR_H_INT 1 << 13   Perform full chroma upsampling when upscaling to RGB.
//   SWS_FULL_CHR_H_INP 1 << 14   Perform full chroma interpolation when downscaling RGB.
//   SWS_ACCURATE_RND   1 << 18   Force bit-exact output rounding.
//   SWS_BITEXACT       1 << 19   Disable platform-specific optimizations for bit-exactness.
//   SWS_UNSTABLE       1 << 20   Prefer experimental code paths.
//   SWS_DIRECT_BGR     1 << 15   Deprecated: no effect.
//   SWS_ERROR_DIFFUSION 1 << 23   Deprecated: set `SwsDither` instead.
//   SWS_FAST_BILINEAR  1 <<  0   fast bilinear filtering
//   SWS_BILINEAR       1 <<  1   bilinear filtering
//   SWS_BICUBIC        1 <<  2   2-tap cubic B-spline
//   SWS_X              1 <<  3   experimental
//   SWS_POINT          1 <<  4   nearest neighbor
//   SWS_AREA           1 <<  5   area averaging
//   SWS_BICUBLIN       1 <<  6   bicubic luma, bilinear chroma
//   SWS_GAUSS          1 <<  7   gaussian approximation
//   SWS_SINC           1 <<  8   unwindowed sinc
//   SWS_LANCZOS        1 <<  9   3-tap sinc/sinc
//   SWS_SPLINE         1 << 10   unwindowed natural cubic spline
ffi_enum!(
    /// Video scaler algorithm selector (SWS_* algorithm bits, 1 << 0 .. 1 << 10).
    ///
    /// Represents the scaling kernel choice — **mutually exclusive** (pick one),
    /// but the enum still implements `BitOr`/`Into<u32>` so it can be combined
    /// with non-algorithm quality flags (`SWS_FULL_CHR_H_INT`, `SWS_ACCURATE_RND`,
    /// `SWS_BITEXACT`, …) before handing the assembled mask to FFI.
    #[allow(non_camel_case_types)]
    ScaleAlgorithm, u32 {
        /// fast bilinear filtering
        FAST_BILINEAR => ffi::SWS_FAST_BILINEAR;
        /// bilinear filtering
        BILINEAR => ffi::SWS_BILINEAR;
        /// 2-tap cubic B-spline
        BICUBIC => ffi::SWS_BICUBIC;
        /// experimental
        X => ffi::SWS_X;
        /// nearest neighbor
        POINT => ffi::SWS_POINT;
        /// area averaging
        AREA => ffi::SWS_AREA;
        /// bicubic luma, bilinear chroma
        BICUBLIN => ffi::SWS_BICUBLIN;
        /// gaussian approximation
        GAUSS => ffi::SWS_GAUSS;
        /// unwindowed sinc
        SINC => ffi::SWS_SINC;
        /// 3-tap sinc/sinc
        LANCZOS => ffi::SWS_LANCZOS;
        /// unwindowed natural cubic spline
        SPLINE => ffi::SWS_SPLINE;
    }
);

#[allow(clippy::derivable_impls)]
impl Default for ScaleAlgorithm {
    fn default() -> Self {
        Self::BICUBIC
    }
}

ffi_enum!(
    /// Video scaler quality / behaviour flags (SWS_* non-algorithm bits, 1 << 11 .. 1 << 20).
    ///
    /// FFmpeg splits `SwsContext.flags` into two groups, and its header documents the rule
    /// for each: the scaler selection options (see [`ScaleAlgorithm`]) are *"Scaler selection
    /// options. Only one may be active at a time."*, while the remaining bits are a plain
    /// **bitmask** (`SwsContext.flags` is documented as "Bitmask of SWS_*"), so **any subset
    /// of this type may be combined**.
    ///
    /// Consequently a quality **mask**, not a single variant, is what the scaler takes:
    /// combine the named bits with `BitOr` (which yields the raw `u32` mask, per this
    /// crate's flag-set convention) and pass the result to [`Scaler::new_with_options`],
    /// e.g. `ScaleQuality::FULL_CHR_H_INT | ScaleQuality::ACCURATE_RND |
    /// ScaleQuality::BITEXACT` — the baseline FFmpeg recommends, available as
    /// [`ScaleQuality::default_mask`]. The header explicitly notes that `ACCURATE_RND` and
    /// `BITEXACT` are meant to be set together.
    ///
    /// Deprecated `SWS_DIRECT_BGR` / `SWS_ERROR_DIFFUSION` (no effect) are intentionally
    /// **not** modelled here.
    #[allow(non_camel_case_types)]
    ScaleQuality, u32 {
        /// Return an error on underspecified conversions. (FFmpeg 8+ only.)
        #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
        STRICT => ffi::SWS_STRICT;
        /// Emit verbose log of scaling parameters.
        PRINT_INFO => ffi::SWS_PRINT_INFO;
        /// Perform full chroma upsampling when upscaling to RGB.
        FULL_CHR_H_INT => ffi::SWS_FULL_CHR_H_INT;
        /// Perform full chroma interpolation when downscaling RGB.
        FULL_CHR_H_INP => ffi::SWS_FULL_CHR_H_INP;
        /// Force bit-exact output rounding.
        ACCURATE_RND => ffi::SWS_ACCURATE_RND;
        /// Disable platform-specific optimizations for bit-exactness.
        BITEXACT => ffi::SWS_BITEXACT;
        /// Prefer experimental code paths. (FFmpeg 8+)
        #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
        UNSTABLE => ffi::SWS_UNSTABLE;
    }
);

impl ScaleQuality {
    /// Combine a set of quality bits into the mask handed to FFmpeg.
    ///
    /// This is the sanctioned way to spell a quality set: every element is a real
    /// [`ScaleQuality`] variant, so an invalid flag cannot be expressed (an empty list
    /// means "no quality bits").
    ///
    /// ```
    /// use rsmedia::ScaleQuality;
    ///
    /// let mask = ScaleQuality::mask([ScaleQuality::ACCURATE_RND, ScaleQuality::BITEXACT]);
    /// assert_eq!(
    ///     mask,
    ///     ScaleQuality::ACCURATE_RND.as_raw() | ScaleQuality::BITEXACT.as_raw()
    /// );
    /// // Slices and vectors work too.
    /// assert_eq!(
    ///     ScaleQuality::mask(&[ScaleQuality::BITEXACT]),
    ///     ScaleQuality::BITEXACT.as_raw()
    /// );
    /// ```
    pub fn mask(bits: impl AsRef<[ScaleQuality]>) -> u32 {
        bits.as_ref()
            .iter()
            .fold(0, |mask, bit| mask | bit.as_raw())
    }

    /// The default quality bits, in the list form the scaler takes: full chroma
    /// upsampling when upscaling to RGB plus platform-independent bit-exact output.
    /// The header notes that `ACCURATE_RND` and `BITEXACT` are meant to be set together.
    pub fn default_quality() -> [ScaleQuality; 3] {
        [Self::FULL_CHR_H_INT, Self::ACCURATE_RND, Self::BITEXACT]
    }

    /// [`ScaleQuality::default_quality`] as a raw mask
    /// (`FULL_CHR_H_INT | ACCURATE_RND | BITEXACT`) — the baseline that
    /// [`Scaler::new`] uses and [`ScaleQuality::default_mask`] appends.
    pub fn default_mask() -> u32 {
        Self::mask(Self::default_quality())
    }
}

#[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
ffi_enum_wrap_from!(
    /// How to dither when reducing colour depth (maps to `SwsContext.dither`, FFmpeg 8+).
    ///
    /// See the official libswscale documentation for `SwsDither`.
    #[allow(non_camel_case_types)]
    SwsDither => ffi::SwsDither,
    repr = u32,
    fallback = panic {
        /// No dithering.
        NONE => ffi::SWS_DITHER_NONE;
        /// Automatic dithering.
        AUTO => ffi::SWS_DITHER_AUTO;
        /// Ordered dithering by a Bayer matrix.
        BAYER => ffi::SWS_DITHER_BAYER;
        /// Error diffusion.
        ED => ffi::SWS_DITHER_ED;
        /// Arithmetic dithering.
        A_DITHER => ffi::SWS_DITHER_A_DITHER;
        /// XOR-based dithering.
        X_DITHER => ffi::SWS_DITHER_X_DITHER;
    }
);

#[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
ffi_enum_wrap_from!(
    /// How source-alpha is blended onto the destination when the destination has an
    /// alpha channel (maps to `SwsContext.alpha_blend`, FFmpeg 8).
    ///
    /// See the official `SwsAlphaBlend` enumeration.
    #[allow(non_camel_case_types)]
    SwsAlphaBlend => ffi::SwsAlphaBlend,
    repr = u32,
    fallback = panic {
        /// No blending, overwrite with the source.
        NONE => ffi::SWS_ALPHA_BLEND_NONE;
        /// Alpha-blend with a uniform source alpha for the whole image.
        UNIFORM => ffi::SWS_ALPHA_BLEND_UNIFORM;
        /// Alpha-blend with a checkerboard pattern.
        CHECKERBOARD => ffi::SWS_ALPHA_BLEND_CHECKERBOARD;
    }
);

#[cfg(feature = "ffmpeg9")]
ffi_enum_wrap_from!(
    /// Explicit selection of the scaling filter (maps to `SwsContext.scaler` / `scaler_sub`,
    /// **FFmpeg 9+**: neither the fields nor `SWS_SCALE_*` exist in FFmpeg 8, whose
    /// `SwsContext` only carries `scaler_params`). When set to anything other than `AUTO`,
    /// it overrides the filter implied by `SwsContext.flags` (i.e. by [`ScaleAlgorithm`]).
    ///
    /// See the official `SwsScaler` enumeration.
    #[allow(non_camel_case_types)]
    SwsScaler => ffi::SwsScaler,
    repr = u32,
    fallback = panic {
        /// Auto-select the scaling filter from `SwsContext.flags`.
        AUTO => ffi::SWS_SCALE_AUTO;
        /// Fast bilinear filtering.
        BILINEAR => ffi::SWS_SCALE_BILINEAR;
        /// 2-tap cubic B-spline.
        BICUBIC => ffi::SWS_SCALE_BICUBIC;
        /// Nearest-neighbour.
        POINT => ffi::SWS_SCALE_POINT;
        /// Area averaging.
        AREA => ffi::SWS_SCALE_AREA;
        /// Gaussian approximation.
        GAUSSIAN => ffi::SWS_SCALE_GAUSSIAN;
        /// Unwindowed sinc.
        SINC => ffi::SWS_SCALE_SINC;
        /// 3-tap sinc/sinc.
        LANCZOS => ffi::SWS_SCALE_LANCZOS;
        /// Unwindowed natural cubic spline.
        SPLINE => ffi::SWS_SCALE_SPLINE;
    }
);

#[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
ffi_enum_wrap_from!(
    /// Intent for colour conversions (maps to `SwsContext.intent`, FFmpeg 8).
    ///
    /// See the official `SwsIntent` enumeration.
    #[allow(non_camel_case_types)]
    SwsIntent => ffi::SwsIntent,
    repr = u32,
    fallback = panic {
        /// Perceptual intent.
        PERCEPTUAL => ffi::SWS_INTENT_PERCEPTUAL;
        /// Relative colorimetric intent.
        RELATIVE_COLORIMETRIC => ffi::SWS_INTENT_RELATIVE_COLORIMETRIC;
        /// Saturation intent.
        SATURATION => ffi::SWS_INTENT_SATURATION;
        /// Absolute colorimetric intent.
        ABSOLUTE_COLORIMETRIC => ffi::SWS_INTENT_ABSOLUTE_COLORIMETRIC;
    }
);

#[cfg(feature = "ffmpeg9")]
ffi_enum_wrap_from!(
    /// Hardware/software implementation backend selector (maps to `SwsContext.backends`,
    /// FFmpeg 9+ only). Like the other swscale IDs, this is a mutually-exclusive selection,
    /// and it fails fast on an unlisted value.
    ///
    /// Official constants (both `SWS_BACKEND_LEGACY` and `SWS_BACKEND_STABLE` share value `1`
    /// — same-value aliases, so only `LEGACY` is modelled as a variant and both hit it).
    #[allow(non_camel_case_types)]
    SwsBackend => ffi::SwsBackend,
    repr = u32,
    fallback = panic {
        /// Legacy (stateful) API backend; `SWS_BACKEND_STABLE` is the same value (alias).
        LEGACY => ffi::SWS_BACKEND_LEGACY;
        /// Portable C backend.
        C => ffi::SWS_BACKEND_C;
        /// Memory-copy backend (no filtering).
        MEMCPY => ffi::SWS_BACKEND_MEMCPY;
        /// x86-optimised backend.
        X86 => ffi::SWS_BACKEND_X86;
        /// AArch64-optimised backend.
        AARCH64 => ffi::SWS_BACKEND_AARCH64;
        /// SPIR-V / compute-shader backend.
        SPIRV => ffi::SWS_BACKEND_SPIRV;
        /// Any unstable (experimental) backend.
        UNSTABLE => ffi::SWS_BACKEND_UNSTABLE;
        /// All backends.
        ALL => ffi::SWS_BACKEND_ALL;
    }
);

/// 创建软件缩放上下文（按 FFmpeg 版本走新旧 API 路径）：
/// - FFmpeg 6/7：legacy 路径，`sws_getContext()` 一次性传入源/目标参数完成初始化；
/// - FFmpeg 8+：modern 全动态路径，`sws_alloc_context()` 分配后仅设置 flags 字段，
///   尺寸/格式等参数由 `sws_scale_frame()` 从帧属性推导（`sws_init_context()` 自
///   FFmpeg 8.0 起废弃，FFmpeg 9 起拒绝 legacy/modern API 混用）。
#[cfg(any(feature = "ffmpeg6", feature = "ffmpeg7"))]
fn setup_scaler(
    src_width: i32,
    src_height: i32,
    src_pix_fmt: ffi::AVPixelFormat,
    dst_width: i32,
    dst_height: i32,
    dst_pix_fmt: ffi::AVPixelFormat,
    flags: u32,
) -> Result<SwsContext> {
    SwsContext::get_context(
        src_width,
        src_height,
        src_pix_fmt,
        dst_width,
        dst_height,
        dst_pix_fmt,
        flags,
        None,
        None,
        None,
    )
    .context("Failed to create swscale context.")
}

/// FFmpeg 8+ 的全动态路径：参数签名与 6/7 分支保持一致以便调用方无感切换，
/// 除 flags 外的参数（尺寸/格式）由 [`SwsContext::scale_full_frame`] 从帧属性推导，
#[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
#[allow(unused_variables)]
fn setup_scaler(
    src_width: i32,
    src_height: i32,
    src_pix_fmt: ffi::AVPixelFormat,
    dst_width: i32,
    dst_height: i32,
    dst_pix_fmt: ffi::AVPixelFormat,
    flags: u32,
) -> Result<SwsContext> {
    let mut sws_ctx = SwsContext::alloc().context("Failed to allocate swscale context.")?;
    sws_ctx.set_flags(flags);
    Ok(sws_ctx)
}

/// Scales one frame into a new frame, with the default kernel and quality mask.
///
/// Free-function form for a single conversion; a stream should keep a [`Scaler`]
/// instead, which reuses its `SwsContext` (and, optionally, the output buffers)
/// across frames. See [`Scaler::scale_frame`] for the details.
pub fn scale_frame(
    src_frame: &AVFrame,
    dst_width: i32,
    dst_height: i32,
    dst_pix_fmt: PixelFormat,
) -> Result<AVFrame> {
    // Delegates to a one-off `Scaler` so there is exactly one implementation of
    // the actual scaling; a caller who needs another kernel builds a `Scaler`
    // itself (`Scaler::new_with_options`).
    Scaler::new().scale_frame(src_frame, dst_width, dst_height, dst_pix_fmt)
}

/// Persistent streaming video scaler, held by the encoder and the decoder.
///
/// Unlike the free functions [`scale_frame`] / `scale_with_flags` (which create a
/// temporary `SwsContext` on every call), `Scaler` owns the scaling **policy** — one
/// [`ScaleAlgorithm`] kernel plus a mask of [`ScaleQuality`] bits — and keeps a matching
/// `SwsContext` alive across calls, so a continuous stream does not pay for a context
/// allocation per frame.
///
/// The context is bound **lazily**: [`Scaler::new`] allocates nothing; the first
/// [`Scaler::scale_frame`] call creates the context from that frame's source geometry and
/// pixel format plus the destination geometry and pixel format requested for the call.
/// Whenever a later frame needs a different source/destination geometry or format the
/// context is rebuilt, so a stream whose resolution or format changes mid-way keeps
/// scaling correctly.
///
/// Because the destination is supplied per call, one `Scaler` serves both the encoder
/// path (same size, conversion into the codec's pixel format) and the decoder path (resize
/// plus format conversion). [`Encoder`](crate::Encoder) and [`Decoder`](crate::Decoder)
/// each hold one, built inside the builder's `build` from the `with_scale_algorithm` /
/// `with_scale_quality` options; the defaults are the policy below.
///
/// Destination frames are allocated with `alloc_buffer` (a fresh allocation per call)
/// unless pooling is enabled via [`Scaler::with_buffer_pool`]; the pool then recycles the
/// buffers of previously dropped frames, so a steady stream of same-geometry output stops
/// allocating after a couple of frames — see [`BufferPool`](rsmpeg::avutil::AVBufferPool).
///
/// The policy mirrors FFmpeg's own flag rules: [`ScaleAlgorithm`] selects exactly one
/// scaling kernel (*"Only one may be active at a time."*), while the quality bits are a
/// mask of which any subset may be set — see [`ScaleQuality`].
pub struct Scaler {
    /// The (mutually exclusive) scaling kernel selector.
    algorithm: ScaleAlgorithm,
    /// Quality/behaviour bit mask; zero or more [`ScaleQuality`] bits.
    quality: u32,
    /// Context bound on first use, together with the parameters it was created for.
    bound: Option<BoundScaler>,
    /// Whether destination frames are allocated from an internal [`BufferPool`](rsmpeg::avutil::AVBufferPool)
    /// (created per bound context, see [`BoundScaler::pool`]).
    pool_enabled: bool,
}

/// A live `SwsContext` plus the source/destination parameters it was created with.
///
/// The parameters are needed to detect that a later frame requires a rebuild. On FFmpeg
/// 6/7 the legacy context fixes them at creation; on FFmpeg 8+ the context derives them
/// from the frames, and the comparison keeps both versions behaving identically.
///
/// The pool, when pooling is enabled, is created together with the context and sized for
/// exactly these destination parameters — a geometry/format change rebuilds both, and the
/// old pool is then dropped (`av_buffer_pool_uninit` marks it for destruction, so buffers
/// still held by previously returned frames are freed instead of recycled).
struct BoundScaler {
    sws: SwsContext,
    src_width: i32,
    src_height: i32,
    src_pix_fmt: PixelFormat,
    dst_width: i32,
    dst_height: i32,
    dst_pix_fmt: PixelFormat,
    pool: Option<AVBufferPool>,
}

impl Scaler {
    /// Create a scaler with the default policy: the default kernel
    /// ([`ScaleAlgorithm::default`], BICUBIC) and the FFmpeg-recommended quality mask
    /// ([`ScaleQuality::default_mask`]).
    pub fn new() -> Self {
        Self::new_with_options(ScaleAlgorithm::default(), ScaleQuality::default_quality())
    }

    /// Create a scaler with an explicit kernel and quality bits.
    ///
    /// `quality` is the set of quality/behaviour bits to apply, given as a list of
    /// [`ScaleQuality`] values — one bit, several, or an empty list for none. Every
    /// element is a real variant, so no invalid flag can be passed.
    ///
    /// ```
    /// use rsmedia::{ScaleAlgorithm, ScaleQuality, Scaler};
    ///
    /// // One algorithm bit (mutually exclusive) plus a set of quality bits.
    /// let scaler = Scaler::new_with_options(
    ///     ScaleAlgorithm::LANCZOS,
    ///     [ScaleQuality::FULL_CHR_H_INT, ScaleQuality::ACCURATE_RND, ScaleQuality::BITEXACT],
    /// );
    /// assert_eq!(scaler.algorithm(), ScaleAlgorithm::LANCZOS);
    /// assert_eq!(scaler.quality(), ScaleQuality::default_mask());
    /// ```
    pub fn new_with_options(
        algorithm: ScaleAlgorithm,
        quality: impl AsRef<[ScaleQuality]>,
    ) -> Self {
        Self {
            algorithm,
            quality: ScaleQuality::mask(quality),
            bound: None,
            pool_enabled: false,
        }
    }

    /// Enable (`true`) or disable (`false`) pooled allocation of destination frames.
    ///
    /// With pooling on, the destination frame's pixel buffer is taken from an internal
    /// [`BufferPool`](rsmpeg::avutil::AVBufferPool) instead of being freshly allocated per call; when
    /// a previously returned frame is dropped, its buffer goes back to the pool and the
    /// next same-geometry call reuses it. A steady stream of same-geometry output thus
    /// stops allocating after a couple of frames, and the buffers are zero-filled exactly
    /// like `alloc_buffer`'s, so padding bytes stay deterministic for the encoder.
    ///
    /// The pool is created lazily together with the scaling context and sized for the
    /// bound destination geometry; a geometry/format change rebuilds it. This is a
    /// construction-time setting — it consumes and returns the scaler — so the pool is
    /// always in place before the first [`Self::scale_frame`].
    ///
    /// ```rust
    /// use rsmedia::Scaler;
    ///
    /// let scaler = Scaler::new().with_buffer_pool(true);
    /// assert!(scaler.pool_enabled());
    /// ```
    pub fn with_buffer_pool(mut self, enabled: bool) -> Self {
        self.pool_enabled = enabled;
        self
    }

    /// Whether pooled destination-frame allocation is enabled
    /// (see [`Self::with_buffer_pool`]).
    pub fn pool_enabled(&self) -> bool {
        self.pool_enabled
    }

    /// The scaling kernel this scaler was configured with.
    pub fn algorithm(&self) -> ScaleAlgorithm {
        self.algorithm
    }

    /// The quality/behaviour bit mask this scaler was configured with (possibly several
    /// bits; test individual bits against `ScaleQuality::X.as_raw()`).
    pub fn quality(&self) -> u32 {
        self.quality
    }

    /// The combined algorithm + quality mask handed to FFmpeg.
    pub fn flags(&self) -> u32 {
        self.algorithm.as_raw() | self.quality
    }

    /// Scale a frame into a newly allocated destination frame of `dst_width` ×
    /// `dst_height` in `dst_pix_fmt`.
    ///
    /// The source geometry and pixel format come from `src_frame`. The scaling context is
    /// created on the first call and rebuilt whenever either side's geometry or pixel
    /// format changes.
    pub fn scale_frame(
        &mut self,
        src_frame: &AVFrame,
        dst_width: i32,
        dst_height: i32,
        dst_pix_fmt: PixelFormat,
    ) -> Result<AVFrame> {
        if !src_frame.hw_frames_ctx.is_null() {
            return Err(RsmediaError::unsupported(
                "Hardware frames are not supported in this software scaler",
            ));
        }

        // 帧的格式来自解码器，可能超出本 crate 收录的范围：报错而不是 panic。
        let src_pix_fmt = PixelFormat::from_ffi_checked(src_frame.format).ok_or_else(|| {
            RsmediaError::unsupported(format!(
                "Unsupported source pixel format {} on a {}x{} frame",
                src_frame.format, src_frame.width, src_frame.height
            ))
        })?;
        let reusable = self.bound.as_ref().is_some_and(|bound| {
            bound.src_width == src_frame.width
                && bound.src_height == src_frame.height
                && bound.src_pix_fmt == src_pix_fmt
                && bound.dst_width == dst_width
                && bound.dst_height == dst_height
                && bound.dst_pix_fmt == dst_pix_fmt
        });
        if !reusable {
            // FFmpeg 6/7 的 legacy `SwsContext` 在此固定源/目标几何与格式；FFmpeg 8+
            // 的动态上下文只取 flags，几何在缩放时由帧属性推导——两条路径都按这里的
            // 目标参数调用，故两个版本的行为保持一致。
            let sws = setup_scaler(
                src_frame.width,
                src_frame.height,
                src_frame.format,
                dst_width,
                dst_height,
                dst_pix_fmt.into(),
                self.flags(),
            )?;
            // 池与上下文同生命周期：按本次绑定的目标几何建池，几何/格式变化
            // 重建时旧池一并析构（未归还的缓冲由引用计数安全释放）。
            let pool = if self.pool_enabled {
                Some(AVBufferPool::new(pooled_frame_buffer_size(
                    dst_pix_fmt,
                    dst_width,
                    dst_height,
                )?)?)
            } else {
                None
            };
            self.bound = Some(BoundScaler {
                sws,
                src_width: src_frame.width,
                src_height: src_frame.height,
                src_pix_fmt,
                dst_width,
                dst_height,
                dst_pix_fmt,
                pool,
            });
        }
        let bound = self.bound.as_mut().expect("bound above");

        let mut dst_frame = match bound.pool.as_mut() {
            Some(pool) => alloc_pooled_frame(pool, dst_width, dst_height, dst_pix_fmt)?,
            None => {
                let mut dst_frame = AVFrame::new();
                dst_frame.set_width(dst_width);
                dst_frame.set_height(dst_height);
                dst_frame.set_format(dst_pix_fmt.into());
                dst_frame
                    .alloc_buffer()
                    .context("Failed to allocate destination frame buffer")?;
                dst_frame
            }
        };
        imgutils::copy_frame_metadata(src_frame, &mut dst_frame, false)?;

        #[cfg(any(feature = "ffmpeg6", feature = "ffmpeg7"))]
        {
            let ret = unsafe {
                ffi::sws_scale_frame(
                    bound.sws.as_mut_ptr(),
                    dst_frame.as_mut_ptr(),
                    src_frame.as_ptr(),
                )
            };
            if ret < 0 {
                return Err(RsmediaError::msg(format!(
                    "Failed to call sws_scale_frame, ret: {ret}"
                )));
            }
        }

        #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
        bound
            .sws
            .scale_full_frame(&mut dst_frame, src_frame)
            .context("Failed to scale frame.")?;

        log::debug!(
            "Sws scale from src:[{}x{}, {:?}] to dst:[{}x{}, {:?}]",
            src_frame.width,
            src_frame.height,
            src_pix_fmt,
            dst_width,
            dst_height,
            dst_pix_fmt
        );

        Ok(dst_frame)
    }

    /// Like [`Scaler::scale_frame`], but takes ownership of `src`.
    ///
    /// When the source already matches the requested destination format **and**
    /// dimensions, `src` is returned unchanged (zero-cost, no allocation);
    /// otherwise it is converted via the persistent context into a newly
    /// allocated destination frame.
    ///
    /// This centralises the "convert only when the pixel format / geometry
    /// differ" short-circuit that would otherwise be duplicated across the
    /// decode and encode pipelines.
    pub fn scale_if_needed(
        &mut self,
        src: AVFrame,
        dst_width: i32,
        dst_height: i32,
        dst_pix_fmt: PixelFormat,
    ) -> Result<AVFrame> {
        if src.format == i32::from(dst_pix_fmt)
            && src.width == dst_width
            && src.height == dst_height
        {
            return Ok(src);
        }
        self.scale_frame(&src, dst_width, dst_height, dst_pix_fmt)
    }
}

impl Default for Scaler {
    fn default() -> Self {
        Self::new()
    }
}

/// Only the policy is caller-visible, so `Debug` reports it along with whether a context
/// has been bound and whether pooling is on — `SwsContext` itself implements no `Debug`.
impl std::fmt::Debug for Scaler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scaler")
            .field("algorithm", &self.algorithm)
            .field("quality", &format!("{:#x}", self.quality))
            .field("bound", &self.bound.is_some())
            .field("pool_enabled", &self.pool_enabled)
            .finish()
    }
}

/// 池化帧的 stride 对齐（字节）。与 `av_frame_get_buffer` 的默认视频对齐一致，
/// 编码器内部的 SIMD 读取路径按此假设优化。
const POOL_ALIGN: i32 = 32;

/// 池化帧缓冲的额外留白（字节）。`av_image_fill_arrays` 只要求
/// `av_image_get_buffer_size(align)` 的精确尺寸，多留一点以覆盖
/// `av_frame_get_buffer` 同样会加的 padding 余量，防御 SIMD 越界读。
const POOL_PADDING: usize = 64;

/// 计算池化帧缓冲所需尺寸：`av_image_get_buffer_size`（与
/// [`alloc_pooled_frame`] 使用的 `av_image_fill_arrays` 同一 `align`，
/// 两者互为镜像）加上安全留白。
fn pooled_frame_buffer_size(fmt: PixelFormat, width: i32, height: i32) -> Result<usize> {
    let size = unsafe { ffi::av_image_get_buffer_size(fmt.into(), width, height, POOL_ALIGN) };
    if size < 0 {
        return Err(RsmediaError::invalid_config(format!(
            "cannot size a pooled frame buffer for {fmt:?} {width}x{height}: \
             av_image_get_buffer_size returned {size}"
        )));
    }
    Ok(size as usize + POOL_PADDING)
}

/// 从池中取缓冲并组装一个可写入的目标帧。
///
/// 帧的所有平面指针由 `av_image_fill_arrays` 按 `POOL_ALIGN` 对齐指向池缓冲
/// 内部；缓冲所有权移交给 `frame.buf[0]`——帧被 unref（或引用计数归零）时，
/// 缓冲自动归还池（池已析构则直接释放）。
///
/// FFmpeg 的池在**复用**时不会重新清零缓冲（与 `av_frame_get_buffer` 的
/// "每次清零分配"不同），这里在组装帧前显式清零整个缓冲：memset 的代价
/// 远小于一次 malloc，换来与 `alloc_buffer` 完全一致的跨平台语义——帧的
/// padding 字节内容确定为零，编码器内部的 SIMD 读取路径不受脏数据影响。
fn alloc_pooled_frame(
    pool: &mut AVBufferPool,
    width: i32,
    height: i32,
    fmt: PixelFormat,
) -> Result<AVFrame> {
    let mut frame = AVFrame::new();
    frame.set_width(width);
    frame.set_height(height);
    frame.set_format(fmt.into());

    let buffer = pool
        .get()
        .context("Failed to get a buffer from the frame pool")?;
    // Safety: buffer.data 有效且长度为 buffer.size
    // 整段清零写是合法的独占访问（引用计数为 1，无其他持有者）。
    unsafe {
        std::ptr::write_bytes((*buffer.as_ptr()).data, 0, (*buffer.as_ptr()).size);
    }

    // 池缓冲的起始地址对齐由 FFmpeg 的 pool allocator 决定，跨平台不保证
    // 32 字节（Windows 上 `av_malloc` 通常仅 16 对齐）。而编码器的 SIMD
    // 读取依赖平面指针 32 字节对齐（与 `av_frame_get_buffer` 的默认一致），
    // 故在缓冲内部把数据基准偏移到下一个 32 字节边界后，再交给
    // `av_image_fill_arrays` 铺排平面——这样 `data[0]` 恒为 32 对齐。
    // offset ∈ [0, 31]，`POOL_PADDING` 足以覆盖；`av_image_fill_arrays` 的
    // 排布随之从对齐后的起点延续，不越界。
    let base = unsafe { (*buffer.as_ptr()).data as usize };
    let offset = (POOL_ALIGN as usize - (base % POOL_ALIGN as usize)) % POOL_ALIGN as usize;
    // Safety: offset ∈ [0,31] 落在池缓冲内部（POOL_PADDING=64 足够覆盖）。
    let aligned = unsafe { (*buffer.as_ptr()).data.add(offset) as *const u8 };

    let mut data = [std::ptr::null_mut::<u8>(); 8];
    let mut linesize = [0i32; 8];
    // Safety: aligned 指向池缓冲内部（尺寸 ≥ pooled_frame_buffer_size 的结果
    // + 对齐偏移），data/linesize 是本地数组，参数均为 FFmpeg 要求的合法值。
    let ret = unsafe {
        ffi::av_image_fill_arrays(
            data.as_mut_ptr(),
            linesize.as_mut_ptr(),
            aligned,
            fmt.into(),
            width,
            height,
            POOL_ALIGN,
        )
    };
    if ret < 0 {
        return Err(RsmediaError::msg(format!(
            "av_image_fill_arrays failed for {fmt:?} {width}x{height}, ret: {ret}"
        )));
    }

    // Safety: frame 由本函数刚构造，无其他引用；rsmpeg 的 wrap 不实现
    // DerefMut，字段写入经 UnsafeDerefMut::deref_mut 完成。
    let raw = unsafe { frame.deref_mut() };
    raw.data = data;
    raw.linesize = linesize;
    // 视频帧约定 extended_data == data（av_frame_get_buffer 同样如此设置）。
    raw.extended_data = raw.data.as_mut_ptr();
    // Safety: 所有权整体移交（引用计数本就为 1），帧 Drop 时由
    // av_frame_unref 归还/释放。
    raw.buf[0] = buffer.into_raw().as_ptr();
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PixelFormat;
    use crate::error::{Context, Result};
    use rsmpeg::avutil::AVFrame;

    fn create_test_frame(width: i32, height: i32, pix_fmt: PixelFormat) -> Result<AVFrame> {
        let mut frame = AVFrame::new();
        frame.set_width(width);
        frame.set_height(height);
        frame.set_format(pix_fmt.into());
        frame
            .alloc_buffer()
            .context("Failed to allocate frame buffer")?;
        Ok(frame)
    }

    #[test]
    fn test_scale_frame_video() -> Result<()> {
        // 64x64 YUV420P -> 32x32 RGB24
        let src = create_test_frame(64, 64, PixelFormat::YUV420P)?;
        // fill with non-zero pattern
        unsafe {
            for plane in 0..3usize {
                let height = if plane == 0 { 64 } else { 32 };
                let data = std::slice::from_raw_parts_mut(
                    src.data[plane],
                    src.linesize[plane] as usize * height as usize,
                );
                for (i, b) in data.iter_mut().enumerate() {
                    *b = (i % 251) as u8;
                }
            }
        }

        let dst = scale_frame(&src, 32, 32, PixelFormat::RGB24).context("scale failed")?;

        assert_eq!(dst.width, 32);
        assert_eq!(dst.height, 32);
        assert_eq!(dst.format, i32::from(PixelFormat::RGB24));
        Ok(())
    }

    /// 逐平面比较两个同几何/同格式帧的**可见像素内容**（忽略 stride 与
    /// padding 差异）：YUV420P 按 luma + 两个色度平面，RGB24 按行。
    fn assert_visible_pixels_equal(
        a: &AVFrame,
        b: &AVFrame,
        width: u32,
        height: u32,
        fmt: PixelFormat,
    ) {
        let rows = |frame: &AVFrame, plane: usize, rows: u32, row_bytes: u32| {
            (0..rows as usize)
                .map(|y| unsafe {
                    std::slice::from_raw_parts(
                        frame.data[plane].add(y * frame.linesize[plane] as usize),
                        row_bytes as usize,
                    )
                })
                .collect::<Vec<_>>()
        };

        let planes: &[(usize, u32, u32)] = match fmt {
            // (plane, rows, row_bytes)
            PixelFormat::YUV420P => &[
                (0, height, width),
                (1, height / 2, width / 2),
                (2, height / 2, width / 2),
            ],
            PixelFormat::RGB24 => &[(0, height, width * 3)],
            other => panic!("unhandled format in test helper: {other:?}"),
        };

        for &(plane, rows_count, row_bytes) in planes {
            let a_rows = rows(a, plane, rows_count, row_bytes);
            let b_rows = rows(b, plane, rows_count, row_bytes);
            for (y, (ra, rb)) in a_rows.iter().zip(b_rows.iter()).enumerate() {
                assert_eq!(ra, rb, "plane {plane} row {y} differs");
            }
        }
    }

    /// 池化帧必须能被下游通过 `av_frame_ref` 安全引用（编码器
    /// `avcodec_send_frame` 内部正是这样引用帧的）：引用后共享同一缓冲、
    /// buf[0] 引用计数 +1，释放后恢复。
    #[test]
    fn test_scaler_pool_frame_supports_ffmpeg_ref() -> Result<()> {
        let mut scaler = Scaler::new().with_buffer_pool(true);
        let src = create_test_frame(64, 64, PixelFormat::YUV420P)?;
        let frame = scaler.scale_frame(&src, 32, 32, PixelFormat::YUV420P)?;
        assert!(!frame.buf[0].is_null());

        let mut retained = AVFrame::new();
        // Safety: 两帧均为有效 AVFrame，av_frame_ref 的标准用法。
        let ret = unsafe { ffi::av_frame_ref(retained.as_mut_ptr(), frame.as_ptr()) };
        assert_eq!(ret, 0, "av_frame_ref failed: {ret}");
        assert_eq!(retained.data[0], frame.data[0], "引用共享同一缓冲");
        assert_eq!(retained.linesize[0], frame.linesize[0]);

        // Safety: buf[0] 来自成功的 scale_frame，非空。
        let count = unsafe { ffi::av_buffer_get_ref_count(frame.buf[0]) };
        assert_eq!(count, 2, "frame + retained 应各持一个引用");

        // Safety: retained 由 av_frame_ref 成功创建。
        unsafe { ffi::av_frame_unref(retained.as_mut_ptr()) };
        // Safety: 同上，buf[0] 仍被 frame 持有。
        let count = unsafe { ffi::av_buffer_get_ref_count(frame.buf[0]) };
        assert_eq!(count, 1);
        Ok(())
    }

    /// 池化帧的缓冲复用：归还后必须复用同一缓冲（同指针），持有中的帧
    /// 强制池拿新缓冲，全部归还后两次取回应恰好落回 b/c 的两个缓冲。
    #[test]
    fn test_scaler_frame_pool_recycles_buffers() -> Result<()> {
        let mut scaler = Scaler::new().with_buffer_pool(true);
        assert!(scaler.pool_enabled());

        let src = create_test_frame(64, 64, PixelFormat::YUV420P)?;

        // 第 1 帧：真实分配。
        let a = scaler.scale_frame(&src, 32, 32, PixelFormat::YUV420P)?;
        assert!(!a.buf[0].is_null(), "池化帧必须持有 buf[0]");
        assert!(a.is_allocated());
        let ptr_a = a.data[0];

        // 归还后第 2 帧：必须复用同一缓冲（同数据指针）。
        drop(a);
        let b = scaler.scale_frame(&src, 32, 32, PixelFormat::YUV420P)?;
        assert_eq!(b.data[0], ptr_a, "复用的缓冲数据指针应与上一帧相同");

        // b 仍存活：第 3 帧必须拿新缓冲。
        let c = scaler.scale_frame(&src, 32, 32, PixelFormat::YUV420P)?;
        assert_ne!(c.data[0], b.data[0]);

        // 全部归还后：两次取回应恰好是 b/c 的两个缓冲。
        let ptr_b = b.data[0];
        let ptr_c = c.data[0];
        drop(b);
        drop(c);
        let d = scaler.scale_frame(&src, 32, 32, PixelFormat::YUV420P)?;
        let e = scaler.scale_frame(&src, 32, 32, PixelFormat::YUV420P)?;
        let got = [d.data[0], e.data[0]];
        assert!(
            (got[0] == ptr_b && got[1] == ptr_c) || (got[0] == ptr_c && got[1] == ptr_b),
            "归还后的两次取回应复用 {ptr_b:?}/{ptr_c:?}，实际 {got:?}"
        );
        Ok(())
    }

    /// 池化输出与 `alloc_buffer` 输出的像素内容必须完全一致（同源帧、同
    /// 缩放策略），覆盖 YUV420P（多平面）与 RGB24（packed）两种布局。
    #[test]
    fn test_scaler_pool_output_matches_non_pool() -> Result<()> {
        for (src_fmt, dst_fmt, (sw, sh), (dw, dh)) in [
            (
                PixelFormat::YUV420P,
                PixelFormat::YUV420P,
                (64, 48),
                (32, 24),
            ),
            (PixelFormat::YUV420P, PixelFormat::RGB24, (64, 48), (32, 24)),
        ] {
            let src = create_test_frame(sw, sh, src_fmt)?;
            // 用非零图案填充，避免全零帧掩盖拷贝/错位问题。
            unsafe {
                let total = src.linesize[0] as usize * sh as usize;
                std::ptr::write_bytes(src.data[0], 0x5A, total);
            }

            let mut pooled = Scaler::new().with_buffer_pool(true);
            let mut plain = Scaler::new();

            let a = pooled.scale_frame(&src, dw, dh, dst_fmt)?;
            let b = plain.scale_frame(&src, dw, dh, dst_fmt)?;
            assert_eq!((a.width, a.height), (dw, dh));
            assert_eq!(a.format, b.format);
            assert_eq!(a.linesize[0] % 32, 0, "池化帧 stride 应按 32 对齐");
            assert_visible_pixels_equal(&a, &b, dw as u32, dh as u32, dst_fmt);

            // 复用后的缓冲内容同样正确（先归还 a，再缩放一帧比对）。
            drop(a);
            let a2 = pooled.scale_frame(&src, dw, dh, dst_fmt)?;
            assert_visible_pixels_equal(&a2, &b, dw as u32, dh as u32, dst_fmt);
        }
        Ok(())
    }

    /// 几何/格式变化时池随上下文重建：新旧两组几何的输出都正确。重建后
    /// 新池按新尺寸重新分配，这里只做**内容/几何**的确定性校验——注意不能
    /// 断言新缓冲指针与旧指针不同：旧池析构后其 malloc 地址会被系统分配器
    /// 立即复用，指针相等与否不是池重建的可观测属性。
    #[test]
    fn test_scaler_pool_rebuilds_on_geometry_change() -> Result<()> {
        let mut scaler = Scaler::new().with_buffer_pool(true);
        let mut plain = Scaler::new();
        let src_small = create_test_frame(64, 64, PixelFormat::YUV420P)?;
        let src_mid = create_test_frame(48, 48, PixelFormat::YUV420P)?;

        // 建立旧池并产出小尺寸帧。
        let a = scaler.scale_frame(&src_small, 32, 32, PixelFormat::YUV420P)?;
        assert_eq!((a.width, a.height), (32, 32));
        drop(a);

        // 几何变化：旧池析构、新池按 24x20 尺寸重建。
        let b = scaler.scale_frame(&src_mid, 24, 20, PixelFormat::RGB24)?;
        assert_eq!((b.width, b.height), (24, 20));
        assert_eq!(b.format, i32::from(PixelFormat::RGB24));
        let reference = plain.scale_frame(&src_mid, 24, 20, PixelFormat::RGB24)?;
        assert_visible_pixels_equal(&b, &reference, 24, 20, PixelFormat::RGB24);

        // 新几何再取一帧，内容依旧正确（旧几何的缓冲不再影响新池）。
        let c = scaler.scale_frame(&src_mid, 24, 20, PixelFormat::RGB24)?;
        assert_eq!((c.width, c.height), (24, 20));
        assert_visible_pixels_equal(&c, &reference, 24, 20, PixelFormat::RGB24);
        Ok(())
    }

    /// 稳态流水线特性：连续 50 帧同几何缩放，真实分配次数必须停留在
    /// 极小值（≤2）——这是池化生效、热路径不再逐帧 malloc 的直接证据。
    #[test]
    fn test_scaler_pool_steady_state_stops_allocating() -> Result<()> {
        let mut scaler = Scaler::new().with_buffer_pool(true);
        let src = create_test_frame(64, 64, PixelFormat::YUV420P)?;

        let mut seen = std::collections::HashSet::new();
        for _ in 0..50 {
            let frame = scaler.scale_frame(&src, 32, 32, PixelFormat::YUV420P)?;
            assert_eq!((frame.width, frame.height), (32, 32));
            seen.insert(frame.data[0] as usize);
            drop(frame); // 每帧用完即归还
        }
        assert!(
            seen.len() <= 2,
            "50 帧稳态流水的去重缓冲指针数应 ≤2，实际 {}",
            seen.len()
        );
        Ok(())
    }

    /// 安全性：**复用的缓冲必须清零**。FFmpeg 的池归还时不重置内容，而
    /// `alloc_buffer` 保证帧 padding 为零——池化路径在使用前显式清零整个
    /// 缓冲，本测试把整个缓冲写满垃圾、归还、再缩放，断言可见像素正确且
    /// 全部 padding（行间隙 + 平面间隙 + 尾部留白）为 0。
    #[test]
    fn test_scaler_pool_padding_zeroed_on_reuse() -> Result<()> {
        let mut scaler = Scaler::new().with_buffer_pool(true);
        let src = create_test_frame(64, 48, PixelFormat::YUV420P)?;
        unsafe {
            std::ptr::write_bytes(src.data[0], 0x3C, src.linesize[0] as usize * 48);
        }

        // 第一帧：把它的整个池缓冲写满垃圾再归还。
        let first = scaler.scale_frame(&src, 32, 24, PixelFormat::YUV420P)?;
        let buf_size = unsafe { (*first.buf[0]).size };
        unsafe {
            std::ptr::write_bytes((*first.buf[0]).data, 0xFF, buf_size);
        }
        assert!(buf_size > 0);
        drop(first);

        // 第二帧（复用同一缓冲）：可见像素必须正确，padding 必须为 0。
        let second = scaler.scale_frame(&src, 32, 24, PixelFormat::YUV420P)?;
        let mut plain = Scaler::new();
        let reference = plain.scale_frame(&src, 32, 24, PixelFormat::YUV420P)?;
        assert_visible_pixels_equal(&second, &reference, 32, 24, PixelFormat::YUV420P);

        // 行间隙：YUV420P luma 行内 width..linesize 必须全零。
        let (w, h) = (32usize, 24usize);
        for y in 0..h {
            let row_gap = unsafe {
                std::slice::from_raw_parts(
                    second.data[0].add(y * second.linesize[0] as usize + w),
                    second.linesize[0] as usize - w,
                )
            };
            assert!(row_gap.iter().all(|&b| b == 0), "luma 行 {y} 的间隙非零");
        }
        // 尾部留白（POOL_PADDING=64）必须全零。注意不能用 `second.data[0]`
        // 定位尾部：`alloc_pooled_frame` 会把数据基准在缓冲内部偏移到 32 字节
        // 边界（跨平台不保证池缓冲对齐），因此 `data[0]` 可能 ≠ 缓冲起始地址。
        // 缓冲的尾部留白恒在 `buf[0].data + size - 64`，即池缓冲的物理末尾。
        let second_buf_size = unsafe { (*second.buf[0]).size };
        let tail = unsafe {
            std::slice::from_raw_parts((*second.buf[0]).data.add(second_buf_size - 64), 64)
        };
        assert!(tail.iter().all(|&b| b == 0), "缓冲尾部留白非零");
        Ok(())
    }

    /// 安全性：池化帧的缓冲指针至少 32 字节对齐（`av_malloc` 的跨平台
    /// 保证下限），`av_image_fill_arrays` 的平面排布与编码器 SIMD 读取
    /// 都依赖这一点。
    #[test]
    fn test_scaler_pool_frame_alignment() -> Result<()> {
        let mut scaler = Scaler::new().with_buffer_pool(true);
        let src = create_test_frame(64, 64, PixelFormat::YUV420P)?;
        for _ in 0..4 {
            let frame = scaler.scale_frame(&src, 32, 32, PixelFormat::YUV420P)?;
            let addr = frame.data[0] as usize;
            assert_eq!(addr % 32, 0, "pooled frame data at {addr:#x} not aligned");
            // 平面 1/2 的指针同样对齐（fill_arrays 在缓冲内按 align 排布）。
            let addr_uv = frame.data[1] as usize;
            assert_eq!(addr_uv % 32, 0, "chroma plane at {addr_uv:#x} not aligned");
        }
        Ok(())
    }

    /// 安全性：几何变化重建池时，**旧池的未归还缓冲**必须安全存活到
    /// 归零（av_buffer_pool_uninit 的延迟析构语义），随后新池继续工作。
    #[test]
    fn test_scaler_pool_outstanding_buffer_survives_rebuild() -> Result<()> {
        let mut scaler = Scaler::new().with_buffer_pool(true);
        let src_small = create_test_frame(64, 64, PixelFormat::YUV420P)?;
        let src_mid = create_test_frame(48, 48, PixelFormat::YUV420P)?;

        // 旧池的帧，故意不归还。
        let outstanding = scaler.scale_frame(&src_small, 32, 32, PixelFormat::YUV420P)?;
        let old_ptr = outstanding.data[0];

        // 几何变化：旧池析构（outstanding 仍持有其缓冲），新池建立。
        let b = scaler.scale_frame(&src_mid, 24, 20, PixelFormat::RGB24)?;
        assert_eq!((b.width, b.height), (24, 20));
        drop(b);

        // 旧帧此时才归还：缓冲不属于任何活池，直接释放（不回到新池）。
        drop(outstanding);

        // 新池继续正常工作：新几何的帧不受影响。
        let c = scaler.scale_frame(&src_mid, 24, 20, PixelFormat::RGB24)?;
        assert_eq!((c.width, c.height), (24, 20));
        assert_ne!(c.data[0], old_ptr, "新池的缓冲不应与旧池缓冲混淆");
        Ok(())
    }

    /// 质量位是**集合**：`Scaler` 接收若干具名位并合成掩码（算法位仍只能有一个）。
    #[test]
    fn test_scaler_quality_mask_accepts_multiple_bits() -> Result<()> {
        // 多个质量位 = FFmpeg 建议的基线。
        let scaler = Scaler::new_with_options(
            ScaleAlgorithm::LANCZOS,
            [
                ScaleQuality::FULL_CHR_H_INT,
                ScaleQuality::ACCURATE_RND,
                ScaleQuality::BITEXACT,
            ],
        );
        assert_eq!(scaler.algorithm(), ScaleAlgorithm::LANCZOS);
        assert_eq!(scaler.quality(), ScaleQuality::default_mask());
        assert_eq!(
            scaler.flags(),
            (ScaleAlgorithm::LANCZOS.as_raw() | ScaleQuality::default_mask()),
        );

        // 单个位、切片、空集合（= 无质量位）。
        let single = Scaler::new_with_options(ScaleAlgorithm::AREA, [ScaleQuality::BITEXACT]);
        assert_eq!(single.quality(), ScaleQuality::BITEXACT.as_raw());
        let slice = ScaleQuality::default_quality();
        let from_slice = Scaler::new_with_options(ScaleAlgorithm::BICUBLIN, &slice[..1]);
        assert_eq!(from_slice.quality(), ScaleQuality::FULL_CHR_H_INT.as_raw());
        let none = Scaler::new_with_options(ScaleAlgorithm::POINT, []);
        assert_eq!(none.quality(), 0);

        // 默认构造 = 默认算法 + 默认质量掩码。
        let mut default = Scaler::new();
        assert_eq!(default.algorithm(), ScaleAlgorithm::default());
        assert_eq!(default.quality(), ScaleQuality::default_mask());

        // 掩码确实送到 FFmpeg：默认策略下可正常缩放。
        let dst = default.scale_frame(
            &create_test_frame(64, 64, PixelFormat::YUV420P)?,
            16,
            16,
            PixelFormat::RGB24,
        )?;
        assert_eq!((dst.width, dst.height), (16, 16));
        assert_eq!(dst.format, i32::from(PixelFormat::RGB24));
        Ok(())
    }

    /// 同一个 `Scaler` 的上下文惰性绑定、随源/目标几何变化自动重建。
    #[test]
    fn test_scaler_rebinds_when_geometry_changes() -> Result<()> {
        let mut scaler = Scaler::new_with_options(
            ScaleAlgorithm::BILINEAR,
            [ScaleQuality::ACCURATE_RND, ScaleQuality::BITEXACT],
        );

        // 首帧：绑定上下文。
        let first = scaler.scale_frame(
            &create_test_frame(64, 64, PixelFormat::YUV420P)?,
            32,
            32,
            PixelFormat::RGB24,
        )?;
        assert_eq!((first.width, first.height), (32, 32));

        // 第二帧：源与目标几何都变了，必须重建后仍然正确。
        let second = scaler.scale_frame(
            &create_test_frame(48, 48, PixelFormat::YUV420P)?,
            24,
            20,
            PixelFormat::RGB24,
        )?;
        assert_eq!((second.width, second.height), (24, 20));
        assert_eq!(second.format, i32::from(PixelFormat::RGB24));

        // 回到第一组参数：仍可复用（重建不影响后续调用）。
        let third = scaler.scale_frame(
            &create_test_frame(64, 64, PixelFormat::YUV420P)?,
            32,
            32,
            PixelFormat::RGB24,
        )?;
        assert_eq!((third.width, third.height), (32, 32));
        Ok(())
    }

    /// `scale_if_needed` returns the source frame unchanged when the target
    /// format and dimensions already match (zero-cost no-op), and converts
    /// only when they differ.
    #[test]
    fn test_scale_if_needed_noops_on_matching_format_and_size() -> Result<()> {
        let mut scaler = Scaler::new();

        // 源已是目标格式与尺寸 → 应原样返回（no-op）。
        let matching = scaler.scale_if_needed(
            create_test_frame(64, 64, PixelFormat::RGB24)?,
            64,
            64,
            PixelFormat::RGB24,
        )?;
        assert_eq!((matching.width, matching.height), (64, 64));
        assert_eq!(matching.format, i32::from(PixelFormat::RGB24));

        // 仅尺寸不同 → 必须缩放。
        let resized = scaler.scale_if_needed(
            create_test_frame(64, 64, PixelFormat::RGB24)?,
            32,
            32,
            PixelFormat::RGB24,
        )?;
        assert_eq!((resized.width, resized.height), (32, 32));

        // 仅格式不同 → 必须转换。
        let conv = scaler.scale_if_needed(
            create_test_frame(64, 64, PixelFormat::YUV420P)?,
            64,
            64,
            PixelFormat::RGB24,
        )?;
        assert_eq!((conv.width, conv.height), (64, 64));
        assert_eq!(conv.format, i32::from(PixelFormat::RGB24));
        Ok(())
    }

    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    #[test]
    fn test_scale_modern_options() -> Result<()> {
        use rsmpeg::swscale::SwsContext;

        let src = create_test_frame(64, 64, PixelFormat::YUV420P)?;
        unsafe {
            for plane in 0..3usize {
                let height = if plane == 0 { 64 } else { 32 };
                let data = std::slice::from_raw_parts_mut(
                    src.data[plane],
                    src.linesize[plane] as usize * height as usize,
                );
                for (i, b) in data.iter_mut().enumerate() {
                    *b = (i % 251) as u8;
                }
            }
        }

        let mut ctx = SwsContext::alloc().context("allocate sws context")?;
        ctx.set_flags(ScaleAlgorithm::LANCZOS.as_raw() | ScaleQuality::default_mask());
        ctx.set_threads(0);
        ctx.set_dither(ffi::SWS_DITHER_AUTO);
        ctx.set_alpha_blend(ffi::SWS_ALPHA_BLEND_NONE);
        #[cfg(feature = "ffmpeg9")]
        {
            ctx.set_scaler(ffi::SWS_SCALE_BICUBIC);
            ctx.set_backends(ffi::SWS_BACKEND_ALL);
        }

        let mut dst = create_test_frame(32, 32, PixelFormat::RGB24)?;
        ctx.scale_full_frame(&mut dst, &src)
            .context("scale failed")?;

        assert_eq!(dst.width, 32);
        assert_eq!(dst.height, 32);
        assert_eq!(dst.format, i32::from(PixelFormat::RGB24));
        Ok(())
    }
}
