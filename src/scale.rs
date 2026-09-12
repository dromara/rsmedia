use crate::error::{Context, Result, RsmediaError};
use crate::{PixelFormat, imgutils};

use rsmpeg::avutil::AVFrame;
use rsmpeg::ffi;
use rsmpeg::swscale::SwsContext;

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
    /// [`Scaler::new`] uses and [`ScaleAlgorithm::default_mask`] appends.
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

/// # Safety
///
/// ffi::sws_scale_frame
pub fn scale_frame(
    src_frame: &AVFrame,
    dst_width: i32,
    dst_height: i32,
    dst_pix_fmt: PixelFormat,
) -> Result<AVFrame> {
    scale_with_flags(
        src_frame,
        dst_width,
        dst_height,
        dst_pix_fmt,
        ScaleAlgorithm::default(),
        ScaleQuality::default_quality(),
    )
}

/// # Safety
///
/// ffi::sws_scale_frame
///
/// `quality` is the set of quality/behaviour bits to apply — the algorithm is one bit,
/// the quality flags are a combinable set, so this takes a list of [`ScaleQuality`]
/// values (empty for none).
fn scale_with_flags(
    src_frame: &AVFrame,
    dst_width: i32,
    dst_height: i32,
    dst_pix_fmt: PixelFormat,
    algorithm: ScaleAlgorithm,
    quality: impl AsRef<[ScaleQuality]>,
) -> Result<AVFrame> {
    if !src_frame.hw_frames_ctx.is_null() {
        return Err(RsmediaError::unsupported(
            "Hardware frames are not supported in this software scaler",
        ));
    }

    let flags = algorithm.as_raw() | ScaleQuality::mask(quality);
    let mut dst_frame = AVFrame::new();
    dst_frame.set_width(dst_width);
    dst_frame.set_height(dst_height);
    dst_frame.set_format(dst_pix_fmt.into());
    dst_frame
        .alloc_buffer()
        .context("Failed to allocate destination frame buffer")?;
    imgutils::copy_frame_metadata(src_frame, &mut dst_frame, false)?;
    let mut sws_ctx = setup_scaler(
        src_frame.width,
        src_frame.height,
        src_frame.format,
        dst_width,
        dst_height,
        dst_pix_fmt.into(),
        flags,
    )
    .context("Failed to create swscale context.")?;

    // FFmpeg 6/7：legacy 初始化的上下文直调 `sws_scale_frame`（对已初始化上下文属
    // 向后兼容用法）；FFmpeg 8+：全动态上下文必须走 modern 封装
    // [`SwsContext::scale_full_frame`]，FFmpeg 9 起对未初始化的上下文直调底层
    // `sws_scale` 会因新旧 API 混用而拒绝（AVERROR EINVAL）。
    #[cfg(any(feature = "ffmpeg6", feature = "ffmpeg7"))]
    {
        let ret = unsafe {
            ffi::sws_scale_frame(
                sws_ctx.as_mut_ptr(),
                dst_frame.as_mut_ptr(),
                src_frame.as_ptr(),
            )
        };
        if ret < 0 {
            return Err(RsmediaError::custom(format!(
                "Failed to call sws_scale_frame, ret: {ret}"
            )));
        }
    }

    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    sws_ctx
        .scale_full_frame(&mut dst_frame, src_frame)
        .context("Failed to scale frame.")?;

    log::debug!(
        "Sws scale from src:[{}x{}, {:?}] to dst:[{}x{}, {:?}]",
        src_frame.width,
        src_frame.height,
        PixelFormat::from(src_frame.format),
        dst_width,
        dst_height,
        dst_pix_fmt
    );

    Ok(dst_frame)
}

/// Persistent streaming video scaler, held by the encoder and the decoder.
///
/// Unlike the free functions [`scale_frame`] / [`scale_with_flags`] (which create a
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
}

/// A live `SwsContext` plus the source/destination parameters it was created with.
///
/// The parameters are needed to detect that a later frame requires a rebuild. On FFmpeg
/// 6/7 the legacy context fixes them at creation; on FFmpeg 8+ the context derives them
/// from the frames, and the comparison keeps both versions behaving identically.
struct BoundScaler {
    sws: SwsContext,
    src_width: i32,
    src_height: i32,
    src_pix_fmt: PixelFormat,
    dst_width: i32,
    dst_height: i32,
    dst_pix_fmt: PixelFormat,
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
        }
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

        let src_pix_fmt = PixelFormat::from(src_frame.format);
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
            self.bound = Some(BoundScaler {
                sws,
                src_width: src_frame.width,
                src_height: src_frame.height,
                src_pix_fmt,
                dst_width,
                dst_height,
                dst_pix_fmt,
            });
        }
        let bound = self.bound.as_mut().expect("bound above");

        let mut dst_frame = AVFrame::new();
        dst_frame.set_width(dst_width);
        dst_frame.set_height(dst_height);
        dst_frame.set_format(dst_pix_fmt.into());
        dst_frame
            .alloc_buffer()
            .context("Failed to allocate destination frame buffer")?;
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
                return Err(RsmediaError::custom(format!(
                    "Failed to call sws_scale_frame, ret: {ret}"
                )));
            }
        }

        #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
        bound
            .sws
            .scale_full_frame(&mut dst_frame, src_frame)
            .context("Failed to scale frame.")?;

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
/// has been bound — `SwsContext` itself implements no `Debug`.
impl std::fmt::Debug for Scaler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scaler")
            .field("algorithm", &self.algorithm)
            .field("quality", &format!("{:#x}", self.quality))
            .field("bound", &self.bound.is_some())
            .finish()
    }
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
