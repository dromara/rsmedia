use anyhow::Result;
use chrono::Local;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, Stream, StreamConfig};
use rodio::{Decoder, DeviceSinkBuilder};
use std::fs::File;
use std::io::{BufReader, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use wavers::Wav;

/// 录音器结构体
pub struct AudioRecorder {
    device: Device,
    config: StreamConfig,
    stream: Option<Stream>,
    running: Arc<AtomicBool>,
    recording_buffer: Arc<Mutex<Vec<f32>>>,
    start_time: Arc<Mutex<Option<Instant>>>,
}

impl AudioRecorder {
    /// 创建新的录音器实例
    pub fn new() -> Result<Self> {
        let host = cpal::default_host();

        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow::anyhow!("未找到输入设备"))?;

        println!("使用输入设备: {}", device.description()?);

        let config: StreamConfig = device.default_input_config()?.into();

        println!("采样率: {} Hz", config.sample_rate);
        println!("通道数: {}", config.channels);

        Ok(Self {
            device,
            config,
            stream: None,
            recording_buffer: Arc::new(Mutex::new(Vec::new())),
            running: Arc::new(AtomicBool::new(false)),
            start_time: Arc::new(Mutex::new(None)),
        })
    }

    /// 开始录音
    pub fn start_recording(&mut self) -> Result<()> {
        let recording_buffer = Arc::clone(&self.recording_buffer);
        let running = Arc::clone(&self.running);
        let start_time = Arc::clone(&self.start_time);

        self.running.store(true, Ordering::SeqCst);
        *start_time.lock().unwrap() = Some(Instant::now());

        let stream = match self.device.default_input_config()?.sample_format() {
            SampleFormat::F32 => self.build_stream::<f32>(&recording_buffer, &running)?,
            SampleFormat::I16 => self.build_stream::<i16>(&recording_buffer, &running)?,
            SampleFormat::U16 => self.build_stream::<u16>(&recording_buffer, &running)?,
            _ => return Err(anyhow::anyhow!("不支持的采样格式")),
        };

        stream.play()?;
        self.stream = Some(stream);

        Ok(())
    }

    /// 停止录音
    pub fn stop_recording(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        self.stream = None;
    }

    /// 构建音频流
    fn build_stream<T>(
        &self,
        recording_buffer: &Arc<Mutex<Vec<f32>>>,
        running: &Arc<AtomicBool>,
    ) -> Result<Stream>
    where
        T: cpal::Sample<Float = f32> + cpal::SizedSample,
    {
        let err_fn = |err| eprintln!("输入音频流发生错误: {}", err);

        let recording_buffer = Arc::clone(recording_buffer);
        let running = Arc::clone(running);

        let stream = self.device.build_input_stream(
            &self.config,
            move |data: &[T], _: &_| {
                if running.load(Ordering::SeqCst) {
                    let mut buffer = recording_buffer.lock().unwrap();
                    // 使用 Sample trait 的方法转换为 f32
                    for &sample in data.iter() {
                        buffer.push(sample.to_float_sample());
                    }
                }
            },
            err_fn,
            None,
        )?;

        Ok(stream)
    }

    /// 获取录音时长（秒）
    pub fn get_duration(&self) -> f32 {
        if let Some(start_time) = *self.start_time.lock().unwrap() {
            start_time.elapsed().as_secs_f32()
        } else {
            0.0
        }
    }

    /// 是否正在录音
    pub fn is_recording(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// 保存录音到WAV文件
    pub fn save_to_wav(&self, filename: &str) -> Result<()> {
        let samples = self.recording_buffer.lock().unwrap();
        let sample_rate = self.config.sample_rate as i32;
        let n_channels = self.config.channels;

        // 使用 wavers 的 write 函数保存 WAV 文件
        wavers::write(Path::new(filename), &samples, sample_rate, n_channels)?;

        Ok(())
    }

    /// 获取当前音量级别 (RMS值)
    /// dB是标准的音频测量单位
    /// 常见的dB值范围：
    /// 0 dB: 参考值
    /// -60 dB: 很安静
    /// -40 dB: 低音量
    /// -20 dB: 中等音量
    /// 0 dB: 满量程（最大无失真音量）
    pub fn get_volume_level(&self) -> f32 {
        let buffer = self.recording_buffer.lock().unwrap();
        if buffer.is_empty() {
            return 0.0;
        }

        // 取最后1024个采样计算RMS值
        let window_size = 1024.min(buffer.len());
        let samples = &buffer[buffer.len() - window_size..];

        // 计算RMS值
        let (sum_squares, peak) = samples.iter().fold((0.0f32, 0.0f32), |(sum, peak), &x| {
            let x_abs = x.abs();
            (sum + x * x, peak.max(x_abs))
        });

        let rms = (sum_squares / window_size as f32).sqrt();

        // 混合RMS和峰值以获得更好的响应
        let mixed = 0.8 * rms + 0.2 * peak;

        // 将RMS值转换为分贝 (dB)
        let db = 20.0 * mixed.log10();

        // 将分贝值归一化到0-1范围
        // 假设-60dB是最小可听值，0dB是最大值
        let normalized = (db + 60.0) / 60.0;
        normalized.max(0.0).min(1.0)
    }
}

/// 录音控制器
pub struct RecordingController {
    recorder: AudioRecorder,
}

impl RecordingController {
    pub fn new() -> Result<Self> {
        Ok(Self {
            recorder: AudioRecorder::new()?,
        })
    }

    /// 开始录音，等待手动停止
    pub fn start_recording(&mut self) -> Result<()> {
        println!("开始录音... (按回车键停止)");
        self.recorder.start_recording()?;

        // 创建一个通道来通知录音停止
        let (tx, rx) = std::sync::mpsc::channel();

        // 在单独的线程中处理用户输入
        std::thread::spawn(move || {
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).ok();
            tx.send(()).ok();
        });

        // 主循环更新显示
        while self.recorder.is_recording() {
            let volume = self.recorder.get_volume_level();
            let volume_bar = Self::create_volume_bar(volume);

            // dB = 20 * log10 (振幅比)
            // 将RMS值转换为dB
            print!(
                "\r录音时长: {:.1}s | 音量: {} {:.2} dB",
                self.recorder.get_duration(),
                volume_bar,
                20.0 * volume.log10()
            );

            std::io::stdout().flush()?;

            // 非阻塞地检查是否收到停止信号
            if rx.try_recv().is_ok() {
                break;
            }

            // 短暂休眠以减少 CPU 使用
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        self.recorder.stop_recording();

        let filename = format!("recording_{}.wav", Local::now().format("%Y%m%d_%H%M%S"));

        println!("\n正在保存录音...");
        self.recorder.save_to_wav(&filename)?;
        println!("录音已保存到: {}", filename);

        // 显示录音文件信息
        let wav = Wav::<f32>::from_path(&filename)?;
        println!("时长: {:.2} 秒", wav.duration());

        Ok(())
    }

    /// 创建音量条
    fn create_volume_bar(volume: f32) -> String {
        let bar_length = (volume * 50.0) as usize;
        let bar: String = std::iter::repeat('█')
            .take(bar_length)
            .chain(std::iter::repeat('░').take(50 - bar_length))
            .collect();
        bar
    }

    /// 播放录音
    pub fn play_recording(&self, filename: &str) -> Result<()> {
        // 首先读取并显示 WAV 文件信息
        let wav = Wav::<f32>::from_path(filename)?;
        println!("正在播放录音文件:");
        println!("采样率: {} Hz", wav.sample_rate());
        println!("通道数: {}", wav.n_channels());
        println!("时长: {:.2} 秒", wav.duration());

        // 使用 rodio 播放音频
        let stream_handle = DeviceSinkBuilder::open_default_sink().unwrap();
        let player = rodio::Player::connect_new(stream_handle.mixer());

        let file = File::open(filename)?;
        let source = Decoder::new(BufReader::new(file))?;
        player.append(source);

        println!("播放中... (按 Ctrl+C 停止)");
        player.sleep_until_end();
        Ok(())
    }
}

fn main() -> Result<()> {
    let mut controller = RecordingController::new()?;

    // 开始录音
    controller.start_recording()?;

    // 查找并播放最新录制的音频
    if let Ok(entries) = std::fs::read_dir(".") {
        if let Some(latest_recording) = entries
            .filter_map(|entry| entry.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext == "wav")
                    .unwrap_or(false)
            })
            .max_by_key(|e| e.metadata().unwrap().modified().unwrap())
        {
            println!("\n播放录音...");
            controller.play_recording(latest_recording.path().to_str().unwrap())?;
        }
    }

    Ok(())
}
