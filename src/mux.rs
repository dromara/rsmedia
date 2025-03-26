use crate::decode::DecodeRawResult;
use crate::encode::EncodeRawResult;
use crate::flags::MediaType;
use crate::hwaccel::HWDeviceConfig;
use crate::io::{private::Output, Reader, Writer};
use crate::stream::StreamInfo;
use crate::{utils, Decoder, DecoderBuilder, Encoder, Resize, StreamReader, StreamWriter};

use rsmpeg::avutil::AVFrame;

use anyhow::{Context, Error, Result};
use std::path::Path;

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
    pub encoder: Encoder,
    pub stream_info: StreamInfo,
    pub media_type: MediaType,
    pub stream_idx: usize,
}

impl MuxerStream {
    pub fn new(encoder: Encoder, stream_info: StreamInfo) -> Self {
        let media_type = encoder.media_type();
        let stream_idx = stream_info.index;
        Self {
            encoder,
            stream_info,
            stream_idx,
            media_type,
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

    pub fn add_stream(&mut self, encoder: Encoder) -> Result<usize> {
        let stream_idx = self
            .writer
            .add_stream(encoder.codecpar(), encoder.time_base());
        let stream_info = StreamInfo::from_writer(&self.writer, stream_idx)?;
        self.streams.push(MuxerStream::new(encoder, stream_info));
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
    pub fn mux(&mut self, frame: AVFrame, stream_idx: usize) -> Result<Option<W::Out>> {
        if self.have_written_header {
            let mux_stream = self.get_stream_mut(stream_idx)?;

            match mux_stream.encoder.encode_raw(&frame) {
                EncodeRawResult::Packet(mut packet) => {
                    packet.set_pos(-1);
                    packet.set_stream_index(mux_stream.stream_idx as i32);
                    // 将编码器输出的数据包时间戳，从编码器时间基转换到输出流时间基
                    // encode_ctx_timebase => out_stream_time_base
                    packet.rescale_ts(
                        mux_stream.encoder.time_base(),
                        mux_stream.stream_info.time_base,
                    );

                    Ok(Some({
                        if self.interleaved {
                            self.writer.write_interleaved(&mut packet)?
                        } else {
                            self.writer.write_frame(&mut packet)?
                        }
                    }))
                }
                EncodeRawResult::Drain | EncodeRawResult::Flushed => {
                    log::debug!("Encoder Drain or Flushed.");
                    Ok(None)
                }
                EncodeRawResult::Error(e) => Err(e),
            }
        } else {
            self.have_written_header = true;
            self.writer.write_header()?;
            self.mux(frame, stream_idx)
        }
    }

    /// Signal to the muxer that writing has finished. This will cause a trailer to be written if
    /// the container format has one.
    pub fn finish(&mut self) -> Result<Option<W::Out>> {
        for mux_stream in self.streams.iter_mut() {
            // flush the encoder to ensure all packets are sent to the muxer.
            let out_stream_index = mux_stream.stream_idx;
            let out_stream_time_base = mux_stream.stream_info.time_base;
            mux_stream.encoder.flush(
                &mut self.writer,
                self.interleaved,
                out_stream_index,
                out_stream_time_base,
            )?;
        }

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
pub struct Demuxer<R: Reader> {
    pub reader: R,
    streams: Vec<DemuxerStream>,
}

/// stream definition for demuxer
pub struct DemuxerStream {
    pub decoder: Decoder,
    pub stream_info: StreamInfo,
    pub media_type: MediaType,
    pub stream_idx: usize,
}

impl DemuxerStream {
    pub fn new(decoder: Decoder, stream_info: StreamInfo) -> Self {
        let media_type = decoder.media_type();
        let stream_idx = stream_info.index;
        Self {
            decoder,
            stream_info,
            media_type,
            stream_idx,
        }
    }
}

/// Demux result
#[derive(Debug)]
pub enum DemuxResult {
    /// decoded frame
    Frame(usize, AVFrame),
    /// need more data
    Drain,
    /// EOF of input
    Flushed,
    /// error
    Error(Error),
}

impl<R: Reader> Demuxer<R> {
    pub fn from_reader(
        reader: R,
        resize: Option<Resize>,
        device_config: Option<HWDeviceConfig>,
    ) -> Result<Self> {
        let device_type = device_config.as_ref().map(|c| c.device_type);
        let nb_streams = reader.input().nb_streams as usize;
        let mut streams = Vec::new();
        for stream_idx in 0..nb_streams {
            let stream_info = StreamInfo::from_reader(&reader, stream_idx)?;
            // auto detect hardware acceleration decoder codec
            let codec_name = stream_info.find_decoder_name(device_type);
            let decoder = DecoderBuilder::new(stream_info.media_type)
                .with_hardware_device(device_config.clone())
                .with_codec_name(codec_name)
                .with_resize(resize)
                .build(&reader)
                .context("Failed to build decoder")?;
            streams.push(DemuxerStream::new(decoder, stream_info));
        }

        Ok(Self { reader, streams })
    }

    pub fn streams(&self) -> &[DemuxerStream] {
        &self.streams
    }

    pub fn get_stream(&self, index: usize) -> Result<&DemuxerStream> {
        self.streams
            .iter()
            .find(|s| s.stream_idx == index)
            .ok_or_else(|| Error::msg(format!("Stream index: {} not found", index)))
    }

    pub fn get_stream_mut(&mut self, index: usize) -> Result<&mut DemuxerStream> {
        self.streams
            .iter_mut()
            .find(|s| s.stream_idx == index)
            .ok_or_else(|| Error::msg(format!("Stream index: {} not found", index)))
    }

    pub fn demux(&mut self) -> DemuxResult {
        match self.internal_demux() {
            (index, DecodeRawResult::Frame(frame)) => DemuxResult::Frame(index, frame),
            (_i, DecodeRawResult::Drain) => DemuxResult::Drain,
            (_i, DecodeRawResult::Flushed) => DemuxResult::Flushed,
            (_i, DecodeRawResult::Error(e)) => DemuxResult::Error(e),
        }
    }

    /// handle demux internal logic
    fn internal_demux(&mut self) -> (usize, DecodeRawResult) {
        let (in_stream_index, in_stream_time_base, mut packet) = {
            let (in_stream, pkt) = match self.reader.read_packet() {
                Ok(Some((s, p))) => (s, p),
                Ok(None) => return (0, DecodeRawResult::Flushed),
                Err(e) => {
                    log::error!("Error reading packet: {}", e);
                    return (0, DecodeRawResult::Error(e));
                }
            };
            (in_stream.index(), in_stream.time_base(), pkt)
        };

        let demux_stream = self.get_stream_mut(in_stream_index).unwrap();
        // 解码前处理输入数据包, 将输入容器的时间基转换为解码器的时间基
        // in_stream->time_base  =>  dec_ctx->time_base
        packet.set_pos(-1);
        packet.set_stream_index(in_stream_index as i32);
        packet.rescale_ts(in_stream_time_base, demux_stream.decoder.time_base());
        (in_stream_index, demux_stream.decoder.decode_raw(&packet))
    }
}

/// Demuxer iterator
///
/// # Examples
///
/// ```rust,ignore
/// let mut demuxer = Demuxer::from_reader(StreamReader::new(Path::new("my_file.mp4"))?)?;
/// for (stream_index, frame) in demuxer {
///     println!("stream_index: {}, frame: {}", stream_index, frame.width());
/// }
/// ```
impl<R: Reader> Iterator for Demuxer<R> {
    type Item = Result<(usize, AVFrame)>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.demux() {
            DemuxResult::Frame(stream_index, frame) => Some(Ok((stream_index, frame))),
            DemuxResult::Drain => self.next(),
            DemuxResult::Flushed => None,
            DemuxResult::Error(e) => Some(Err(e)),
        }
    }
}

/// transcode from one container format to another
///
/// # Examples
///
/// ```rust,ignore
/// transcode("input.mp4", "output.mov").unwrap();
/// ```
pub fn transcode(input_path: &str, output_path: &str) -> Result<()> {
    let mut input_reader = StreamReader::new(Path::new(input_path))?;
    let input = input_reader.input();

    let mut output_writer = StreamWriter::new(Path::new(output_path))?;
    let output = output_writer.output_mut();

    let stream_mapping: Vec<_> = {
        let mut stream_index = 0usize;
        input
            .streams()
            .iter()
            .map(|stream| {
                let codec_type = stream.codecpar().codec_type();
                if !codec_type.is_video() && !codec_type.is_audio() && !codec_type.is_subtitle() {
                    None
                } else {
                    output.new_stream().set_codecpar(stream.codecpar().clone());
                    stream_index += 1;
                    Some(stream_index - 1)
                }
            })
            .collect()
    };

    output
        .dump(0, utils::from_str(output_path).as_c_str())
        .context("Dump output format context failed.")?;

    output
        .write_header(&mut None)
        .context("Writer header failed.")?;

    while let Some((in_stream, mut packet)) =
        input_reader.read_packet().context("Read packet failed.")?
    {
        let input_stream_index = in_stream.index();
        let Some(output_stream_index) = stream_mapping[input_stream_index] else {
            continue;
        };
        {
            let output_stream = &output.streams()[output_stream_index];
            packet.rescale_ts(in_stream.time_base(), output_stream.time_base);
            packet.set_stream_index(output_stream_index as i32);
            packet.set_pos(-1);
        }
        output
            .interleaved_write_frame(&mut packet)
            .context("Interleaved write frame failed.")?;
    }

    output.write_trailer().context("Write trailer failed.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EncoderBuilder, PixelFormat, SampleFormat, StreamReader, StreamWriter};

    use anyhow::{Context, Result};
    use rsmpeg::avutil::{AVChannelLayout, AVFrame};
    use std::path::Path;

    /// 生成YUV420P格式的视频帧,彩色渐变测试图
    fn generate_video_frame(width: u32, height: u32, frame_index: i64) -> AVFrame {
        let mut frame = AVFrame::new();
        frame.set_width(width as i32);
        frame.set_height(height as i32);
        frame.set_format(PixelFormat::YUV420P.into());
        frame
            .alloc_buffer()
            .context("Failed to allocate buffer for frame")
            .unwrap();

        // 获取各平面参数 (YUV420P布局)
        let y_plane = frame.data_mut()[0];
        let u_plane = frame.data_mut()[1];
        let v_plane = frame.data_mut()[2];

        let y_linesize = frame.linesize[0];
        let u_linesize = frame.linesize[1];
        let v_linesize = frame.linesize[2];

        // 基于帧索引创建动态效果
        let time_factor = (frame_index as f32 * 0.05).sin() * 0.5 + 0.5;

        // 填充Y平面 (亮度)
        for y in 0..height {
            for x in 0..width {
                let index = (y * y_linesize as u32 + x) as usize;
                let gradient = (x as f32 / width as f32 * 255.0) as u8;
                unsafe {
                    *y_plane.add(index) = gradient;
                }
            }
        }

        // 填充U平面 (蓝色分量)
        for y in 0..(height / 2) {
            for x in 0..(width / 2) {
                let index = (y * u_linesize as u32 + x) as usize;
                let u_value = ((time_factor * 128.0) as u8).wrapping_add(128);
                unsafe {
                    *u_plane.add(index) = u_value;
                }
            }
        }

        // 填充V平面 (红色分量)
        for y in 0..(height / 2) {
            for x in 0..(width / 2) {
                let index = (y * v_linesize as u32 + x) as usize;
                let v_value = (((1.0 - time_factor) * 128.0) as u8).wrapping_add(128);
                unsafe {
                    *v_plane.add(index) = v_value;
                }
            }
        }

        frame
    }

    /// 生成FLTP格式的正弦波音频帧
    fn generate_audio_sine_wave_frame(
        freq: f32,
        channels: usize,
        nb_samples: usize,
        sample_rate: u32,
    ) -> Result<AVFrame> {
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
                let ptr = (*frame.as_mut_ptr()).data[ch] as *mut f32;
                anyhow::ensure!(!ptr.is_null(), "Audio data pointer is null");
                std::slice::from_raw_parts_mut(ptr, nb_samples)
            };

            // 生成正弦波
            data_ptr.iter_mut().enumerate().for_each(|(i, sample)| {
                let t = i as f32 * sample_interval;
                *sample = (two_pi_f * t).sin() * 0.8;
            });
        }

        Ok(frame)
    }

    #[test]
    #[ignore = "demux video"]
    fn test_mux_demux_video() -> Result<()> {
        let output_path = Path::new("/tmp/test_mux_demux_video.mp4");

        let (width, height) = (1920, 1080);
        let video_encoder = Encoder::new_video(width, height).unwrap();

        let stream_writer = StreamWriter::new(output_path)?;
        let mut muxer = Muxer::from_writer(stream_writer);

        let encoder_frame_rate = video_encoder.frame_rate();
        let encoder_time_base = video_encoder.time_base();
        let video_index = muxer.add_stream(video_encoder)?;

        // 生成测试视频帧 // 10秒视频 30fps
        for index in 0..10 * encoder_frame_rate.den as i64 {
            let mut frame = generate_video_frame(width, height, index);
            frame.set_pts(index * encoder_time_base.den as i64);
            frame.set_time_base(encoder_time_base);

            println!(
                "encode video frame:{:?}, time_base:{:?}, encoder_time_base:{:?}",
                frame, frame.time_base, encoder_time_base
            );
            muxer.mux(frame, video_index)?;
        }

        // 完成写入
        muxer.finish().unwrap();

        //////////////////////////////////////////////////////////////////
        //////////////////////////////////////////////////////////////////

        // Demuxer 测试视频解码
        let stream_reader = StreamReader::new(output_path)?;
        let demuxer = Demuxer::from_reader(stream_reader, None, None)?;
        for des in &demuxer.streams {
            println!("{:?}, {:?}", des.stream_idx, des.media_type)
        }

        for res in demuxer {
            match res {
                Ok((index, frame)) => {
                    println!(
                        "stream index:{}, {:?}, timebase:{:?}",
                        index, frame, frame.time_base
                    );
                }
                Err(e) => {
                    println!("Error decoding frame: {}", e)
                }
            }
        }

        Ok(())
    }

    #[test]
    #[ignore = "test_mux_demux_audio_aac 需要写文件操作"]
    fn test_mux_demux_audio_aac() -> Result<()> {
        let output_path = Path::new("/tmp/test_mux_demux_audio_aac.aac");
        let sample_rate = 44_100;
        // aac 要求输入的样本数必须是1024
        let nb_samples = 1024;
        let channels = 2;

        // 添加音频流
        let audio_encoder = Encoder::new_audio(channels, sample_rate, SampleFormat::FLTP).unwrap();
        let stream_writer = StreamWriter::new(output_path)?;
        let mut muxer = Muxer::from_writer(stream_writer);

        let encoder_time_base = audio_encoder.time_base();
        let audio_index = muxer.add_stream(audio_encoder)?;

        // 累积的样本数，用于计算PTS
        let mut total_samples = 0;

        // 生成测试音频帧 // 5秒音频 440Hz
        for _ in 0..(sample_rate * 5 / nb_samples) {
            let mut sine_frame = generate_audio_sine_wave_frame(
                440.0,
                channels as usize,
                nb_samples as usize,
                sample_rate,
            )?;

            // 设置正确的PTS和时间基
            sine_frame.set_pts(total_samples);
            sine_frame.set_time_base(encoder_time_base);

            println!(
                "audio frame: {:?}, time_base={:?}",
                sine_frame, encoder_time_base
            );

            muxer.mux(sine_frame, audio_index)?;

            // 更新累积的样本数
            total_samples += nb_samples as i64;
        }

        muxer.finish().unwrap();

        //////////////////////////////////////////////////////////////////
        //////////////////////////////////////////////////////////////////

        // Demuxer 测试音频解码
        let stream_reader = StreamReader::new(output_path)?;
        let demuxer = Demuxer::from_reader(stream_reader, None, None)?;
        for des in &demuxer.streams {
            println!("{:?}, {:?}", des.stream_idx, des.media_type)
        }

        for res in demuxer {
            match res {
                Ok((index, frame)) => {
                    println!(
                        "stream index:{}, {:?}, time_base:{:?}",
                        index, frame, frame.time_base
                    )
                }
                Err(e) => {
                    println!("Error decoding frame: {}", e)
                }
            }
        }

        Ok(())
    }

    #[test]
    #[ignore = "test_mux_demux_audio_mp3 需要写文件操作"]
    fn test_mux_demux_audio_mp3() -> Result<()> {
        let output_path = Path::new("/tmp/test_mux_demux_audio_mp3.mp3");
        let sample_rate = 44_100;
        let bit_rate = 128_000;
        // MP3通常使用1152个样本/帧
        let nb_samples = 1152;
        let channels = 2;

        // 修改音频编码器为MP3
        let audio_encoder =
            EncoderBuilder::new_audio(bit_rate, channels, sample_rate, SampleFormat::FLTP)
                // 使用LAME MP3编码器
                .with_codec_name(Some("libmp3lame".to_string()))
                .build()?;

        let stream_writer = StreamWriter::new(output_path)?;
        let mut muxer = Muxer::from_writer(stream_writer);

        let encoder_time_base = audio_encoder.time_base();
        let audio_index = muxer.add_stream(audio_encoder)?;

        // 累积的样本数，用于计算PTS
        let mut total_samples = 0;

        // 生成测试音频帧 // 5秒音频 440Hz
        for _ in 0..(sample_rate * 5 / nb_samples) {
            let mut sine_frame = generate_audio_sine_wave_frame(
                440.0,
                channels as usize,
                nb_samples as usize,
                sample_rate,
            )?;

            // 设置正确的PTS和时间基
            sine_frame.set_pts(total_samples);
            sine_frame.set_time_base(encoder_time_base);

            println!(
                "audio frame: {:?}, time_base={:?}",
                sine_frame, encoder_time_base
            );

            muxer.mux(sine_frame, audio_index)?;

            // 更新累积的样本数
            total_samples += nb_samples as i64;
        }

        muxer.finish().unwrap();

        Ok(())
    }

    #[test]
    #[ignore = "demux test_multiple_streams"]
    fn test_multiple_streams() -> Result<()> {
        // 视频参数
        pub const VIDEO_WIDTH: u32 = 1280;
        pub const VIDEO_HEIGHT: u32 = 720;
        pub const VIDEO_FPS: u32 = 30;
        pub const VIDEO_DURATION_SEC: u32 = 10;

        // 音频参数
        pub const AUDIO_SAMPLE_RATE: u32 = 48_000;
        pub const AUDIO_CHANNELS: u32 = 2;
        pub const SAMPLES_PER_FRAME: u32 = 1024;

        let output_path = Path::new("/tmp/test_multiple_streams.mp4");

        let video_encoder = EncoderBuilder::new_video(VIDEO_WIDTH, VIDEO_HEIGHT)
            .with_frame_rate(VIDEO_FPS as i32, 1)
            // 使用标准的90kHz时间基
            .with_time_base(1, 90_000)
            .build()?;

        let audio_encoder =
            Encoder::new_audio(AUDIO_CHANNELS, AUDIO_SAMPLE_RATE, SampleFormat::FLTP).unwrap();

        let stream_writer = StreamWriter::new(output_path)?;
        let mut muxer = Muxer::from_writer(stream_writer);

        let video_time_base = video_encoder.time_base();
        let audio_time_base = audio_encoder.time_base();

        // 添加视频流 和 音频流
        let video_idx = muxer.add_stream(video_encoder)?;
        let audio_idx = muxer.add_stream(audio_encoder)?;

        // 计算总视频帧数
        let total_video_frames = (VIDEO_FPS * VIDEO_DURATION_SEC) as i64;

        // 计算每个视频帧对应的音频样本数，例如：48000Hz / 30fps = 1600个(样本/视频帧)
        let audio_samples_per_video_frame = (AUDIO_SAMPLE_RATE as f64 / VIDEO_FPS as f64) as usize;

        // 音频的PTS，需要根据视频帧数和音频帧数计算
        let mut audio_pts: i64 = 0;

        for frame_idx in 0..total_video_frames {
            // 生成视频帧
            let mut video_frame = generate_video_frame(VIDEO_WIDTH, VIDEO_HEIGHT, frame_idx);

            // 设置视频帧PTS (以编码器 90kHz 为基准)
            let frame_duration = video_time_base.den as i64 / VIDEO_FPS as i64;
            let video_pts = frame_idx * frame_duration;
            video_frame.set_pts(video_pts);
            video_frame.set_time_base(video_time_base);

            println!(
                "Video frame: {}, pts: {}, timebase: {:?}",
                frame_idx, video_pts, video_time_base
            );
            muxer.mux(video_frame, video_idx)?;

            // 视频和音频帧不一对一写入,为什么需要这样计算？
            // 1. 不同的时间基准 ：
            // - 视频以帧率计算（如30fps）
            // - 音频以采样率计算（如48000Hz）
            // 2. 不同的编码要求 ：
            // - AAC音频编码器要求固定的帧大小（通常是1024个样本）
            // - 视频编码器（如H.264）有不同的帧大小要求
            // 3. 同步需求 ：
            // - 为了保持音视频同步，需要确保每个视频帧对应的音频数据都被正确编码
            //
            // 向上取整除法，计算需要生成的音频帧数
            // 在音频处理中，我们需要知道多少个固定大小的帧能容纳所有样本。如果不向上取整，可能会丢失部分音频数据。
            // 例如，对于1600个样本和1024大小的帧：
            // - 1600 / 1024 = 1.56... ≈ 1（向下取整）
            // - 但1个帧只能容纳1024个样本，剩余576个样本会被丢弃
            // - 使用向上取整：(1600 + 1024 - 1) / 1024 = 2，确保所有样本都被处理
            // 这就是为什么在计算音频帧数时使用这个向上取整除法公式的原因
            let audio_frames_needed =
                (audio_samples_per_video_frame as u32).div_ceil(SAMPLES_PER_FRAME);
            for _ in 0..audio_frames_needed {
                // 使用新的音频帧生成函数
                let mut audio_frame = generate_audio_sine_wave_frame(
                    440.0, // 440Hz的音调
                    AUDIO_CHANNELS as usize,
                    SAMPLES_PER_FRAME as usize,
                    AUDIO_SAMPLE_RATE,
                )?;

                audio_frame.set_pts(audio_pts);
                audio_frame.set_time_base(audio_time_base);

                println!(
                    "Audio frame: pts: {}, samples: {}, timebase: {:?}",
                    audio_pts, SAMPLES_PER_FRAME, audio_time_base
                );

                muxer.mux(audio_frame, audio_idx)?;

                // 更新音频PTS (以采样率为基准)
                audio_pts += SAMPLES_PER_FRAME as i64;
            }
        }

        // 完成写入
        muxer.finish().unwrap();

        /////////////////////////////////////////////////////////////////////////////
        /////////////////////////////////////////////////////////////////////////////

        // 解封装验证
        let stream_reader = StreamReader::new(output_path)?;
        let demuxer = Demuxer::from_reader(stream_reader, None, None)?;
        for stream in &demuxer.streams {
            println!("{:?}, {:?}", stream.stream_idx, stream.media_type)
        }

        for res in demuxer {
            match res {
                Ok((index, frame)) => {
                    println!("stream index:{}, {:?}", index, frame)
                }
                Err(e) => {
                    println!("Error decoding frame: {}", e)
                }
            }
        }

        Ok(())
    }

    #[test]
    #[ignore = "mux transcode need a file"]
    fn test_transcode() -> Result<()> {
        transcode("/tmp/bear.mp4", "/tmp/bear_transcode.mov")?;
        Ok(())
    }
}
