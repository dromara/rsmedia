use anyhow::{Error, Result};
use rsmpeg::avcodec::{AVCodec, AVCodecContext};
use rsmpeg::ffi;
use std::ffi::CStr;
use std::ptr::NonNull;

pub struct CodecConfig {
    codec: NonNull<ffi::AVCodec>,
}

impl<'codec> CodecConfig {
    pub fn new(id: ffi::AVCodecID) -> Self {
        let codec = unsafe {
            let codec = AVCodec::find_encoder(id)
                .or_else(|| AVCodec::find_decoder(id))
                .ok_or_else(|| Error::msg(format!("Codec not found: {id}")))
                .unwrap();
            NonNull::new_unchecked(codec.as_ptr() as *mut _)
        };
        CodecConfig { codec }
    }

    pub fn new_with_name(codec_name: &CStr) -> Result<Self> {
        let codec = AVCodec::find_encoder_by_name(codec_name)
            .or_else(|| AVCodec::find_decoder_by_name(codec_name))
            .ok_or_else(|| Error::msg(format!("Codec not found: '{codec_name:?}'")))?;
        Ok(Self::new(codec.id))
    }

    pub fn new_with_ctx(ctx: &AVCodecContext) -> Self {
        CodecConfig {
            codec: unsafe { NonNull::new_unchecked(ctx.codec as *const _ as *mut _) },
        }
    }

    pub fn is_encoder(&self) -> bool {
        unsafe { ffi::av_codec_is_encoder(self.codec.as_ptr()) != 0 }
    }

    pub fn is_decoder(&self) -> bool {
        unsafe { ffi::av_codec_is_decoder(self.codec.as_ptr()) != 0 }
    }

    unsafe fn probe_len<T>(mut ptr: *const T, tail: T) -> usize {
        for len in 0.. {
            let left = ptr as *const u8;
            let left = unsafe { std::slice::from_raw_parts(left, std::mem::size_of::<T>()) };
            let right = &tail as *const _ as *const u8;
            let right = unsafe { std::slice::from_raw_parts(right, std::mem::size_of::<T>()) };
            if left == right {
                return len;
            }
            unsafe {
                ptr = ptr.add(1);
            }
        }
        usize::MAX
    }

    unsafe fn build_array<'r, T>(ptr: *const T, tail: T) -> Option<&'r [T]> {
        if ptr.is_null() {
            None
        } else {
            let len = unsafe { Self::probe_len(ptr, tail) };
            if len == usize::MAX {
                None
            } else {
                Some(unsafe { std::slice::from_raw_parts(ptr, len) })
            }
        }
    }

    /// Retrieve a list of all supported values for a given configuration type.
    ///
    /// # Arguments
    /// * `config_type` - The type of configuration to retrieve.
    /// * `tail` - The value that marks the end of the list.
    ///
    /// See: <https://ffmpeg.org/pipermail/ffmpeg-cvslog/2024-September/145256.html>
    /// [`avcodec_get_supported_config(avctx, codec, config, flags, out_configs, out_num_configs)`]
    ///
    /// * `avctx`    An optional context to use. Values such as strict_std_compliance may affect the result. If NULL, default values are used.
    /// * `codec`    The codec to query, or NULL to use avctx->codec.
    /// * `config`    The configuration to query.
    /// * `flags`    Currently unused; should be set to zero.
    /// * `out_configs`    On success, set to a list of configurations, terminated by a config-specific terminator, or NULL if all possible values are supported.
    /// * `out_num_configs`    On success, set to the number of elements in out_configs, excluding the terminator. Optional.
    #[cfg(feature = "ffmpeg7")]
    unsafe fn get_supported_config<T>(
        &self,
        config_type: ffi::AVCodecConfig,
        tail: T,
    ) -> Result<Option<&'codec [T]>> {
        let mut configs = std::ptr::null();
        let mut num_configs = 0;

        let codec_ptr = self.codec.as_ptr() as *const _;

        let ret = ffi::avcodec_get_supported_config(
            std::ptr::null(),
            codec_ptr,
            config_type,
            0,
            &mut configs,
            &mut num_configs,
        );

        if ret < 0 {
            return Err(Error::msg(format!(
                "Failed to get codec supported config:{ret}"
            )));
        }

        Ok(unsafe { Self::build_array(configs as *const T, tail) })
    }

    pub fn supported_pixel_formats(&self) -> Result<Option<&'codec [ffi::AVPixelFormat]>> {
        #[cfg(feature = "ffmpeg7")]
        unsafe {
            self.get_supported_config(ffi::AV_CODEC_CONFIG_PIX_FORMAT, ffi::AV_PIX_FMT_NONE)
        }
        #[cfg(not(feature = "ffmpeg7"))]
        unsafe {
            // terminates with -1
            Ok(Self::build_array((*self.codec.as_ptr()).pix_fmts, -1))
        }
    }

    pub fn supported_frame_rates(&self) -> Result<Option<&'codec [ffi::AVRational]>> {
        let tail = ffi::AVRational { num: 0, den: 0 };
        #[cfg(feature = "ffmpeg7")]
        unsafe {
            self.get_supported_config(ffi::AV_CODEC_CONFIG_FRAME_RATE, tail)
        }
        #[cfg(not(feature = "ffmpeg7"))]
        unsafe {
            // terminates with AVRational{0, 0}
            Ok(Self::build_array(
                (*self.codec.as_ptr()).supported_framerates,
                tail,
            ))
        }
    }

    pub fn supported_sample_rates(&self) -> Result<Option<&'codec [i32]>> {
        #[cfg(feature = "ffmpeg7")]
        unsafe {
            self.get_supported_config(ffi::AV_CODEC_CONFIG_SAMPLE_RATE, 0)
        }
        #[cfg(not(feature = "ffmpeg7"))]
        unsafe {
            // terminates with 0
            Ok(Self::build_array(
                (*self.codec.as_ptr()).supported_samplerates,
                0,
            ))
        }
    }

    pub fn supported_sample_formats(&self) -> Result<Option<&'codec [ffi::AVSampleFormat]>> {
        #[cfg(feature = "ffmpeg7")]
        unsafe {
            self.get_supported_config(ffi::AV_CODEC_CONFIG_SAMPLE_FORMAT, ffi::AV_SAMPLE_FMT_NONE)
        }
        #[cfg(not(feature = "ffmpeg7"))]
        unsafe {
            // terminates with -1
            Ok(Self::build_array((*self.codec.as_ptr()).sample_fmts, -1))
        }
    }

    pub fn supported_channel_layouts(&self) -> Result<Option<&'codec [ffi::AVChannelLayout]>> {
        let tail = unsafe {
            ffi::AVChannelLayout {
                order: ffi::AV_CHANNEL_ORDER_UNSPEC,
                nb_channels: 0,
                u: std::mem::zeroed(),
                opaque: std::ptr::null_mut(),
            }
        };
        #[cfg(feature = "ffmpeg7")]
        unsafe {
            self.get_supported_config(ffi::AV_CODEC_CONFIG_CHANNEL_LAYOUT, tail)
        }
        #[cfg(not(feature = "ffmpeg7"))]
        unsafe {
            // terminates with {0}
            Ok(Self::build_array((*self.codec.as_ptr()).ch_layouts, tail))
        }
    }

    pub fn supported_color_ranges(&self) -> Result<Option<&'codec [ffi::AVColorRange]>> {
        #[cfg(feature = "ffmpeg7")]
        unsafe {
            self.get_supported_config(
                ffi::AV_CODEC_CONFIG_COLOR_RANGE,
                ffi::AVCOL_RANGE_UNSPECIFIED,
            )
        }
        #[cfg(not(feature = "ffmpeg7"))]
        {
            Ok(None)
        }
    }

    pub fn supported_color_spaces(&self) -> Result<Option<&'codec [ffi::AVColorSpace]>> {
        #[cfg(feature = "ffmpeg7")]
        unsafe {
            self.get_supported_config(ffi::AV_CODEC_CONFIG_COLOR_SPACE, ffi::AVCOL_SPC_UNSPECIFIED)
        }
        #[cfg(not(feature = "ffmpeg7"))]
        {
            Ok(None)
        }
    }

    /// for audio codec, check if it supports variable frame size
    pub fn support_variable_frame_size(&self) -> bool {
        unsafe {
            (*self.codec.as_ptr()).capabilities & ffi::AV_CODEC_CAP_VARIABLE_FRAME_SIZE as i32 != 0
        }
    }

    /// for codec, check if it supports delay
    pub fn support_delayed_frame(&self) -> bool {
        unsafe { (*self.codec.as_ptr()).capabilities & ffi::AV_CODEC_CAP_DELAY as i32 != 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 断言视频编码/解码器支持的非空像素格式列表非空。
    /// 若 FFmpeg 返回 `None`（表示"所有值均支持"）则视为通过。
    fn assert_video_config(config: &CodecConfig, name: &str) {
        let pix = config
            .supported_pixel_formats()
            .unwrap_or_else(|e| panic!("{name}: query pixel formats failed: {e}"));
        assert!(
            pix.map(|v| !v.is_empty()).unwrap_or(true),
            "{name}: expected non-empty supported pixel formats"
        );

        let rates = config
            .supported_frame_rates()
            .unwrap_or_else(|e| panic!("{name}: query frame rates failed: {e}"));
        assert!(
            rates.map(|v| !v.is_empty()).unwrap_or(true),
            "{name}: frame rates should be non-empty if specified"
        );

        let ranges = config
            .supported_color_ranges()
            .unwrap_or_else(|e| panic!("{name}: query color ranges failed: {e}"));
        assert!(
            ranges.map(|v| !v.is_empty()).unwrap_or(true),
            "{name}: color ranges should be non-empty if specified"
        );

        let spaces = config
            .supported_color_spaces()
            .unwrap_or_else(|e| panic!("{name}: query color spaces failed: {e}"));
        assert!(
            spaces.map(|v| !v.is_empty()).unwrap_or(true),
            "{name}: color spaces should be non-empty if specified"
        );
    }

    /// 断言音频编码/解码器支持的采样率、采样格式列表非空。
    fn assert_audio_config(config: &CodecConfig, name: &str) {
        let rates = config
            .supported_sample_rates()
            .unwrap_or_else(|e| panic!("{name}: query sample rates failed: {e}"));
        assert!(
            rates.map(|v| !v.is_empty()).unwrap_or(true),
            "{name}: sample rates should be non-empty if specified"
        );

        let fmts = config
            .supported_sample_formats()
            .unwrap_or_else(|e| panic!("{name}: query sample formats failed: {e}"));
        assert!(
            fmts.map(|v| !v.is_empty()).unwrap_or(true),
            "{name}: expected non-empty supported sample formats"
        );

        let layouts = config
            .supported_channel_layouts()
            .unwrap_or_else(|e| panic!("{name}: query channel layouts failed: {e}"));
        assert!(
            layouts.map(|v| !v.is_empty()).unwrap_or(true),
            "{name}: channel layouts should be non-empty if specified"
        );
    }

    #[test]
    fn test_supported_video_codec() {
        for id in [
            ffi::AV_CODEC_ID_H264,
            ffi::AV_CODEC_ID_MPEG4,
            ffi::AV_CODEC_ID_VP8,
            ffi::AV_CODEC_ID_VP9,
            ffi::AV_CODEC_ID_HEVC,
            ffi::AV_CODEC_ID_AV1,
        ] {
            let config = CodecConfig::new(id);
            assert_video_config(&config, &format!("video codec {id}"));
            assert!(
                config.is_encoder() || config.is_decoder(),
                "video codec {id} should be encoder or decoder"
            );
        }
    }

    #[test]
    #[cfg(unix)]
    fn test_supported_video_codec_name() {
        for name in [
            c"libx264",
            c"libx265",
            c"mpeg4",
            c"mpeg1video",
            c"mpeg2video",
        ] {
            let config = CodecConfig::new_with_name(name)
                .unwrap_or_else(|e| panic!("could not find codec {name:?}: {e}"));
            assert_video_config(&config, &format!("video codec {name:?}"));
        }
    }

    #[test]
    fn test_supported_audio_codec() {
        for id in [
            ffi::AV_CODEC_ID_AAC,
            ffi::AV_CODEC_ID_FLAC,
            ffi::AV_CODEC_ID_MP3,
            ffi::AV_CODEC_ID_OPUS,
            ffi::AV_CODEC_ID_VORBIS,
        ] {
            let config = CodecConfig::new(id);
            assert_audio_config(&config, &format!("audio codec {id}"));
            assert!(
                config.is_encoder() || config.is_decoder(),
                "audio codec {id} should be encoder or decoder"
            );
        }
    }

    #[test]
    #[cfg(unix)]
    #[ignore = "skip test_supported_audio_codec_name"]
    fn test_supported_audio_codec_name() {
        for name in [c"libmp3lame", c"libopus", c"libvorbis"] {
            let config = CodecConfig::new_with_name(name)
                .unwrap_or_else(|e| panic!("could not find codec {name:?}: {e}"));
            assert_audio_config(&config, &format!("audio codec {name:?}"));
        }
    }
}
