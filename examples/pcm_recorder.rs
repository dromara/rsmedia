//! 麦克风流式录音示例 —— `cpal` 采集 + rsmedia [`PcmSink`](rsmedia::pcm::PcmSink) 编码封装。
//!
//! 演示「cpal 桥」的核心用法：cpal 输入回调把交错 PCM 块直接投递给
//! `PcmSink`，由 rsmedia 完成重采样（设备 48kHz → 编码器 44.1kHz 亦可）、
//! AAC 编码与 M4A 封装 —— 内存占用 O(1)，不缓存整段录音
//! （对比 `audio_recorder.rs` 的全量缓存 WAV 方案，长录音不再爆内存）。
//!
//! ```text
//! 麦克风 --cpal 回调(交错 PCM 块)--> mpsc channel --> PcmSink --> AAC/M4A
//! ```
//!
//! 用法：
//! ```text
//! cargo run --example pcm_recorder                    # 录 10s -> tests/output/pcm/cpal_recording.m4a
//! cargo run --example pcm_recorder -- out.m4a 30      # 指定输出文件与最长秒数
//! ```
//!
//! 录音过程中按回车可提前结束；结束后自动用 rodio 回放（symphonia 解码 AAC）。

use anyhow::{Result, bail};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, Stream, StreamConfig};
use rsmedia::mux::Muxer;
use rsmedia::pcm::{PcmSink, PcmSpec};
use rsmedia::{Encoder, Writer};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::time::{Duration, Instant};

/// 一次 cpal 回调投递的交错 PCM 块（变体与 `PcmSink::write_*` 一一对应）。
enum Chunk {
    F32(Vec<f32>),
    I16(Vec<i16>),
    U8(Vec<u8>),
}

/// `&[T]` → `Chunk` 的采样格式封装约定。
trait IntoChunk: Copy {
    fn into_chunk(samples: &[Self]) -> Chunk;
}

impl IntoChunk for f32 {
    fn into_chunk(samples: &[Self]) -> Chunk {
        Chunk::F32(samples.to_vec())
    }
}

impl IntoChunk for i16 {
    fn into_chunk(samples: &[Self]) -> Chunk {
        Chunk::I16(samples.to_vec())
    }
}

impl IntoChunk for u8 {
    fn into_chunk(samples: &[Self]) -> Chunk {
        Chunk::U8(samples.to_vec())
    }
}

fn main() -> Result<()> {
    // ---- 参数 ----
    let mut args = std::env::args().skip(1);
    let output = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(default_output);
    let seconds: u64 = args.next().map(|s| s.parse()).transpose()?.unwrap_or(10);

    if let Some(dir) = output.parent() {
        std::fs::create_dir_all(dir)?;
    }

    // ---- 打开输入设备 ----
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow::anyhow!("未找到默认输入设备（麦克风）"))?;
    let supported = device.default_input_config()?;
    let config: StreamConfig = supported.into();
    let sample_format = supported.sample_format();
    let (rate, channels) = (config.sample_rate, config.channels);
    println!("输入设备: {}", device.description()?);
    println!("设备配置: {rate} Hz × {channels} ch，采样格式 {sample_format:?}");

    // ---- rsmedia 侧：Encoder（默认 AAC）+ Muxer + PcmSink ----
    // 编码器规格可以与设备不同（例：设备 48kHz 单声道 -> 编码器 44.1kHz 立体声），
    // PcmSink 内部的持久重采样器会自动完成格式/采样率/声道数转换。
    let encoder = Encoder::new_audio(channels as i32, rate as i32, rsmedia::SampleFormat::FLTP)?;
    let mut muxer = Muxer::new(&output)?;
    let audio_index = muxer.add_encoder(encoder)?;
    let mut sink = PcmSink::new(muxer, audio_index, PcmSpec::new(rate, channels))?;

    // ---- cpal 侧：音频回调线程 --mpsc--> 主线程（PcmSink 非线程安全，留在主线程）----
    let (chunk_tx, chunk_rx) = mpsc::channel::<Chunk>();
    let stream = match sample_format {
        SampleFormat::F32 => build_input_stream::<f32>(&device, &config, chunk_tx.clone())?,
        SampleFormat::I16 => build_input_stream::<i16>(&device, &config, chunk_tx.clone())?,
        SampleFormat::U8 => build_input_stream::<u8>(&device, &config, chunk_tx.clone())?,
        other => bail!("暂不支持的设备采样格式 {other:?}（仅支持 F32/I16/U8）"),
    };
    stream.play()?;

    // 回车提前结束的监视线程
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    std::thread::spawn(move || {
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).ok();
        let _ = stop_tx.send(());
    });
    println!("开始录音，最长 {seconds}s，按回车提前结束...");

    // ---- 主循环：取块 -> PcmSink（内部完成重采样/凑帧/编码/封装）----
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let mut printed_sec = 0u64;
    loop {
        if Instant::now() >= deadline {
            break;
        }
        match chunk_rx.recv_timeout(Duration::from_millis(200)) {
            Ok(chunk) => {
                write_chunk(&mut sink, chunk)?;
                if stop_rx.try_recv().is_ok() {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if stop_rx.try_recv().is_ok() {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        let sec = sink.input_samples() / rate as u64;
        if sec > printed_sec {
            printed_sec = sec;
            print!("\r已录音 {sec}s / {seconds}s ");
            std::io::stdout().flush()?;
        }
    }

    // 停止采集后冲掉仍在途的块，避免尾部截断
    drop(stream);
    for chunk in chunk_rx.try_iter() {
        write_chunk(&mut sink, chunk)?;
    }

    // ---- 收尾：冲刷重采样器尾样 + 编码器剩余样本 + 写 trailer ----
    // （忘记调用时 Drop 亦可兜底，但显式 finish 能感知错误）
    let recorded_samples = sink.input_samples();
    let _ = sink.finish()?;

    let recorded_secs = recorded_samples as f64 / rate as f64;
    let size = std::fs::metadata(&output)?.len();
    println!(
        "\n完成：{recorded_secs:.2}s（{recorded_samples} samples）-> {}（{size} bytes）",
        output.display()
    );

    // rodio 回放（symphonia 解码 AAC/M4A）
    println!("回放中...");
    playback(&output)?;
    Ok(())
}

/// 按块写入 PcmSink；`write_f32/i16/u8` 的返回值（最后一次 mux 的输出）此处不关心。
fn write_chunk<W: Writer>(sink: &mut PcmSink<W>, chunk: Chunk) -> Result<()> {
    match chunk {
        Chunk::F32(c) => {
            let _ = sink.write_f32(&c)?;
        }
        Chunk::I16(c) => {
            let _ = sink.write_i16(&c)?;
        }
        Chunk::U8(c) => {
            let _ = sink.write_u8(&c)?;
        }
    }
    Ok(())
}

/// 构建指定采样类型的 cpal 输入流；回调里仅做拷贝并入队，不做任何阻塞/编码。
fn build_input_stream<T>(
    device: &Device,
    config: &StreamConfig,
    tx: Sender<Chunk>,
) -> Result<Stream>
where
    T: cpal::SizedSample + IntoChunk,
{
    let err_fn = |err| eprintln!("输入流错误: {err}");
    Ok(device.build_input_stream(
        *config,
        move |data: &[T], _: &_| {
            let _ = tx.send(T::into_chunk(data));
        },
        err_fn,
        None,
    )?)
}

fn default_output() -> PathBuf {
    PathBuf::from("tests/output/pcm/cpal_recording.m4a")
}

fn playback(path: &Path) -> Result<()> {
    let device_sink = rodio::DeviceSinkBuilder::open_default_sink()?;
    let player = rodio::Player::connect_new(device_sink.mixer());
    let file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();
    // 注意：必须用 builder 声明 seekable。`Decoder::new` 默认把源当作不可 seek，
    // 而(symphonia) isomp4 demuxer 无法在非 seek 源上跳过 mdat 寻找尾部的 moov
    // （FFmpeg 默认输出布局 ftyp->mdat->moov），会报
    // "The format of the data has not been recognized."
    let source = rodio::Decoder::builder()
        .with_data(std::io::BufReader::new(file))
        .with_byte_len(len)
        .with_seekable(true)
        .build()?;
    player.append(source);
    player.sleep_until_end();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 复现回放识别路径：PcmSink 生成的 m4a 必须能被 rodio/symphonia 识别。
    /// 无需麦克风 —— 用合成正弦 PCM 代替录音输入。
    #[test]
    fn test_recorded_m4a_recognized_by_rodio() -> Result<()> {
        let output = PathBuf::from("tests/output/pcm/rodio_check.m4a");
        std::fs::create_dir_all(output.parent().unwrap())?;
        let _ = std::fs::remove_file(&output);

        let (rate, channels) = (44_100u32, 2u16);
        let encoder =
            Encoder::new_audio(channels as i32, rate as i32, rsmedia::SampleFormat::FLTP)?;
        let mut muxer = Muxer::new(&output)?;
        let audio_index = muxer.add_encoder(encoder)?;
        let mut sink = PcmSink::new(muxer, audio_index, PcmSpec::new(rate, channels))?;

        // 1 秒 440Hz 正弦，模拟 cpal 回调块粒度
        let total = rate as usize;
        let mut written = 0usize;
        while written < total {
            let n = 1000.min(total - written);
            let mut chunk = Vec::with_capacity(n * channels as usize);
            for i in 0..n {
                let t = (written + i) as f32 / rate as f32;
                let v = (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.3;
                for _ in 0..channels {
                    chunk.push(v);
                }
            }
            write_chunk(&mut sink, Chunk::F32(chunk))?;
            written += n;
        }
        let _ = sink.finish()?;

        let file = std::fs::File::open(&output)?;
        let len = file.metadata()?.len();
        // 与 playback() 一致：声明 seekable，否则 isomp4 无法跳过 mdat 找到 moov
        let decoded = rodio::Decoder::builder()
            .with_data(std::io::BufReader::new(file))
            .with_byte_len(len)
            .with_seekable(true)
            .build();
        match decoded {
            Ok(_) => Ok(()),
            Err(e) => panic!("rodio cannot recognize recorded m4a: {e}"),
        }
    }
}
