use crate::hwaccel::HWDeviceType;
use rsmpeg::avcodec::AVCodec;
use rsmpeg::avcodec::AVCodecContext;
use rsmpeg::avutil::AVFrame;
use rsmpeg::avutil::AVPixelFormat;
use rsmpeg::error::RsmpegError;
use rsmpeg::ffi;

pub struct HWDeviceContext {
    ptr: *mut ffi::AVBufferRef,
}

impl HWDeviceContext {
    pub fn new(device_type: HWDeviceType) -> Result<HWDeviceContext, RsmpegError> {
        let mut ptr: *mut ffi::AVBufferRef = std::ptr::null_mut();

        unsafe {
            match ffi::av_hwdevice_ctx_create(
                (&mut ptr) as *mut *mut ffi::AVBufferRef,
                device_type.into(),
                std::ptr::null(),
                std::ptr::null_mut(),
                0,
            ) {
                0 => Ok(HWDeviceContext { ptr }),
                e => Err(RsmpegError::from(e)),
            }
        }
    }

    unsafe fn ref_raw(&self) -> *mut ffi::AVBufferRef {
        ffi::av_buffer_ref(self.ptr)
    }
}

impl Drop for HWDeviceContext {
    fn drop(&mut self) {
        unsafe {
            ffi::av_buffer_unref(&mut self.ptr);
        }
    }
}

pub fn hwdevice_list_available_device_types() -> Vec<HWDeviceType> {
    let mut hwdevice_types = Vec::new();
    let mut hwdevice_type = unsafe { ffi::av_hwdevice_iterate_types(ffi::AV_HWDEVICE_TYPE_NONE) };
    while hwdevice_type != ffi::AV_HWDEVICE_TYPE_NONE {
        hwdevice_types.push(HWDeviceType::from(hwdevice_type).unwrap());
        hwdevice_type = unsafe { ffi::av_hwdevice_iterate_types(hwdevice_type) };
    }
    hwdevice_types
}

pub fn hwdevice_transfer_frame(
    target_frame: &mut AVFrame,
    hwdevice_frame: &AVFrame,
) -> Result<(), RsmpegError> {
    unsafe {
        match ffi::av_hwframe_transfer_data(target_frame.as_mut_ptr(), hwdevice_frame.as_ptr(), 0) {
            0 => Ok(()),
            e => Err(RsmpegError::from(e)),
        }
    }
}

pub fn codec_find_hwaccel_pixfmt(
    codec: &AVCodec,
    hwaccel_type: HWDeviceType,
) -> Option<AVPixelFormat> {
    let mut i = 0;
    loop {
        unsafe {
            let hw_config = ffi::avcodec_get_hw_config(codec.as_ptr(), i);
            if !hw_config.is_null() {
                let hw_config_supports_codec = (((*hw_config).methods) as i32
                    & ffi::AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX as i32)
                    != 0;
                if hw_config_supports_codec && (*hw_config).device_type == hwaccel_type.into() {
                    break Some((*hw_config).pix_fmt);
                }
            } else {
                break None;
            }
        }
        i += 1;
    }
}

pub fn codec_context_hwaccel_set_get_format(
    codec_context: &mut AVCodecContext,
    hw_pixfmt: AVPixelFormat,
) {
    unsafe {
        (*codec_context.as_mut_ptr()).opaque = ffi::AVPixelFormat::from(hw_pixfmt) as _;
        (*codec_context.as_mut_ptr()).get_format = Some(hwaccel_get_format);
    }
}

pub fn codec_context_hwaccel_set_hw_device_ctx(
    codec_context: &mut AVCodecContext,
    hardware_device_context: &HWDeviceContext,
) {
    unsafe {
        (*codec_context.as_mut_ptr()).hw_device_ctx = hardware_device_context.ref_raw();
    }
}

#[no_mangle]
unsafe extern "C" fn hwaccel_get_format(
    ctx: *mut ffi::AVCodecContext,
    pix_fmts: *const ffi::AVPixelFormat,
) -> ffi::AVPixelFormat {
    let mut p = pix_fmts;
    while *p != ffi::AV_PIX_FMT_NONE {
        if *p == ((*ctx).opaque as i32) as ffi::AVPixelFormat {
            return *p;
        }
        p = p.add(1);
    }
    ffi::AV_PIX_FMT_NONE
}
