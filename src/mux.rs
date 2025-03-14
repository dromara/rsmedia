use crate::flags::MediaType;
use crate::io::{Reader, Writer};
use crate::stream::StreamInfo;
use crate::{Decoder, DecoderBuilder, Encoder, Packet, Rational};

use anyhow::{Context, Error, Result};

/// Represents a muxer. A muxer allows muxing media packets into a new container format. Muxing does
/// not require encoding and/or decoding.
///
/// # Examples
///
/// Mux to an MKV file:
///
/// ```rust,ignore
/// let reader = Reader::new(Path::new("from_file.mp4")).unwrap();
/// let writer = Writer::new(Path::new("to_file.mkv")).unwrap();
/// let muxer = MuxerBuilder::new(writer)
///     .with_streams(&reader)
///     .unwrap()
///     .build();
/// while let Ok(packet) = reader.read() {
///     muxer.mux(packet).unwrap();
/// }
/// muxer.finish().unwrap();
/// ```
///
/// Mux from file to MP4 and print length of first 100 buffer segments:
///
/// ```rust,ignore
/// let reader = Reader::new(Path::new("my_file.mp4")).unwrap();
/// let writer = BufferWriter::new("mp4").unwrap();
/// let mut muxer = MuxerBuilder::new(writer)
///     .with_streams(&reader)
///     .build()
///     .unwrap();
/// for _ in 0..100 {
///     println!("len: {}", muxer.mux().unwrap().len());
/// }
/// muxer.finish()?;
/// ```
pub struct Muxer<W: Writer> {
    pub writer: W,
    streams: Vec<MuxerStream>,
    interleaved: bool,
    have_written_header: bool,
    have_written_trailer: bool,
}

pub struct MuxerStream {
    pub time_base: Rational,
    pub media_type: MediaType,
    pub stream_idx: usize,
}

impl MuxerStream {
    pub fn new(index: usize, media_type: MediaType, time_base: Rational) -> Self {
        Self {
            time_base,
            media_type,
            stream_idx: index,
        }
    }
}

impl<W: Writer> Muxer<W> {
    pub fn from_writer(writer: W) -> Self {
        Self {
            writer,
            streams: Vec::new(),
            interleaved: false,
            have_written_header: false,
            have_written_trailer: false,
        }
    }

    pub fn add_stream(&mut self, encoder: &Encoder) -> Result<usize> {
        let stream_idx = self
            .writer
            .add_stream(encoder.codecpar(), encoder.time_base().into());
        self.streams.push(MuxerStream::new(
            stream_idx,
            encoder.media_type(),
            encoder.time_base(),
        ));
        Ok(stream_idx)
    }

    pub fn get_stream(&self, index: usize) -> Result<&MuxerStream> {
        self.streams
            .iter()
            .find(|s| s.stream_idx == index)
            .ok_or_else(|| Error::msg(format!("Stream index: {} not found", index)))
    }

    pub fn get_stream_mut(&mut self, index: usize) -> Result<&mut MuxerStream> {
        self.streams
            .iter_mut()
            .find(|s| s.stream_idx == index)
            .ok_or_else(|| Error::msg(format!("Stream index: {} not found", index)))
    }

    /// Mux a single packet. This will mux a single packet.
    ///
    /// # Arguments
    ///
    /// * `packet` - [`Packet`] to mux.
    pub fn mux(&mut self, packet: Packet, stream_idx: usize) -> Result<W::Out> {
        if self.have_written_header {
            let stream = self.streams.iter().find(|s| s.stream_idx == stream_idx);
            if stream.is_none() {
                return Err(Error::msg(format!(
                    "Stream index: {} not found",
                    stream_idx
                )));
            }

            let stream = stream.unwrap();
            let mut packet = packet;
            packet.set_pos(-1);
            packet.set_stream_index(stream.stream_idx);
            packet.rescale_ts(packet.time_base(), stream.time_base);

            Ok({
                if self.interleaved {
                    self.writer.write_interleaved(&mut packet)?
                } else {
                    self.writer.write_frame(&mut packet)?
                }
            })
        } else {
            self.have_written_header = true;
            self.writer.write_header()?;
            self.mux(packet, stream_idx)
        }
    }

    /// Signal to the muxer that writing has finished. This will cause a trailer to be written if
    /// the container format has one.
    pub fn finish(&mut self) -> Result<Option<W::Out>> {
        if self.have_written_header && !self.have_written_trailer {
            self.have_written_trailer = true;
            self.writer.write_trailer().map(Some)
        } else {
            Ok(None)
        }
    }
}

unsafe impl<W: Writer> Send for Muxer<W> {}
unsafe impl<W: Writer> Sync for Muxer<W> {}

/// Demuxer
#[allow(dead_code)]
pub struct Demuxer<R: Reader> {
    reader: R,
    streams: Vec<DemuxerStream>,
}

pub struct DemuxerStream {
    pub decoder: Decoder,
    pub time_base: Rational,   // 时间基准（如1/1000）
    pub media_type: MediaType, // 流类型（视频/音频/字幕）
    pub stream_idx: usize,     // 流索引（0-based）
}

impl DemuxerStream {
    pub fn new(decoder: Decoder) -> Self {
        let time_base = decoder.time_base();
        let media_type = decoder.media_type();
        let stream_idx = decoder.stream_index();
        Self {
            decoder,
            time_base,
            media_type,
            stream_idx,
        }
    }
}

impl<R: Reader> Demuxer<R> {
    pub fn from_reader(reader: R) -> Result<Self> {
        let nb_streams = reader.input().nb_streams as usize;
        let mut streams = Vec::new();
        for stream_idx in 0..nb_streams {
            let stream_info = StreamInfo::from_reader(&reader, stream_idx)?;
            let media_type = MediaType::from(stream_info.media_type.0);
            let decoder = DecoderBuilder::new()
                .with_media_type(media_type)
                .build(&reader)
                .context("Failed to build decoder")?;
            streams.push(DemuxerStream::new(decoder));
        }

        Ok(Self { reader, streams })
    }

    pub fn get_stream(&self, index: usize) -> Result<&DemuxerStream> {
        self.streams
            .iter()
            .find(|s| s.stream_idx == index)
            .ok_or_else(|| Error::msg(format!("Stream index: {} not found", index)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        EncoderBuilder, PixelFormat, SampleFormat, StreamReader, StreamWriter, StreamWriterBuilder,
    };

    use anyhow::{Context, Result};
    use rsmpeg::avutil::{AVChannelLayout, AVFrame};
    use std::path::Path;

    /// 生成YUV420P格式的测试视频帧
    fn generate_test_frame(width: u32, height: u32, pts: i64) -> AVFrame {
        let mut frame = AVFrame::new();
        frame.set_width(width as i32);
        frame.set_height(height as i32);
        frame.set_pts(pts);
        frame.set_format(PixelFormat::YUV420P.into());
        frame
            .alloc_buffer()
            .context("Failed to allocate buffer for frame")
            .unwrap();

        // 获取各平面参数 (YUV420P布局)
        let y_stride = frame.linesize[0] as usize;
        let u_stride = frame.linesize[1] as usize;
        let v_stride = frame.linesize[2] as usize;

        // 安全访问数据指针
        unsafe {
            let y_data = frame.data[0];
            // Y平面填充渐变
            for y in 0..height {
                for x in 0..width {
                    let y_val = ((x + y) % 256) as u8;
                    let idx = y as usize * y_stride + x as usize;
                    *y_data.add(idx) = y_val;
                }
            }

            // UV平面填充灰色 (128)
            let u_data = frame.data[1];
            let v_data = frame.data[2];
            for y in 0..(height / 2) {
                for x in 0..(width / 2) {
                    let u_idx = y as usize * u_stride + x as usize;
                    let v_idx = y as usize * v_stride + x as usize;
                    *u_data.add(u_idx) = 128;
                    *v_data.add(v_idx) = 128;
                }
            }
        }

        frame
    }

    /// 生成FLTP格式的正弦波音频帧
    fn generate_sine_wave_frame(freq: f32, nb_samples: usize, sample_rate: u32) -> Result<AVFrame> {
        let channels: usize = 2;
        let mut frame = AVFrame::new();
        frame.set_format(SampleFormat::FLTP as _);
        frame.set_ch_layout(AVChannelLayout::from_nb_channels(channels as i32).into_inner());
        frame.set_sample_rate(sample_rate as i32);
        frame.set_nb_samples(nb_samples as i32);
        frame
            .alloc_buffer()
            .context("Failed to allocate buffer for frame")?;

        let sample_interval = 1.0 / sample_rate as f32;
        let two_pi_f = 2.0 * std::f32::consts::PI * freq;

        for ch in 0..channels {
            let data_ptr = unsafe {
                std::slice::from_raw_parts_mut(
                    (*frame.as_mut_ptr()).data[ch] as *mut f32,
                    nb_samples,
                )
            };

            for (i, sample) in data_ptr.iter_mut().enumerate() {
                let t = i as f32 * sample_interval;
                let value = (two_pi_f * t).sin() * 0.8;
                *sample = value;
            }
        }

        Ok(frame)
    }

    #[test]
    fn test_mux_demux_video() -> Result<()> {
        let output_path = Path::new("/tmp/test_mux_demux_video.mp4");

        let (width, height) = (1920, 1080);
        let mut video_encoder = EncoderBuilder::new()
            .with_video_size(width, height)
            .with_frame_rate(30)
            .build()?;

        let stream_writer = StreamWriter::new(output_path)?;
        let mut muxer = Muxer::from_writer(stream_writer);
        let video_index = muxer.add_stream(&video_encoder)?;

        // 生成测试视频帧 // 10秒视频 30fps
        for pts in 0..300 {
            let frame = generate_test_frame(width, height, pts);

            match video_encoder.encode_raw(&frame) {
                Ok(Some(packet)) => muxer.mux(packet, video_index)?,
                Ok(None) => {
                    println!("No packet received from encoder.");
                }
                Err(e) => {
                    return Err(anyhow::anyhow!(
                        "Failed to receive packet from encoder: {}",
                        e
                    ));
                }
            }
        }

        // 完成写入
        video_encoder.flush().unwrap();
        muxer.finish().unwrap();

        // Demuxer 测试视频解码
        let stream_reader = StreamReader::new(output_path)?;
        let mut demuxer = Demuxer::from_reader(stream_reader)?;
        for des in demuxer.streams {
            println!("{:?}, {:?}", des.stream_idx, des.media_type)
        }

        loop {
            match demuxer.reader.read_packet() {
                Ok(Some((stream, packet))) => {
                    // if stream.index() == stream_index {
                    //     return Ok(Packet::new(packet, stream.time_base()));
                    // }
                    // log::debug!("Skipping packet from stream: {}", stream.index());
                    println!("{:?}", packet)
                }
                Ok(None) => {
                    // return Err(Error::msg("No more packets"))
                    break;
                }
                Err(e) => {
                    log::error!("Error reading packet: {}", e);
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    #[test]
    fn test_mux_demux_audio() -> Result<()> {
        let output_path = Path::new("/tmp/test_mux_demux_audio.aac");
        let sample_rate = 44100;
        let bit_rate = 128000;
        let channels = 2;
        let nb_samples = 1024;

        let writer = StreamWriterBuilder::new(output_path).build()?;
        // 添加音频流
        let mut audio_encoder = EncoderBuilder::new()
            .with_media_type(MediaType::AUDIO) // 指定音频编码
            .with_nb_channels(channels) // 立体声
            .with_sample_rate(sample_rate) // 采样率
            .with_bit_rate(bit_rate) // 128kbps 比特率
            .with_sample_format(SampleFormat::FLTP) // 平面浮点格式
            .with_codec_name("aac".to_string()) // 指定AAC编码
            .build()?;

        let stream_writer = StreamWriter::new(output_path)?;
        let mut muxer = Muxer::from_writer(stream_writer);
        let audio_index = muxer.add_stream(&audio_encoder)?;

        // 生成测试音频帧 // 5秒音频 440Hz
        for pts in (0..sample_rate * 5).step_by(nb_samples) {
            let mut sine_frame = generate_sine_wave_frame(440.0, nb_samples, sample_rate)?;
            sine_frame.set_pts(pts as i64);

            match audio_encoder.encode_raw(&sine_frame) {
                Ok(Some(packet)) => {
                    muxer.mux(packet, audio_index).unwrap();
                }
                Ok(None) => {
                    println!("No packet received from encoder.");
                }
                Err(e) => {
                    return Err(anyhow::anyhow!(
                        "Failed to receive packet from encoder: {}",
                        e
                    ));
                }
            }
        }

        // 完成写入
        audio_encoder.flush().unwrap();
        muxer.finish().unwrap();

        // Demuxer 测试音频解码
        let stream_reader = StreamReader::new(output_path)?;
        let mut demuxer = Demuxer::from_reader(stream_reader)?;
        for des in demuxer.streams {
            println!("{:?}, {:?}", des.stream_idx, des.media_type)
        }

        loop {
            match demuxer.reader.read_packet() {
                Ok(Some((stream, packet))) => {
                    // if stream.index() == stream_index {
                    //     return Ok(Packet::new(packet, stream.time_base()));
                    // }
                    // log::debug!("Skipping packet from stream: {}", stream.index());
                    println!("{:?}", packet)
                }
                Ok(None) => {
                    // return Err(Error::msg("No more packets"))
                    break;
                }
                Err(e) => {
                    log::error!("Error reading packet: {}", e);
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    #[test]
    fn test_multiple_streams() -> Result<()> {
        // 视频参数
        pub const VIDEO_WIDTH: u32 = 1280;
        pub const VIDEO_HEIGHT: u32 = 720;
        pub const VIDEO_FPS: i32 = 30;
        pub const VIDEO_DURATION: i32 = 10; // 视频延时 单位：秒

        // 音频参数
        pub const AUDIO_SAMPLE_RATE: u32 = 48000;
        pub const AUDIO_CHANNELS: u32 = 2;
        pub const AUDIO_BITRATE: i64 = 128_000;
        pub const SAMPLES_PER_FRAME: u32 = 1024;

        let output_path = Path::new("/tmp/test_multiple_streams.mp4");

        let mut video_encoder = EncoderBuilder::new()
            .with_media_type(MediaType::VIDEO)
            .with_video_size(VIDEO_WIDTH, VIDEO_HEIGHT)
            .with_format("mp4")
            .with_frame_rate(VIDEO_FPS)
            .build()?;

        let mut audio_encoder = EncoderBuilder::new()
            .with_media_type(MediaType::AUDIO) // 指定音频编码
            .with_nb_channels(AUDIO_CHANNELS) // 立体声
            .with_sample_rate(AUDIO_SAMPLE_RATE) // 采样率
            .with_bit_rate(AUDIO_BITRATE) // 比特率
            .with_sample_format(SampleFormat::FLTP) // 平面浮点格式
            .with_codec_name("aac".to_string()) // 指定AAC编码
            .build()?;

        let stream_writer = StreamWriter::new(output_path)?;
        let mut muxer = Muxer::from_writer(stream_writer);

        // 添加流
        let video_idx = muxer.add_stream(&video_encoder)?;
        let audio_idx = muxer.add_stream(&audio_encoder)?;

        // 计算音频帧间隔
        let audio_frame_interval = {
            let audio_duration = SAMPLES_PER_FRAME as f64 / AUDIO_SAMPLE_RATE as f64;
            let video_duration = 1.0 / VIDEO_FPS as f64;
            (audio_duration / video_duration).ceil() as i32
        };

        // 生成测试数据
        let total_frames = VIDEO_FPS * VIDEO_DURATION;
        for pts in 0..total_frames {
            // 处理视频帧
            let video_frame = generate_test_frame(VIDEO_WIDTH, VIDEO_HEIGHT, pts as i64);
            match video_encoder.encode_raw(&video_frame) {
                Ok(Some(packet)) => {
                    muxer.mux(packet, video_idx).unwrap();
                }
                Ok(None) => {
                    println!("No packet received from video_encoder.");
                }
                Err(e) => {
                    return Err(anyhow::anyhow!(
                        "Failed to receive packet from video_encoder: {}",
                        e
                    ));
                }
            }

            // 处理音频帧
            if pts % audio_frame_interval == 0 {
                let audio_frame = generate_sine_wave_frame(
                    1000.0,
                    SAMPLES_PER_FRAME as usize,
                    AUDIO_SAMPLE_RATE,
                )?;
                match audio_encoder.encode_raw(&audio_frame) {
                    Ok(Some(packet)) => {
                        muxer.mux(packet, audio_idx).unwrap();
                    }
                    Ok(None) => {
                        println!("No packet received from audio_encoder.");
                    }
                    Err(e) => {
                        return Err(anyhow::anyhow!(
                            "Failed to receive packet from audio_encoder: {}",
                            e
                        ));
                    }
                }
            }
        }

        // 收尾工作
        video_encoder.flush().unwrap();
        audio_encoder.flush().unwrap();
        muxer.finish().unwrap();

        // 解封装验证
        let stream_reader = StreamReader::new(output_path)?;
        let demuxer = Demuxer::from_reader(stream_reader)?;
        for stream in demuxer.streams {
            println!("{:?}, {:?}", stream.stream_idx, stream.media_type)
        }

        Ok(())
    }
}
