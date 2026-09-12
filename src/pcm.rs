//! Streaming PCM encode bridge.
//!
//! [`PcmSink`] bridges realtime interleaved PCM sources (e.g. `cpal`/`rodio`
//! recording callbacks, WAV readers) into rsmedia's audio [`Encoder`](crate::encode::Encoder) +
//! [`Muxer`] pipeline:
//!
//! - 任意大小的交错 PCM 块被封装为 packed [`AVFrame`]-（`f32`/`i16`/`u8`），
//!   按块编码，无需调用方对齐编码器 frame_size；
//! - 采样格式/采样率/声道数与编码器不一致时，由编码路径自动重采样转换
//!   （见 [`Encoder`](crate::encode::Encoder) 的 audio `rescale`）；
//! - pts 在编码器时间基（1/编码器采样率）下按样本位置累计，跨块连续；
//! - 编码器内部 FIFO 负责凑满固定帧长（如 AAC 的 1024 样本）；
//! - [`PcmSink::finish`]（或 `Drop`，[`Muxer`] 会自动收尾）冲刷剩余样本并写 trailer。
//!
//! # Example
//!
//! ```no_run
//! use rsmedia::error::Result;
//! fn main() -> Result<()> {
//!     use rsmedia::mux::Muxer;
//!     use rsmedia::pcm::{PcmSink, PcmSpec};
//!     use rsmedia::{Encoder, SampleFormat};
//!     use std::path::Path;
//!
//!     // 1. 按目标规格建编码器（默认 AAC）并加入 Muxer
//!     let encoder = Encoder::new_audio(2, 44_100, SampleFormat::FLTP)?;
//!     let mut muxer = Muxer::new("out.m4a")?;
//!     let audio_index = muxer.add_encoder(encoder)?;
//!
//!     // 2. 用 PcmSink 绑定音频流；spec 描述实时源（如麦克风）的采样率/声道数，
//!     //    与编码器规格不一致时由内部持久重采样器自动转换
//!     let mut sink = PcmSink::new(muxer, audio_index, PcmSpec::new(48_000, 2))?;
//!
//!     // 3. 在 cpal 录音回调里直接投交错 f32 块（O(1) 内存，无需对齐 frame_size）
//!     let mic_chunk = vec![0.0f32; 1024];
//!     sink.write_f32(&mic_chunk)?;
//!
//!     // 4. 冲刷重采样尾样/编码器并写 trailer（Drop 可兜底，显式调用可感知错误）
//!     sink.finish()?;
//!     Ok(())
//! }
//! ```
//!
//! 完整可运行的 cpal 录音示例见 `examples/pcm_recorder.rs`。

use crate::error::{Context, Result, RsmediaError};
use crate::io::Writer;
use crate::mux::Muxer;
use crate::resample::Resampler;
use crate::stream::MediaType;
use crate::time;

use rsmpeg::avutil::{AVChannelLayout, AVFrame};
use rsmpeg::ffi;

/// 输入 PCM 规格（来自麦克风/文件等实时源的交错 PCM）。
///
/// 采样格式由写入方法决定（[`PcmSink::write_f32`]/[`PcmSink::write_i16`]/
/// [`PcmSink::write_u8`]），此处只描述采样率与声道数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PcmSpec {
    /// 输入采样率（Hz），如 cpal 的 `SampleRate(48_000)`。
    pub sample_rate: u32,
    /// 声道数（交错布局），如立体声为 2。
    pub channels: u16,
}

impl PcmSpec {
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        Self {
            sample_rate,
            channels,
        }
    }
}

/// 单次封装进一个 AVFrame 的最大输入样本数（每声道），限制单帧内存占用；
/// 超长输入会被自动切分为多帧。
const MAX_CHUNK_SAMPLES: usize = 4096;

/// Streaming PCM → [`Encoder`](crate::encode::Encoder) → [`Muxer`] 桥接器。
///
/// 持有 [`Muxer`] 的所有权；`write_*` 按 cpal 回调粒度投递交错 PCM，
/// `finish` 冲刷编码器并写 trailer。忘记 `finish` 时 `Drop` 仍会经
/// [`Muxer::finish`] 自动收尾，保证输出文件完整性。
///
/// 内部持有一个**持久** [`Resampler`]（首次写入时按实际输入格式惰性创建）：
/// 输入块先转成编码器的采样格式/采样率/声道数，再按实际输出样本数累计
/// pts 送入编码器。持久化上下文是关键 —— 重采样时 swr 内部滤波延迟会把
/// 样本缓存在调用之间，若每次调用重建上下文（如逐帧临时转换），尾部样本
/// 会随各块延迟丢失。
pub struct PcmSink<W: Writer> {
    muxer: Muxer<W>,
    stream_index: usize,
    /// 输入 PCM 规格。
    spec: PcmSpec,
    /// 编码器采样格式/采样率/声道布局（转换目标）。
    encoder_format: ffi::AVSampleFormat,
    encoder_layout: ffi::AVChannelLayout,
    encoder_sample_rate: i32,
    /// 持久重采样器，按首次写入的输入采样格式惰性创建。
    resampler: Option<(ffi::AVSampleFormat, Resampler)>,
    /// 已写入的输入样本数（每声道）。
    input_samples: u64,
    /// 编码器时间基（1/`encoder_sample_rate`）下的输出样本位置（pts）。
    output_samples: u64,
}

impl<W: Writer> PcmSink<W> {
    /// Wraps an existing [`Muxer`] and binds it to an audio stream.
    ///
    /// # Arguments
    ///
    /// * `muxer` - Muxer that owns the audio [`Encoder`](crate::encode::Encoder)（add_encoder 后传入）。
    /// * `stream_index` - [`Muxer::add_encoder`] 返回的音频流索引。
    /// * `spec` - 输入 PCM 的采样率与声道数（写入方法决定采样格式）。
    pub fn new(muxer: Muxer<W>, stream_index: usize, spec: PcmSpec) -> Result<Self> {
        if spec.sample_rate == 0 || spec.channels == 0 {
            return Err(RsmediaError::invalid_config(format!(
                "invalid PCM spec: sample_rate={}, channels={}",
                spec.sample_rate, spec.channels
            )));
        }
        let (encoder_format, encoder_layout, encoder_sample_rate) = {
            let mux_stream = muxer.get_stream(stream_index)?;
            if mux_stream.media_type != MediaType::AUDIO {
                return Err(RsmediaError::invalid_config(format!(
                    "stream {stream_index} is {:?}, not AUDIO",
                    mux_stream.media_type
                )));
            }
            let encoder = mux_stream.encoder.as_ref().ok_or_else(|| {
                RsmediaError::custom(format!(
                    "stream {stream_index} is a copy stream; PCM playback requires an encoder stream"
                ))
            })?;
            if encoder.sample_rate() <= 0 {
                return Err(RsmediaError::invalid_config(
                    "audio encoder has invalid sample rate",
                ));
            }
            (
                encoder.sample_fmt() as _,
                encoder.ch_layout().clone().into_inner(),
                encoder.sample_rate(),
            )
        };
        Ok(Self {
            muxer,
            stream_index,
            spec,
            encoder_format,
            encoder_layout,
            encoder_sample_rate,
            resampler: None,
            input_samples: 0,
            output_samples: 0,
        })
    }

    /// 写入交错 `f32` PCM 块（如 cpal `SampleFormat::F32` 回调数据）。
    ///
    /// 返回最后一次 mux 的输出（见 [`Muxer::mux`]）。
    pub fn write_f32(&mut self, interleaved: &[f32]) -> Result<Option<W::Out>> {
        self.write_chunks(interleaved, ffi::AV_SAMPLE_FMT_FLT)
    }

    /// 写入交错 `i16` PCM 块（如 cpal `SampleFormat::I16` 回调数据）。
    pub fn write_i16(&mut self, interleaved: &[i16]) -> Result<Option<W::Out>> {
        self.write_chunks(interleaved, ffi::AV_SAMPLE_FMT_S16)
    }

    /// 写入交错 `u8` PCM 块（无符号 8bit，与 `AV_SAMPLE_FMT_U8` 一致）。
    pub fn write_u8(&mut self, interleaved: &[u8]) -> Result<Option<W::Out>> {
        self.write_chunks(interleaved, ffi::AV_SAMPLE_FMT_U8)
    }

    /// 冲刷重采样器尾样、编码器剩余样本并写 trailer，消费 sink。
    ///
    /// 未调用时 `Drop` 会自动执行相同收尾（经 [`Muxer::finish`]），
    /// 但无法感知错误，且 Drop 路径不会冲刷重采样器尾样 —— 显式调用推荐。
    pub fn finish(mut self) -> Result<Option<W::Out>> {
        self.drain_resampler()?;
        self.muxer.finish()
    }

    /// 当前已写入的输入样本数（每声道）。
    pub fn input_samples(&self) -> u64 {
        self.input_samples
    }

    /// 输入 PCM 规格。
    pub fn spec(&self) -> PcmSpec {
        self.spec
    }

    fn write_chunks<T: Copy>(
        &mut self,
        interleaved: &[T],
        sample_format: ffi::AVSampleFormat,
    ) -> Result<Option<W::Out>> {
        let channels = self.spec.channels as usize;
        if !interleaved.len().is_multiple_of(channels) {
            return Err(RsmediaError::invalid_config(format!(
                "interleaved PCM length {} is not a multiple of {} channels",
                interleaved.len(),
                channels
            )));
        }
        let samples_per_channel = interleaved.len() / channels;
        if samples_per_channel == 0 {
            return Ok(None);
        }

        let mut last_out = None;
        for chunk in interleaved.chunks(MAX_CHUNK_SAMPLES * channels) {
            last_out = self.write_chunk(chunk, sample_format)?;
        }
        Ok(last_out)
    }

    fn write_chunk<T: Copy>(
        &mut self,
        interleaved: &[T],
        sample_format: ffi::AVSampleFormat,
    ) -> Result<Option<W::Out>> {
        let nb_samples = (interleaved.len() / self.spec.channels as usize) as i32;

        // 输入帧：packed（交错）样本，全部位于 data[0]
        let mut src = AVFrame::new();
        src.set_format(sample_format);
        src.set_nb_samples(nb_samples);
        src.set_sample_rate(self.spec.sample_rate as i32);
        src.set_ch_layout(
            AVChannelLayout::from_nb_channels(self.spec.channels as i32).into_inner(),
        );
        src.alloc_buffer()
            .context("Failed to allocate PCM input frame buffer")?;
        unsafe {
            let dst = std::slice::from_raw_parts_mut(
                (*src.as_mut_ptr()).data[0] as *mut T,
                interleaved.len(),
            );
            dst.copy_from_slice(interleaved);
        }
        self.input_samples += nb_samples as u64;

        // 转换到编码器规格并按实际输出样本数累计 pts
        let mut dst = self.alloc_encoder_frame(self.encoder_sample_rate)?;
        self.ensure_resampler(sample_format)?
            .convert_frame(&src, &mut dst)?;
        let out_nb = dst.nb_samples;
        if out_nb <= 0 {
            // 重采样器内部缓冲（滤波延迟），随后续输入/flush 输出
            return Ok(None);
        }
        dst.set_pts(self.output_samples as i64);
        self.output_samples += out_nb as u64;
        self.muxer.mux(dst, self.stream_index)
    }

    /// 惰性创建持久重采样器；后续写入必须使用同一输入采样格式。
    fn ensure_resampler(&mut self, sample_format: ffi::AVSampleFormat) -> Result<&mut Resampler> {
        if self.resampler.is_none() {
            let in_layout =
                AVChannelLayout::from_nb_channels(self.spec.channels as i32).into_inner();
            let resampler = Resampler::new(
                in_layout,
                sample_format,
                self.spec.sample_rate as i32,
                self.encoder_layout,
                self.encoder_format,
                self.encoder_sample_rate,
            )?;
            self.resampler = Some((sample_format, resampler));
        }
        let (created_format, resampler) = self.resampler.as_mut().expect("just created");
        if *created_format != sample_format {
            return Err(RsmediaError::invalid_config(format!(
                "input sample format changed mid-stream: started with {}, now {}",
                created_format, sample_format
            )));
        }
        Ok(resampler)
    }

    /// 分配编码器规格的输出帧（容量 `capacity` 样本/声道）。
    fn alloc_encoder_frame(&self, capacity: i32) -> Result<AVFrame> {
        let mut frame = AVFrame::new();
        frame.set_format(self.encoder_format);
        frame.set_nb_samples(capacity.max(1));
        frame.set_sample_rate(self.encoder_sample_rate);
        frame.set_ch_layout(self.encoder_layout);
        frame.set_time_base(time::new_rational(1, self.encoder_sample_rate));
        frame
            .alloc_buffer()
            .context("Failed to allocate encoder-format frame buffer")?;
        Ok(frame)
    }

    /// EOF 时冲刷重采样器内部缓冲的尾样，避免丢失最后几十毫秒。
    fn drain_resampler(&mut self) -> Result<()> {
        // take() 取出以避免同时可变/不可变借用 self；冲刷后不再需要
        let Some((_, mut resampler)) = self.resampler.take() else {
            return Ok(());
        };
        // 上界：按 1 秒容量分配，循环取空（swr 滤波延迟通常仅几十毫秒）
        loop {
            let mut dst = self.alloc_encoder_frame(self.encoder_sample_rate)?;
            resampler.flush(&mut dst)?;
            let out_nb = dst.nb_samples;
            if out_nb <= 0 {
                break;
            }
            dst.set_pts(self.output_samples as i64);
            self.output_samples += out_nb as u64;
            self.muxer.mux(dst, self.stream_index)?;
        }
        Ok(())
    }
}

impl<W: Writer> std::fmt::Debug for PcmSink<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PcmSink")
            .field("stream_index", &self.stream_index)
            .field("spec", &self.spec)
            .field("encoder_sample_rate", &self.encoder_sample_rate)
            .field("input_samples", &self.input_samples)
            .finish()
    }
}

impl<W: Writer> Drop for PcmSink<W> {
    fn drop(&mut self) {
        // Muxer 的 Drop 会自动 flush 编码器 + 写 trailer（见 Muxer::drop），
        // 这里无需额外处理；显式 finish 仍推荐，可感知错误。
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EncoderBuilder;
    use crate::encode::Encoder;
    use crate::io::Reader;
    use crate::mux::Demuxer;
    use crate::{SampleFormat, test_support};

    /// 生成交错 f32 正弦 PCM（amplitude 0.3，单声道/多声道相同相位）。
    fn sine_samples(
        start_sample: u64,
        len_per_channel: usize,
        channels: u16,
        rate: u32,
    ) -> Vec<f32> {
        let mut samples = Vec::with_capacity(len_per_channel * channels as usize);
        for i in 0..len_per_channel {
            let t = (start_sample + i as u64) as f32 / rate as f32;
            let v = (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.3;
            for _ in 0..channels {
                samples.push(v);
            }
        }
        samples
    }

    /// 解码输出文件，返回音频流的总样本数、采样率与声道数。
    fn decode_audio_stream(path: &std::path::Path) -> Result<(usize, u32, u16)> {
        let demuxer = Demuxer::new(path)?;
        let (audio_stream_index, sample_rate, channels) = {
            let s = demuxer
                .streams()
                .iter()
                .find(|s| s.media_type == MediaType::AUDIO)
                .expect("no audio stream in output");
            (
                s.stream_index,
                s.stream_info.sample_rate as u32,
                s.stream_info.channel_layout.nb_channels as u16,
            )
        };

        let mut total_samples = 0usize;
        for res in demuxer {
            let (index, frame) = res?;
            if index == audio_stream_index {
                total_samples += frame.nb_samples as usize;
            }
        }
        Ok((total_samples, sample_rate, channels))
    }

    /// 用 `ffmpeg`-free 的方式校验 codec id：直接读容器流参数。
    fn output_codec_id(path: &std::path::Path) -> ffi::AVCodecID {
        let reader = crate::StreamReader::new(path).expect("reopen output");
        reader.input().streams()[0].codecpar().codec_id
    }

    /// f32 流式写入（含不规则块大小）→ AAC：解码后样本数接近输入、
    /// 采样率/声道数保持、codec 为 aac。
    #[test]
    fn test_pcm_sink_f32_to_aac() -> Result<()> {
        let output_path = test_support::test_output_path("pcm", "test_pcm_f32.m4a");
        test_support::remove_test_output(&output_path);

        let (in_rate, channels) = (44_100u32, 2u16);
        let total_in = 44_100usize; // 1 秒

        let encoder = Encoder::new_audio(channels as i32, in_rate as i32, SampleFormat::FLTP)?;
        let mut muxer = Muxer::new(&output_path)?;
        let audio_index = muxer.add_encoder(encoder)?;
        let mut sink = PcmSink::new(muxer, audio_index, PcmSpec::new(in_rate, channels))?;

        // 不规则块大小：覆盖 FIFO 凑帧逻辑（1024 样本 AAC 帧长 vs 任意回调块）
        let mut written = 0usize;
        for chunk in [1000usize, 123, 7777, 512, 34_688] {
            if written >= total_in {
                break;
            }
            let n = chunk.min(total_in - written);
            sink.write_f32(&sine_samples(written as u64, n, channels, in_rate))?;
            written += n;
        }
        assert_eq!(sink.input_samples() as usize, total_in);
        sink.finish()?;

        assert_eq!(output_codec_id(&output_path), ffi::AV_CODEC_ID_AAC);
        let (samples, out_rate, out_channels) = decode_audio_stream(&output_path)?;
        assert_eq!(out_rate, in_rate);
        assert_eq!(out_channels, channels);
        // AAC 编码有 priming/padding，允许 ±一个 frame_size 的误差
        let encoder_frame = 1024usize;
        assert!(
            (samples as i64 - total_in as i64).abs() <= encoder_frame as i64,
            "decoded {samples} samples, expected ~{total_in}"
        );

        Ok(())
    }

    /// 重采样桥接：输入 48kHz 单声道 f32 → 44.1kHz 立体声编码器，
    /// 解码后应为 44.1kHz、时长接近 1 秒。
    #[test]
    fn test_pcm_sink_resample() -> Result<()> {
        let output_path = test_support::test_output_path("pcm", "test_pcm_resample.m4a");
        test_support::remove_test_output(&output_path);

        let (in_rate, out_rate) = (48_000u32, 44_100u32);
        let (in_channels, out_channels) = (1u16, 2u16);
        let in_total = 48_000usize; // 1 秒

        let encoder = Encoder::new_audio(out_channels as i32, out_rate as i32, SampleFormat::FLTP)?;
        let mut muxer = Muxer::new(&output_path)?;
        let audio_index = muxer.add_encoder(encoder)?;
        let mut sink = PcmSink::new(muxer, audio_index, PcmSpec::new(in_rate, in_channels))?;

        // 模拟 cpal 典型回调块：512 帧/次
        let mut written = 0usize;
        while written < in_total {
            let n = 512usize.min(in_total - written);
            sink.write_f32(&sine_samples(written as u64, n, in_channels, in_rate))?;
            written += n;
        }
        sink.finish()?;

        let (samples, decoded_rate, decoded_channels) = decode_audio_stream(&output_path)?;
        assert_eq!(decoded_rate, out_rate);
        assert_eq!(decoded_channels, out_channels);
        // 重采样 48k→44.1k 后应得到 ~44100 样本（± 1 帧）
        let expected = in_total as f64 * out_rate as f64 / in_rate as f64;
        assert!(
            (samples as f64 - expected).abs() <= 1024.0,
            "decoded {samples} samples, expected ~{expected}"
        );

        Ok(())
    }

    /// i16 写入路径：交错 S16 → AAC，解码样本数与输入一致（±1 帧）。
    #[test]
    fn test_pcm_sink_i16_to_aac() -> Result<()> {
        let output_path = test_support::test_output_path("pcm", "test_pcm_i16.m4a");
        test_support::remove_test_output(&output_path);

        let (in_rate, channels) = (44_100u32, 2u16);
        let total_in = 22_050usize; // 0.5 秒

        let encoder = Encoder::new_audio(channels as i32, in_rate as i32, SampleFormat::FLTP)?;
        let mut muxer = Muxer::new(&output_path)?;
        let audio_index = muxer.add_encoder(encoder)?;
        let mut sink = PcmSink::new(muxer, audio_index, PcmSpec::new(in_rate, channels))?;

        let mut written = 0usize;
        while written < total_in {
            let n = 2048usize.min(total_in - written);
            let mut chunk = Vec::with_capacity(n * channels as usize);
            for i in 0..n {
                let t = (written + i) as f32 / in_rate as f32;
                let v = (2.0 * std::f32::consts::PI * 440.0 * t).sin();
                for _ in 0..channels {
                    chunk.push((v * i16::MAX as f32 * 0.3) as i16);
                }
            }
            sink.write_i16(&chunk)?;
            written += n;
        }
        sink.finish()?;

        let (samples, out_rate, out_channels) = decode_audio_stream(&output_path)?;
        assert_eq!(out_rate, in_rate);
        assert_eq!(out_channels, channels);
        assert!(
            (samples as i64 - total_in as i64).abs() <= 1024,
            "decoded {samples} samples, expected ~{total_in}"
        );

        Ok(())
    }

    /// u8 写入路径：交错 U8（128 为静音中点）→ AAC，解码样本数与输入一致（±1 帧）。
    #[test]
    fn test_pcm_sink_u8_to_aac() -> Result<()> {
        let output_path = test_support::test_output_path("pcm", "test_pcm_u8.m4a");
        test_support::remove_test_output(&output_path);

        let (in_rate, channels) = (44_100u32, 1u16);
        let total_in = 22_050usize; // 0.5 秒

        let encoder = Encoder::new_audio(channels as i32, in_rate as i32, SampleFormat::FLTP)?;
        let mut muxer = Muxer::new(&output_path)?;
        let audio_index = muxer.add_encoder(encoder)?;
        let mut sink = PcmSink::new(muxer, audio_index, PcmSpec::new(in_rate, channels))?;

        let mut written = 0usize;
        while written < total_in {
            let n = 960usize.min(total_in - written);
            let chunk: Vec<u8> = (0..n)
                .map(|i| {
                    let t = (written + i) as f32 / in_rate as f32;
                    (128.0 + (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 40.0) as u8
                })
                .collect();
            sink.write_u8(&chunk)?;
            written += n;
        }
        sink.finish()?;

        let (samples, out_rate, out_channels) = decode_audio_stream(&output_path)?;
        assert_eq!(out_rate, in_rate);
        assert_eq!(out_channels, channels);
        assert!(
            (samples as i64 - total_in as i64).abs() <= 1024,
            "decoded {samples} samples, expected ~{total_in}"
        );

        Ok(())
    }

    /// 音频滤镜通用输入格式声明：编码器 FLTP，滤镜链声明输入 FLT（packed），
    /// 链内 `aformat` 转回 FLTP。进图前 swr 预转换 + 图内转换 + 滤镜后 rescale
    /// 三段协同，解码样本数与输入一致（±1 帧）。
    #[test]
    fn test_pcm_sink_filter_input_sample_format() -> Result<()> {
        let output_path = test_support::test_output_path("pcm", "test_pcm_filter_input.m4a");
        test_support::remove_test_output(&output_path);

        let (in_rate, channels) = (44_100u32, 2u16);
        let total_in = 22_050usize; // 0.5 秒

        let filter = crate::filter::Filter::new(
            "aformat",
            MediaType::AUDIO,
            "aformat=sample_fmts=fltp".to_string(),
        )
        .with_input_format(SampleFormat::FLT);
        let encoder =
            EncoderBuilder::new_audio(128_000, channels as i32, in_rate as i32, SampleFormat::FLTP)
                .with_filters(vec![filter])
                .build()?;
        let mut muxer = Muxer::new(&output_path)?;
        let audio_index = muxer.add_encoder(encoder)?;
        let mut sink = PcmSink::new(muxer, audio_index, PcmSpec::new(in_rate, channels))?;

        let mut written = 0usize;
        while written < total_in {
            let n = 1000usize.min(total_in - written);
            sink.write_f32(&sine_samples(written as u64, n, channels, in_rate))?;
            written += n;
        }
        sink.finish()?;

        let (samples, out_rate, out_channels) = decode_audio_stream(&output_path)?;
        assert_eq!(out_rate, in_rate);
        assert_eq!(out_channels, channels);
        assert!(
            (samples as i64 - total_in as i64).abs() <= 1024,
            "decoded {samples} samples, expected ~{total_in}"
        );

        Ok(())
    }

    /// 参数与状态校验：非音频流拒绝、非法规格拒绝、声道不对齐的块拒绝。
    #[test]
    fn test_pcm_sink_validation() -> Result<()> {
        // 非法规格：new() 出错时 muxer 被丢弃（未写 header，无副作用）
        let assert_invalid_spec = |spec: PcmSpec| -> Result<()> {
            let output_path = test_support::test_output_path("pcm", "test_pcm_invalid.m4a");
            test_support::remove_test_output(&output_path);
            let encoder = Encoder::new_audio(2, 44_100, SampleFormat::FLTP)?;
            let mut muxer = Muxer::new(&output_path)?;
            let audio_index = muxer.add_encoder(encoder)?;
            assert!(PcmSink::new(muxer, audio_index, spec).is_err());
            Ok(())
        };
        assert_invalid_spec(PcmSpec::new(0, 2))?;
        assert_invalid_spec(PcmSpec::new(44_100, 0))?;

        // 非音频流拒绝
        let output_path = test_support::test_output_path("pcm", "test_pcm_invalid.m4a");
        test_support::remove_test_output(&output_path);
        let video_encoder = EncoderBuilder::new_video(64, 64).build()?;
        let mut muxer = Muxer::new(&output_path)?;
        let video_index = muxer.add_encoder(video_encoder)?;
        assert!(PcmSink::new(muxer, video_index, PcmSpec::new(44_100, 2)).is_err());

        // 声道不对齐的交错块拒绝；空块为 no-op
        let output_path = test_support::test_output_path("pcm", "test_pcm_invalid.m4a");
        let encoder = Encoder::new_audio(2, 44_100, SampleFormat::FLTP)?;
        let mut muxer = Muxer::new(&output_path)?;
        let audio_index = muxer.add_encoder(encoder)?;
        let mut sink = PcmSink::new(muxer, audio_index, PcmSpec::new(44_100, 2))?;
        assert!(sink.write_f32(&[0.0f32; 3]).is_err());
        assert!(sink.write_f32(&[]).is_ok());
        Ok(())
    }
}
