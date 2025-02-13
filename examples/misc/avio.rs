use std::ffi::{CStr, CString};
use std::fs::File;
use std::io::Seek;
use std::io::{SeekFrom, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Context, Result};
use image::DynamicImage;

use rsmpeg::{
    avcodec::{AVCodec, AVCodecContext, AVPacket},
    avformat::{
        AVFormatContextInput, AVFormatContextOutput, AVIOContextContainer, AVIOContextCustom,
    },
    avutil::{self, AVFrame, AVMem, AVRational},
    ffi,
};

/// multimedia file input decoding
pub struct Decoder {
    stream_index: usize,
    codec_context: AVCodecContext,
    format_context: AVFormatContextInput,
    current_packet: Option<AVPacket>,
}

impl Decoder {
    pub fn new(source: &str) -> Result<Self> {
        let (stream_idx, input_format_context, decode_context) =
            open_input_file(CString::new(source).unwrap().as_c_str())?;
        Ok(Decoder {
            stream_index: stream_idx,
            codec_context: decode_context,
            format_context: input_format_context,
            current_packet: None,
        })
    }

    pub fn get_framerate(&self) -> Result<u64> {
        let stream = &self.format_context.streams()[self.stream_index];
        let frame_rate = stream.guess_framerate().unwrap();
        Ok((frame_rate.num as f64 / frame_rate.den as f64) as u64)
    }

    pub fn decode_iter(
        &mut self,
    ) -> impl Iterator<Item = Result<(i64, DynamicImage), anyhow::Error>> + '_ {
        std::iter::from_fn(move || match self.decode_next() {
            Ok(Some(frame)) => Some(Ok(frame)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        })
    }

    fn decode_next(&mut self) -> Result<Option<(i64, DynamicImage)>, anyhow::Error> {
        loop {
            // 在这里实现解码逻辑
            if self.current_packet.is_none() {
                while let Some(packet) = self.format_context.read_packet()? {
                    if packet.stream_index as usize == self.stream_index {
                        self.current_packet = Some(packet);
                        break;
                    }
                }
            }

            println!(
                "decode_next current_packet: {}",
                self.current_packet.is_some()
            );

            if let Some(packet) = self.current_packet.take() {
                println!(
                    "packet pts: {}, dts: {}, duration: {}, size: {}, stream_index: {}",
                    packet.pts, packet.dts, packet.duration, packet.size, packet.stream_index
                );

                self.codec_context
                    .send_packet(Some(&packet))
                    .context("Failed to send packet to codec context")?;

                while let Ok(yuv_frame) = self.codec_context.receive_frame() {
                    // 注意这里的 frame 编码格式为 YUV420P，需要转换为 RGB24
                    let rgb_frame = crate::misc::av_convert::avframe_yuv_to_rgb24(&yuv_frame)?;
                    println!(
                        "convert frame from yuv420p to rgb24 pts: {}, time_base: {:?}",
                        rgb_frame.pts, rgb_frame.time_base
                    );

                    let img = crate::misc::av_convert::avframe_rgb24_to_rgb_image_2(&rgb_frame)?;
                    return Ok(Some((yuv_frame.pts, DynamicImage::ImageRgb8(img))));
                }
            } else {
                println!("No packet received for stream_index: {}", self.stream_index);
                break;
            }
        }

        Ok(None)
    }
}

/// Get `video_stream_index`, `input_format_context`, `decode_context`.
pub fn open_input_file(
    filename: &CStr,
) -> anyhow::Result<(usize, AVFormatContextInput, AVCodecContext)> {
    let mut input_format_context = AVFormatContextInput::open(filename, None, &mut None)?;
    input_format_context.dump(0, filename)?;

    let (video_index, decoder) = input_format_context
        .find_best_stream(ffi::AVMEDIA_TYPE_VIDEO)
        .context("Failed to select a video stream")?
        .context("No video stream")?;

    println!(
        "open_input_file: video_index: {}, decoder: {:?}",
        video_index,
        decoder.name().to_str()?
    );

    let decode_context = {
        let input_stream = &input_format_context.streams()[video_index];

        let mut decode_context = AVCodecContext::new(&decoder);
        decode_context.apply_codecpar(&input_stream.codecpar())?;
        if let Some(framerate) = input_stream.guess_framerate() {
            decode_context.set_framerate(framerate);
        }
        decode_context.open(None)?;
        decode_context
    };

    Ok((video_index, input_format_context, decode_context))
}

/// Return output_format_context and encode_context
pub fn open_output_file(
    filename: &CStr,
    decode_context: &AVCodecContext,
) -> anyhow::Result<(AVFormatContextOutput, AVCodecContext)> {
    let buffer = Arc::new(Mutex::new(File::create(filename.to_str()?)?));
    let buffer1 = buffer.clone();

    // Custom IO Context
    let io_context = AVIOContextCustom::alloc_context(
        AVMem::new(4096),
        true,
        vec![],
        None,
        Some(Box::new(move |_: &mut Vec<u8>, buf: &[u8]| {
            let mut buffer = buffer1.lock().unwrap();
            buffer.write_all(buf).unwrap();
            buf.len() as _
        })),
        Some(Box::new(
            move |_: &mut Vec<u8>, offset: i64, whence: i32| {
                println!("offset: {}, whence: {}", offset, whence);
                let mut buffer = match buffer.lock() {
                    Ok(x) => x,
                    Err(_) => return -1,
                };
                let mut seek_ = |offset: i64, whence: i32| -> anyhow::Result<i64> {
                    Ok(match whence {
                        libc::SEEK_CUR => buffer.seek(SeekFrom::Current(offset))?,
                        libc::SEEK_SET => buffer.seek(SeekFrom::Start(offset as u64))?,
                        libc::SEEK_END => buffer.seek(SeekFrom::End(offset))?,
                        _ => return Err(anyhow!("Unsupported whence")),
                    } as i64)
                };
                seek_(offset, whence).unwrap_or(-1)
            },
        )),
    );

    let mut output_format_context =
        AVFormatContextOutput::create(filename, Some(AVIOContextContainer::Custom(io_context)))?;

    let encoder = AVCodec::find_encoder(ffi::AV_CODEC_ID_H264)
        .with_context(|| anyhow!("encoder({}) not found.", ffi::AV_CODEC_ID_H264))?;

    let mut encode_context = AVCodecContext::new(&encoder);
    encode_context.set_height(decode_context.height);
    encode_context.set_width(decode_context.width);
    encode_context.set_sample_aspect_ratio(decode_context.sample_aspect_ratio);
    encode_context.set_pix_fmt(if let Some(pix_fmts) = encoder.pix_fmts() {
        pix_fmts[0]
    } else {
        decode_context.pix_fmt
    });
    encode_context.set_time_base(avutil::av_inv_q(avutil::av_mul_q(
        decode_context.framerate,
        AVRational {
            num: decode_context.ticks_per_frame,
            den: 1,
        },
    )));

    // Some formats want stream headers to be separate.
    if output_format_context.oformat().flags & ffi::AVFMT_GLOBALHEADER as i32 != 0 {
        encode_context.set_flags(encode_context.flags | ffi::AV_CODEC_FLAG_GLOBAL_HEADER as i32);
    }

    encode_context.open(None)?;

    {
        let mut out_stream = output_format_context.new_stream();
        out_stream.set_codecpar(encode_context.extract_codecpar());
        out_stream.set_time_base(encode_context.time_base);
    }

    output_format_context.dump(0, filename)?;
    output_format_context.write_header(&mut None)?;

    Ok((output_format_context, encode_context))
}

/// filename (&CStr): 这是一个指向 C 字符串的指针，表示输出视频文件的名称
/// width (i32): 输出视频的宽度，以像素为单位。
/// height (i32): 输出视频的高度，以像素为单位。
/// ratio (AVRational): 输出视频的纵横比，表示为一个分数
/// framerate (AVRational): 输出视频的帧率，表示为每秒帧数 (fps)
/// ticks_per_frame (i32): 每帧的时间戳增量。
pub fn open_output_file_custom(
    filename: &CStr,
    width: i32,
    height: i32,
    ratio: AVRational,
    framerate: AVRational,
    ticks_per_frame: i32,
) -> anyhow::Result<(AVFormatContextOutput, AVCodecContext)> {
    let buffer = Arc::new(Mutex::new(File::create(filename.to_str()?)?));
    let buffer1 = buffer.clone();

    // Custom IO Context
    let io_context = AVIOContextCustom::alloc_context(
        AVMem::new(4096),
        true,
        vec![],
        None,
        Some(Box::new(move |_: &mut Vec<u8>, buf: &[u8]| {
            let mut buffer = buffer1.lock().unwrap();
            buffer.write_all(buf).unwrap();
            buf.len() as _
        })),
        Some(Box::new(
            move |_: &mut Vec<u8>, offset: i64, whence: i32| {
                println!("offset: {}, whence: {}", offset, whence);
                let mut buffer = match buffer.lock() {
                    Ok(x) => x,
                    Err(_) => return -1,
                };
                let mut seek_ = |offset: i64, whence: i32| -> anyhow::Result<i64> {
                    Ok(match whence {
                        libc::SEEK_CUR => buffer.seek(SeekFrom::Current(offset))?,
                        libc::SEEK_SET => buffer.seek(SeekFrom::Start(offset as u64))?,
                        libc::SEEK_END => buffer.seek(SeekFrom::End(offset))?,
                        _ => return Err(anyhow!("Unsupported whence")),
                    } as i64)
                };
                seek_(offset, whence).unwrap_or(-1)
            },
        )),
    );

    let mut output_format_context =
        AVFormatContextOutput::create(filename, Some(AVIOContextContainer::Custom(io_context)))?;

    let encoder = AVCodec::find_encoder(ffi::AV_CODEC_ID_H264)
        .with_context(|| anyhow!("encoder({}) not found.", ffi::AV_CODEC_ID_H264))?;

    let mut encode_context = AVCodecContext::new(&encoder);
    encode_context.set_width(width);
    encode_context.set_height(height);
    encode_context.set_sample_aspect_ratio(ratio);
    encode_context.set_pix_fmt(if let Some(pix_fmts) = encoder.pix_fmts() {
        pix_fmts[0]
    } else {
        ffi::AV_PIX_FMT_YUV420P
    });

    encode_context.set_time_base(avutil::av_inv_q(avutil::av_mul_q(
        framerate,
        AVRational {
            num: ticks_per_frame,
            den: 1,
        },
    )));

    // Some formats want stream headers to be separate.
    if output_format_context.oformat().flags & ffi::AVFMT_GLOBALHEADER as i32 != 0 {
        encode_context.set_flags(encode_context.flags | ffi::AV_CODEC_FLAG_GLOBAL_HEADER as i32);
    }

    encode_context.open(None)?;

    {
        let mut out_stream = output_format_context.new_stream();
        out_stream.set_codecpar(encode_context.extract_codecpar());
        out_stream.set_time_base(encode_context.time_base);
    }

    output_format_context.dump(0, filename)?;
    output_format_context.write_header(&mut None)?;

    Ok((output_format_context, encode_context))
}

/// Save a `AVFrame` as pgm file.
pub fn pgm_save(frame: &AVFrame, filename: &str) -> Result<()> {
    // Here we only capture the first layer of frame.
    let data = frame.data[0];
    let linesize = frame.linesize[0] as usize;
    let width = frame.width as usize;
    let height = frame.height as usize;

    let buffer = unsafe { std::slice::from_raw_parts(data, linesize * height) };

    // Create pgm file
    let mut pgm_file = std::fs::File::create(filename)?;

    // Write pgm header
    pgm_file.write_all(&format!("P5\n{} {}\n{}\n", width, height, 255).into_bytes())?;

    // Write pgm data
    for i in 0..height {
        pgm_file.write_all(&buffer[i * linesize..i * linesize + width])?;
    }

    pgm_file.flush()?;

    println!("pgm file saved to {}", filename);

    Ok(())
}

pub fn save_image_avframe_yuv(yuv_frame: &AVFrame, output_file_name: &str) -> anyhow::Result<()> {
    // 转换为 RGB24 格式
    let rgb_frame = crate::misc::av_convert::avframe_yuv_to_rgb24(yuv_frame)?;

    // 保存图像
    save_image_avframe_rgb24(&rgb_frame, output_file_name)
        .expect("save_image_avframe_rgb24 failed.");

    Ok(())
}

pub fn save_image_avframe_rgb24(
    rgb_frame: &AVFrame,
    output_file_name: &str,
) -> anyhow::Result<(), Box<dyn std::error::Error>> {
    if rgb_frame.format != ffi::AV_PIX_FMT_RGB24 {
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Unsupported pixel format",
        )));
    }

    // 转换为图像
    let rgb_image = crate::misc::av_convert::avframe_rgb24_to_rgb_image(rgb_frame)?;

    // 确定输出格式并写入文件
    let path = Path::new(output_file_name);
    let extension = path
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .ok_or_else(|| "Unknown image format")?;

    match extension.to_lowercase().as_str() {
        "png" => rgb_image.save_with_format(path, image::ImageFormat::Png)?,
        "jpg" | "jpeg" => rgb_image.save_with_format(path, image::ImageFormat::Jpeg)?,
        _ => {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Unsupported image format",
            )))
        }
    }

    println!("Image saved to {}", output_file_name);

    Ok(())
}
