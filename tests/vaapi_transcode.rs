//! RIIR: https://github.com/FFmpeg/FFmpeg/blob/master/doc/examples/vaapi_transcode.c

use anyhow::{anyhow, bail, Context, Error, Result};
use std::ffi::CStr;

use rsmpeg::avcodec::{AVCodec, AVCodecContext};
use rsmpeg::avformat::{AVFormatContextInput, AVFormatContextOutput};
use rsmpeg::avutil::{self, AVFrame, AVHWDeviceContext, AVPixelFormat};
use rsmpeg::error::RsmpegError;
use rsmpeg::ffi;
use rsmpeg::ffi::{
    AVHWDeviceType, AV_HWDEVICE_TYPE_CUDA, AV_HWDEVICE_TYPE_VAAPI, AV_PIX_FMT_CUDA,
    AV_PIX_FMT_NV12, AV_PIX_FMT_VAAPI,
};

#[no_mangle]
unsafe extern "C" fn hwaccel_get_format(
    ctx: *mut ffi::AVCodecContext,
    pix_fmts: *const ffi::AVPixelFormat,
) -> ffi::AVPixelFormat {
    let mut p = pix_fmts;
    let hw_format = (*ctx).opaque as ffi::AVPixelFormat;
    while *p != ffi::AV_PIX_FMT_NONE {
        if *p == hw_format {
            return *p;
        }
        p = p.add(1);
    }
    ffi::AV_PIX_FMT_NONE
}

fn set_hwframe_ctx(
    is_decoder: bool,
    codec_ctx: &mut AVCodecContext,
    hw_device_ctx: &AVHWDeviceContext,
    width: i32,
    height: i32,
    hw_format: AVPixelFormat,
    sw_format: AVPixelFormat,
) -> Result<()> {
    let mut hw_frames_ref = hw_device_ctx.hwframe_ctx_alloc();
    hw_frames_ref.data().format = hw_format;
    hw_frames_ref.data().sw_format = sw_format;
    hw_frames_ref.data().width = width;
    hw_frames_ref.data().height = height;
    hw_frames_ref.data().initial_pool_size = 20;

    hw_frames_ref
        .init()
        .context("Failed to initialize VAAPI frame context")?;

    codec_ctx.set_hw_frames_ctx(hw_frames_ref);
    codec_ctx.set_pix_fmt(hw_format);

    if is_decoder {
        unsafe {
            let hw_device_ctx_ptr = hw_device_ctx.as_ptr();
            let codec_ctx_ptr = codec_ctx.as_mut_ptr();
            (*codec_ctx_ptr).opaque = hw_format as *mut std::os::raw::c_void;
            (*codec_ctx_ptr).get_format = Some(hwaccel_get_format);
            (*codec_ctx_ptr).sw_pix_fmt = sw_format;
            (*codec_ctx_ptr).hw_device_ctx = hw_device_ctx_ptr as *mut _;
        }
    }

    Ok(())
}

fn open_input_file(
    filename: &CStr,
    decode_codec_name: &CStr,
    hw_device_ctx: &AVHWDeviceContext,
    hw_format: AVPixelFormat,
    sw_format: AVPixelFormat,
) -> Result<(AVCodecContext, AVFormatContextInput, usize)> {
    let mut ifmt_ctx = AVFormatContextInput::open(filename)?;
    let (video_index, _decode_codec) = ifmt_ctx
        .find_best_stream(ffi::AVMEDIA_TYPE_VIDEO)?
        .context("Failed to find video stream")?;
    let video_stream = &ifmt_ctx.streams()[video_index];

    let decode_codec = AVCodec::find_decoder_by_name(decode_codec_name).with_context(|| {
        anyhow!(
            "Failed to find decoder codec: {}",
            decode_codec_name.to_str().unwrap()
        )
    })?;
    let mut decode_ctx = AVCodecContext::new(&decode_codec);
    let time_base = avutil::ra(1, 24);
    decode_ctx.set_time_base(time_base);
    decode_ctx.set_sample_aspect_ratio(avutil::ra(1, 1));
    decode_ctx.apply_codecpar(&video_stream.codecpar())?;

    let (width, height) = (decode_ctx.width, decode_ctx.height);
    set_hwframe_ctx(
        true,
        &mut decode_ctx,
        hw_device_ctx,
        width,
        height,
        hw_format,
        sw_format,
    )?;

    decode_ctx
        .open(None)
        .with_context(|| anyhow!("Failed to open decoder context"))?;

    ifmt_ctx.dump(0, filename)?;

    Ok((decode_ctx, ifmt_ctx, video_index))
}

fn open_output_file(
    filename: &CStr,
    encode_codec_name: &CStr,
    decode_ctx: &AVCodecContext,
    hw_device_ctx: &AVHWDeviceContext,
    hw_format: AVPixelFormat,
    sw_format: AVPixelFormat,
) -> Result<(AVCodecContext, AVFormatContextOutput, usize)> {
    let mut ofmt_ctx = AVFormatContextOutput::create(filename)?;

    let encode_codec = AVCodec::find_encoder_by_name(encode_codec_name).with_context(|| {
        anyhow!(
            "Failed to find encoder codec: {}",
            encode_codec_name.to_str().unwrap()
        )
    })?;
    let mut encode_ctx = AVCodecContext::new(&encode_codec);
    encode_ctx.set_width(decode_ctx.width);
    encode_ctx.set_height(decode_ctx.height);
    encode_ctx.set_framerate(decode_ctx.framerate);
    encode_ctx.set_time_base(decode_ctx.time_base);
    encode_ctx.set_sample_aspect_ratio(decode_ctx.sample_aspect_ratio);

    // Some formats want stream headers to be separate.
    if ofmt_ctx.oformat().flags & ffi::AVFMT_GLOBALHEADER as i32 != 0 {
        encode_ctx.set_flags(encode_ctx.flags | ffi::AV_CODEC_FLAG_GLOBAL_HEADER as i32);
    }

    let (width, height) = (encode_ctx.width, encode_ctx.height);

    set_hwframe_ctx(
        false,
        &mut encode_ctx,
        hw_device_ctx,
        width,
        height,
        hw_format,
        sw_format,
    )?;

    encode_ctx.open(None).with_context(|| {
        anyhow!(
            "Cannot open {} encoder context.",
            encode_codec.name().to_str().unwrap()
        )
    })?;

    let out_stream_index = {
        let mut out_stream = ofmt_ctx.new_stream();
        out_stream.set_codecpar(encode_ctx.extract_codecpar());
        out_stream.set_time_base(encode_ctx.time_base);
        out_stream.index
    };

    Ok((encode_ctx, ofmt_ctx, out_stream_index as usize))
}

/// hw_frame -> sw_frame
fn hw_download(hw_frame: AVFrame, sw_format: AVPixelFormat) -> Result<AVFrame> {
    let mut sw_frame = AVFrame::new();
    sw_frame.set_width(hw_frame.width);
    sw_frame.set_height(hw_frame.height);
    sw_frame.set_format(sw_format);
    sw_frame
        .alloc_buffer()
        .context("Failed to allocate software frame buffer")?;

    sw_frame
        .hwframe_transfer_data(&hw_frame)
        .context("Failed to transfer data from hardware frame to software frame")?;

    sw_frame.set_pts(hw_frame.pts);
    sw_frame.set_time_base(hw_frame.time_base);
    sw_frame.set_sample_rate(hw_frame.sample_rate);

    Ok(sw_frame)
}

/// sw_frame -> hw_frame
fn hw_upload(
    encode_ctx: &mut AVCodecContext,
    sw_frame: AVFrame,
    hw_format: AVPixelFormat,
) -> Result<AVFrame> {
    let mut hw_frames_ctx = encode_ctx
        .hw_frames_ctx_mut()
        .ok_or_else(|| Error::msg("Encoder has no hardware frames context"))?;

    let mut hw_frame = AVFrame::new();
    hw_frame.set_width(sw_frame.width);
    hw_frame.set_height(sw_frame.height);
    hw_frame.set_format(hw_format);
    unsafe {
        (*hw_frame.as_mut_ptr()).hw_frames_ctx = hw_frames_ctx.as_mut_ptr();
    }

    hw_frames_ctx
        .get_buffer(&mut hw_frame)
        .context("Failed to allocate hardware frame buffer")?;

    hw_frame
        .hwframe_transfer_data(&sw_frame)
        .context("Failed to transfer data from software frame to hardware frame")?;

    hw_frame.set_pts(sw_frame.pts);
    hw_frame.set_time_base(sw_frame.time_base);
    hw_frame.set_sample_rate(sw_frame.sample_rate);

    Ok(hw_frame)
}

/// Send an empty packet to the `encode_context` for packet flushing.
fn flush_encoder(
    enc_ctx: &mut AVCodecContext,
    ofmt_ctx: &mut AVFormatContextOutput,
    stream_index: usize,
) -> Result<()> {
    if enc_ctx.codec().capabilities & ffi::AV_CODEC_CAP_DELAY as i32 == 0 {
        return Ok(());
    }
    encode_write_frame(None, enc_ctx, ofmt_ctx, stream_index)?;
    Ok(())
}

fn encode_write_frame(
    mut filt_frame: Option<AVFrame>,
    enc_ctx: &mut AVCodecContext,
    ofmt_ctx: &mut AVFormatContextOutput,
    stream_index: usize,
) -> Result<()> {
    if let Some(filt_frame) = filt_frame.as_mut() {
        if filt_frame.pts != ffi::AV_NOPTS_VALUE {
            filt_frame.set_pts(avutil::av_rescale_q(
                filt_frame.pts,
                filt_frame.time_base,
                enc_ctx.time_base,
            ));
        }
    }

    enc_ctx
        .send_frame(filt_frame.as_ref())
        .context("Encode frame failed.")?;

    loop {
        let mut enc_pkt = match enc_ctx.receive_packet() {
            Ok(packet) => packet,
            Err(RsmpegError::EncoderDrainError) | Err(RsmpegError::EncoderFlushedError) => break,
            Err(e) => bail!(e),
        };

        enc_pkt.set_pos(-1);
        enc_pkt.set_stream_index(stream_index as i32);
        enc_pkt.rescale_ts(
            enc_ctx.time_base,
            ofmt_ctx.streams()[stream_index].time_base,
        );

        ofmt_ctx
            .interleaved_write_frame(&mut enc_pkt)
            .context("Interleaved write frame failed.")?;
    }

    Ok(())
}

fn hw_transcode(
    input: &CStr,
    output: &CStr,
    decode_codec: &CStr,
    encode_codec: &CStr,
    device_type: AVHWDeviceType,
    hw_format: AVPixelFormat,
    sw_format: AVPixelFormat,
) -> Result<()> {
    let hw_device_ctx = AVHWDeviceContext::create(device_type, None, None, 0)
        .context("Failed to create a hw device context")?;

    let (mut decode_ctx, mut ifmt_ctx, input_stream_index) =
        open_input_file(input, decode_codec, &hw_device_ctx, hw_format, sw_format)?;

    let (mut encode_ctx, mut ofmt_ctx, out_stream_index) = open_output_file(
        output,
        encode_codec,
        &decode_ctx,
        &hw_device_ctx,
        hw_format,
        sw_format,
    )?;

    ofmt_ctx
        .write_header(&mut None)
        .context("Failed to write output header")?;

    loop {
        let packet = match ifmt_ctx.read_packet() {
            Ok(Some(x)) => x,
            // No more frames
            Ok(None) => break,
            Err(e) => bail!("Read frame error: {:?}", e),
        };

        // Skip if not video packet
        if packet.stream_index != input_stream_index as i32 {
            continue;
        }

        decode_ctx
            .send_packet(Some(&packet))
            .context("Send packet error.")?;

        loop {
            let hw_frame = match decode_ctx.receive_frame() {
                Ok(frame) => frame,
                Err(RsmpegError::DecoderDrainError) | Err(RsmpegError::DecoderFlushedError) => {
                    break;
                }
                Err(e) => bail!(e),
            };

            assert_eq!(hw_frame.format, hw_format);
            assert!(
                !hw_frame.hw_frames_ctx.is_null(),
                "HW frame context is null"
            );

            let download_sw_frame = hw_download(hw_frame, sw_format)?;

            // do something process, scaler frame etc.
            log::info!("{:?}", download_sw_frame);

            let upload_hw_frame = hw_upload(&mut encode_ctx, download_sw_frame, hw_format)?;

            encode_write_frame(
                Some(upload_hw_frame),
                &mut encode_ctx,
                &mut ofmt_ctx,
                out_stream_index,
            )?
        }
    }

    flush_encoder(&mut encode_ctx, &mut ofmt_ctx, out_stream_index)?;
    ofmt_ctx.write_trailer()?;

    Ok(())
}

#[test]
#[ignore = "Github actions doesn't have vaapi device"]
fn vaapi_transcode_test_vaapi() {
    std::fs::create_dir_all("tests/output/vaapi_transcode/").unwrap();

    hw_transcode(
        c"tests/assets/vids/bear.mp4",
        c"tests/output/vaapi_transcode/vaapi_transcode_h264_vaapi.mp4",
        c"h264_vaapi",
        c"h264_vaapi",
        AV_HWDEVICE_TYPE_VAAPI,
        AV_PIX_FMT_VAAPI,
        AV_PIX_FMT_NV12,
    )
    .unwrap();
}

#[test]
#[ignore = "Github actions doesn't have nvdia graphics card"]
fn nvenc_transcode_test_nvenc() {
    std::fs::create_dir_all("tests/output/nvenc_transcode/").unwrap();
    hw_transcode(
        c"tests/assets/vids/bear.mp4",
        c"tests/output/nvenc_transcode/nvenc_transcode_h264_nvenc.mp4",
        c"h264_cuvid",
        c"h264_nvenc",
        AV_HWDEVICE_TYPE_CUDA,
        AV_PIX_FMT_CUDA,
        AV_PIX_FMT_NV12,
    )
    .unwrap();
}
