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
                .ok_or_else(|| Error::msg(format!("Codec not found: {}", id)))
                .unwrap();
            NonNull::new_unchecked(codec.as_ptr() as *mut _)
        };
        CodecConfig { codec }
    }

    pub fn new_with_name(codec_name: &CStr) -> Result<Self> {
        let codec = AVCodec::find_encoder_by_name(codec_name)
            .or_else(|| AVCodec::find_decoder_by_name(codec_name))
            .ok_or_else(|| Error::msg(format!("Codec not found: '{:?}'", codec_name)))?;
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
                "Failed to get codec supported config:{}",
                ret
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

    #[test]
    fn test_supported_video_codec() {
        let config = CodecConfig::new(ffi::AV_CODEC_ID_H264);
        println!("{:?}", config.supported_pixel_formats().unwrap());
        println!("{:?}", config.supported_frame_rates().unwrap());
        println!("{:?}", config.supported_color_ranges().unwrap());
        println!("{:?}", config.supported_color_spaces().unwrap());
        println!("{:?}", config.support_delayed_frame());
        println!("=========================================");

        let config = CodecConfig::new(ffi::AV_CODEC_ID_MPEG4);
        println!("{:?}", config.supported_pixel_formats().unwrap());
        println!("{:?}", config.supported_frame_rates().unwrap());
        println!("{:?}", config.supported_color_ranges().unwrap());
        println!("{:?}", config.supported_color_spaces().unwrap());
        println!("{:?}", config.support_delayed_frame());
        println!("=========================================");

        let config = CodecConfig::new(ffi::AV_CODEC_ID_VP8);
        println!("{:?}", config.supported_pixel_formats().unwrap());
        println!("{:?}", config.supported_frame_rates().unwrap());
        println!("{:?}", config.supported_color_ranges().unwrap());
        println!("{:?}", config.supported_color_spaces().unwrap());
        println!("{:?}", config.support_delayed_frame());
        println!("=========================================");

        let config = CodecConfig::new(ffi::AV_CODEC_ID_VP9);
        println!("{:?}", config.supported_pixel_formats().unwrap());
        println!("{:?}", config.supported_frame_rates().unwrap());
        println!("{:?}", config.supported_color_ranges().unwrap());
        println!("{:?}", config.supported_color_spaces().unwrap());
        println!("{:?}", config.support_delayed_frame());
        println!("=========================================");

        let config = CodecConfig::new(ffi::AV_CODEC_ID_HEVC);
        println!("{:?}", config.supported_pixel_formats().unwrap());
        println!("{:?}", config.supported_frame_rates().unwrap());
        println!("{:?}", config.supported_color_ranges().unwrap());
        println!("{:?}", config.supported_color_spaces().unwrap());
        println!("{:?}", config.support_delayed_frame());
        println!("=========================================");

        let config = CodecConfig::new(ffi::AV_CODEC_ID_AV1);
        println!("{:?}", config.supported_pixel_formats().unwrap());
        println!("{:?}", config.supported_frame_rates().unwrap());
        println!("{:?}", config.supported_color_ranges().unwrap());
        println!("{:?}", config.supported_color_spaces().unwrap());
        println!("{:?}", config.support_delayed_frame());
        println!("=========================================");
    }

    #[test]
    #[cfg(unix)]
    fn test_supported_video_codec_name() {
        let config = CodecConfig::new_with_name(c"libx264").unwrap();
        println!("{:?}", config.supported_pixel_formats().unwrap());
        println!("{:?}", config.supported_frame_rates().unwrap());
        println!("{:?}", config.supported_color_ranges().unwrap());
        println!("{:?}", config.supported_color_spaces().unwrap());
        println!("{:?}", config.support_delayed_frame());
        println!("=========================================");

        let config = CodecConfig::new_with_name(c"libx265").unwrap();
        println!("{:?}", config.supported_pixel_formats().unwrap());
        println!("{:?}", config.supported_frame_rates().unwrap());
        println!("{:?}", config.supported_color_ranges().unwrap());
        println!("{:?}", config.supported_color_spaces().unwrap());
        println!("{:?}", config.support_delayed_frame());
        println!("=========================================");

        let config = CodecConfig::new_with_name(c"mpeg4").unwrap();
        println!("{:?}", config.supported_pixel_formats().unwrap());
        println!("{:?}", config.supported_frame_rates().unwrap());
        println!("{:?}", config.supported_color_ranges().unwrap());
        println!("{:?}", config.supported_color_spaces().unwrap());
        println!("{:?}", config.support_delayed_frame());
        println!("=========================================");

        let config = CodecConfig::new_with_name(c"mpeg1video").unwrap();
        println!("{:?}", config.supported_pixel_formats().unwrap());
        println!("{:?}", config.supported_frame_rates().unwrap());
        println!("{:?}", config.supported_color_ranges().unwrap());
        println!("{:?}", config.supported_color_spaces().unwrap());
        println!("{:?}", config.support_delayed_frame());
        println!("=========================================");

        let config = CodecConfig::new_with_name(c"mpeg2video").unwrap();
        println!("{:?}", config.supported_pixel_formats().unwrap());
        println!("{:?}", config.supported_frame_rates().unwrap());
        println!("{:?}", config.supported_color_ranges().unwrap());
        println!("{:?}", config.supported_color_spaces().unwrap());
        println!("{:?}", config.support_delayed_frame());
        println!("=========================================");
    }

    #[test]
    fn test_supported_audio_codec() {
        let config = CodecConfig::new(ffi::AV_CODEC_ID_AAC);
        println!("{:?}", config.supported_sample_rates().unwrap());
        println!("{:?}", config.supported_sample_formats().unwrap());
        println!("{:?}", config.supported_channel_layouts().unwrap());
        println!("{:?}", config.support_variable_frame_size());
        println!("=========================================");

        let config = CodecConfig::new(ffi::AV_CODEC_ID_FLAC);
        println!("{:?}", config.supported_sample_rates().unwrap());
        println!("{:?}", config.supported_sample_formats().unwrap());
        println!("{:?}", config.supported_channel_layouts().unwrap());
        println!("{:?}", config.support_variable_frame_size());
        println!("=========================================");

        let config = CodecConfig::new(ffi::AV_CODEC_ID_MP3);
        println!("{:?}", config.supported_sample_rates().unwrap());
        println!("{:?}", config.supported_sample_formats().unwrap());
        println!("{:?}", config.supported_channel_layouts().unwrap());
        println!("{:?}", config.support_variable_frame_size());
        println!("=========================================");

        let config = CodecConfig::new(ffi::AV_CODEC_ID_OPUS);
        println!("{:?}", config.supported_sample_rates().unwrap());
        println!("{:?}", config.supported_sample_formats().unwrap());
        println!("{:?}", config.supported_channel_layouts().unwrap());
        println!("{:?}", config.support_variable_frame_size());
        println!("=========================================");

        let config = CodecConfig::new(ffi::AV_CODEC_ID_VORBIS);
        println!("{:?}", config.supported_sample_rates().unwrap());
        println!("{:?}", config.supported_sample_formats().unwrap());
        println!("{:?}", config.supported_channel_layouts().unwrap());
        println!("{:?}", config.support_variable_frame_size());
        println!("=========================================");
    }

    #[test]
    #[cfg(unix)]
    fn test_supported_audio_codec_name() {
        let config = CodecConfig::new_with_name(c"libmp3lame").unwrap();
        println!("{:?}", config.supported_sample_rates().unwrap());
        println!("{:?}", config.supported_sample_formats().unwrap());
        println!("{:?}", config.supported_channel_layouts().unwrap());
        println!("{:?}", config.support_variable_frame_size());
        println!("=========================================");

        let config = CodecConfig::new_with_name(c"libopus").unwrap();
        println!("{:?}", config.supported_sample_rates().unwrap());
        println!("{:?}", config.supported_sample_formats().unwrap());
        println!("{:?}", config.supported_channel_layouts().unwrap());
        println!("{:?}", config.support_variable_frame_size());
        println!("=========================================");

        let config = CodecConfig::new_with_name(c"libvorbis").unwrap();
        println!("{:?}", config.supported_sample_rates().unwrap());
        println!("{:?}", config.supported_sample_formats().unwrap());
        println!("{:?}", config.supported_channel_layouts().unwrap());
        println!("{:?}", config.support_variable_frame_size());
        println!("=========================================");
    }
}
