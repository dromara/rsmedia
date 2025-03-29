#![allow(dead_code)]

use anyhow::{Context, Result};
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    SampleRate, StreamConfig,
};
use ringbuf::{consumer::Consumer, producer::Producer, traits::Split, HeapRb};
use rubato::{Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType};
use std::sync::{atomic::AtomicBool, atomic::Ordering, Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub enum AudioError {
    DeviceError(String),
    ProcessError(String),
    ConfigError(String),
    ResamplingError(String),
}

/// 音频系统配置
#[derive(Debug, Clone)]
pub struct AudioSystemConfig {
    /// 目标采样率，用于整个音频处理系统
    target_sample_rate: u32,
    /// 支持的采样率范围
    supported_sample_rates: Vec<u32>,
}

impl Default for AudioSystemConfig {
    fn default() -> Self {
        Self {
            target_sample_rate: 44100,
            supported_sample_rates: vec![44100, 48000],
        }
    }
}

#[derive(Debug, Clone)]
pub struct DeviceConfig {
    pub input_gain: f32,
    pub output_gain: f32,
    pub channels: u16,
    pub sample_rate: u32,
}

impl Default for DeviceConfig {
    fn default() -> Self {
        Self {
            input_gain: 1.2,
            output_gain: 0.8,
            channels: 2,
            sample_rate: 48000,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EffectConfig {
    pub echo: Option<EchoParams>,
    pub reverb: Option<ReverbParams>,
}

impl Default for EffectConfig {
    fn default() -> Self {
        Self {
            echo: Some(EchoParams::default()),
            reverb: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BufferConfig {
    pub size: usize,
    pub latency: Duration,
}

impl Default for BufferConfig {
    fn default() -> Self {
        Self {
            size: 1024 * 32,
            latency: Duration::from_millis(20),
        }
    }
}

/// 音频配置参数
#[derive(Debug, Clone)]
pub struct AudioConfig {
    /// 音频设备配置
    pub device: DeviceConfig,
    /// 音频效果配置
    pub effect: EffectConfig,
    /// 缓冲区配置
    pub buffer: BufferConfig,
    /// 音频系统配置
    pub system: AudioSystemConfig,
}

impl AudioConfig {
    pub fn new(device_config: DeviceConfig, system_config: AudioSystemConfig) -> Self {
        Self {
            device: device_config,
            effect: EffectConfig::default(),
            buffer: BufferConfig::default(),
            system: system_config.clone(),
        }
    }

    /// 判断是否需要重采样
    pub fn needs_resampling(&self) -> bool {
        self.device.sample_rate != self.system.target_sample_rate
    }

    /// 获取最佳采样率
    pub fn get_optimal_sample_rate(&self, device: &cpal::Device) -> u32 {
        let supported_configs = device
            .supported_output_configs()
            .unwrap()
            .filter(|config| {
                let range = config.min_sample_rate().0..=config.max_sample_rate().0;
                range.contains(&self.system.target_sample_rate)
            })
            .map(|config| config.min_sample_rate().0)
            .collect::<Vec<_>>();

        if supported_configs.contains(&self.system.target_sample_rate) {
            self.system.target_sample_rate
        } else {
            // 选择最接近的采样率
            supported_configs
                .into_iter()
                .min_by_key(|&rate| (rate as i32 - self.system.target_sample_rate as i32).abs())
                .unwrap_or(self.system.target_sample_rate)
        }
    }
}

/// 混响效果参数
#[derive(Debug, Clone)]
pub struct ReverbParams {
    /// 房间大小 (0.0 - 1.0)
    pub room_size: f32,
    /// 阻尼系数 (0.0 - 1.0)
    pub damping: f32,
    /// 混响宽度 (0.0 - 1.0)
    pub width: f32,
    /// 早期反射强度 (0.0 - 1.0)
    pub early_reflections: f32,
    /// 混响时间 (秒)
    pub reverb_time: f32,
    /// 干湿比 (0.0 - 1.0)
    pub mix: f32,
}

impl Default for ReverbParams {
    fn default() -> Self {
        Self {
            room_size: 0.5,
            damping: 0.5,
            width: 1.0,
            early_reflections: 0.7,
            reverb_time: 1.0,
            mix: 0.3,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AudioFrame {
    pub samples: Vec<f32>,
    pub timestamp: Instant,
    pub channels: u16,
    pub sample_rate: u32,
}

impl AudioFrame {
    pub fn new(samples: Vec<f32>, channels: u16, sample_rate: u32) -> Self {
        Self {
            samples,
            timestamp: Instant::now(),
            channels,
            sample_rate,
        }
    }

    pub fn get_duration(&self) -> Duration {
        Duration::from_secs_f32(
            self.samples.len() as f32 / (self.channels as f32 * self.sample_rate as f32),
        )
    }

    pub fn apply_gain(&mut self, gain: f32) {
        self.samples.iter_mut().for_each(|s| *s *= gain);
    }
}

/// 音频处理器接口
pub trait AudioProcessor: Send {
    fn process(&mut self, frame: &AudioFrame) -> Result<AudioFrame, AudioError>;
    fn reset(&mut self);
    fn update_config(&mut self, config: &AudioConfig);
}

#[derive(Debug, Clone)]
pub struct EchoParams {
    pub delay_ms: f32,
    pub decay: f32,
    pub feedback: f32,
}

impl Default for EchoParams {
    fn default() -> Self {
        Self {
            delay_ms: 300.0,
            decay: 0.5,
            feedback: 0.3,
        }
    }
}

/// 回声处理器
pub struct EchoProcessor {
    buffer: Vec<Vec<f32>>,
    params: EchoParams,
    position: usize,
    sample_rate: u32,
}

impl EchoProcessor {
    pub fn new(params: EchoParams, sample_rate: u32) -> Self {
        let buffer_size =
            ((sample_rate as f32 * params.delay_ms / 1000.0) as usize).next_power_of_two();
        Self {
            buffer: vec![vec![0.0; buffer_size]; 2],
            params,
            position: 0,
            sample_rate,
        }
    }
}

impl AudioProcessor for EchoProcessor {
    fn process(&mut self, frame: &AudioFrame) -> Result<AudioFrame, AudioError> {
        let mut output = vec![0.0; frame.samples.len()];
        let channels = frame.channels as usize;

        for i in (0..frame.samples.len()).step_by(channels) {
            for c in 0..channels {
                let delayed = self.buffer[c][self.position];
                let input = frame.samples[i + c];

                output[i + c] = input + delayed * self.params.decay;
                self.buffer[c][self.position] = input + delayed * self.params.feedback;
            }
            self.position = (self.position + 1) % self.buffer[0].len();
        }

        Ok(AudioFrame::new(output, frame.channels, frame.sample_rate))
    }

    fn reset(&mut self) {
        for buffer in &mut self.buffer {
            buffer.fill(0.0);
        }
        self.position = 0;
    }

    fn update_config(&mut self, config: &AudioConfig) {
        if let Some(echo_params) = &config.effect.echo {
            self.params = echo_params.clone();
        }
    }
}

/// 重采样处理器
pub struct ResamplingProcessor {
    resampler: SincFixedIn<f32>,
    input_sample_rate: u32,
    output_sample_rate: u32,
    channels: usize,
}

impl ResamplingProcessor {
    pub fn new(input_sample_rate: u32, output_sample_rate: u32, channels: u16) -> Result<Self> {
        // 配置重采样参数
        let params = SincInterpolationParameters {
            sinc_len: 256,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 256,
            window: rubato::WindowFunction::BlackmanHarris2,
        };

        // 创建重采样器
        let resampler = SincFixedIn::<f32>::new(
            output_sample_rate as f64 / input_sample_rate as f64,
            1.0,
            params,
            1024,
            channels as usize,
        )
        .context("Failed to create resampler")?;

        Ok(Self {
            resampler,
            input_sample_rate,
            output_sample_rate,
            channels: channels as usize,
        })
    }
}

impl AudioProcessor for ResamplingProcessor {
    fn process(&mut self, frame: &AudioFrame) -> Result<AudioFrame, AudioError> {
        // 将音频数据重组为每个通道一个向量
        let mut input_frames = vec![Vec::new(); self.channels];
        for (i, sample) in frame.samples.iter().enumerate() {
            input_frames[i % self.channels].push(*sample);
        }

        // 执行重采样
        if let Ok(output_frames) = self.resampler.process(&input_frames, None) {
            // 将重采样后的数据重组为交错格式
            let mut output = Vec::with_capacity(output_frames[0].len() * self.channels);
            for i in 0..output_frames[0].len() {
                for c in 0..self.channels {
                    output.push(output_frames[c][i]);
                }
            }

            Ok(AudioFrame::new(
                output,
                frame.channels,
                self.output_sample_rate,
            ))
        } else {
            Ok(frame.clone()) // 重采样失败时返回原始帧
        }
    }

    fn reset(&mut self) {
        self.resampler.reset();
    }

    fn update_config(&mut self, _params: &AudioConfig) {
        // 实现参数更新逻辑
    }
}

/// 音频处理管道
pub struct AudioPipeline {
    config: AudioConfig,
    processors: Vec<Box<dyn AudioProcessor>>,
    resampler: Option<ResamplingProcessor>,
}

impl AudioPipeline {
    pub fn new(config: AudioConfig) -> Result<Self, AudioError> {
        let resampler = if config.device.sample_rate != config.system.target_sample_rate {
            Some(
                ResamplingProcessor::new(
                    config.device.sample_rate,
                    config.system.target_sample_rate,
                    config.device.channels,
                )
                .unwrap(),
            )
        } else {
            None
        };

        Ok(Self {
            config,
            processors: Vec::new(),
            resampler,
        })
    }

    pub fn add_processor(&mut self, processor: Box<dyn AudioProcessor>) {
        self.processors.push(processor);
    }

    pub fn process(&mut self, frame: &AudioFrame) -> Result<AudioFrame, AudioError> {
        let mut current_frame = frame.clone();

        // 应用输入增益
        current_frame.apply_gain(self.config.device.input_gain);

        // 重采样（如果需要）
        if let Some(resampler) = &mut self.resampler {
            current_frame = resampler.process(&current_frame)?;
        }

        // 应用效果处理器
        for processor in &mut self.processors {
            current_frame = processor.process(&current_frame)?;
        }

        // 应用输出增益
        current_frame.apply_gain(self.config.device.output_gain);

        Ok(current_frame)
    }

    pub fn update_config(&mut self, config: AudioConfig) {
        self.config = config.clone();
        for processor in &mut self.processors {
            processor.update_config(&config);
        }
    }
}

/// 音频引擎
pub struct AudioEngine {
    config: AudioConfig,
    pipeline: Arc<Mutex<AudioPipeline>>,
    input_stream: Option<cpal::Stream>,
    output_stream: Option<cpal::Stream>,
    running: Arc<AtomicBool>,
}

impl AudioEngine {
    pub fn new(config: AudioConfig) -> Result<Self> {
        // 创建音频引擎实例
        let mut pipeline = AudioPipeline::new(config.clone())
            .map_err(|e| anyhow::anyhow!("Failed to create audio pipeline: {:?}", e))?;

        // 添加回声效果
        if let Some(echo_params) = config.effect.echo.clone() {
            let echo = EchoProcessor::new(echo_params, config.system.target_sample_rate);
            pipeline.add_processor(Box::new(echo));
        }

        Ok(Self {
            config,
            pipeline: Arc::new(Mutex::new(pipeline)),
            input_stream: None,
            output_stream: None,
            running: Arc::new(AtomicBool::new(false)),
        })
    }

    /// 获取当前音频配置
    pub fn get_config(&self) -> AudioConfig {
        self.pipeline.lock().unwrap().config.clone()
    }

    /// 更新音频配置
    pub fn update_config(&mut self, config: AudioConfig) -> Result<()> {
        let mut pipeline = self.pipeline.lock().unwrap();
        pipeline.update_config(config);
        Ok(())
    }

    /// 添加音频处理器
    pub fn add_processor(&mut self, processor: Box<dyn AudioProcessor>) {
        self.pipeline.lock().unwrap().add_processor(processor);
    }

    pub fn start(&mut self) -> Result<()> {
        let host = cpal::default_host();
        let input_device = host
            .default_input_device()
            .context("input device not found")?;
        let output_device = host
            .default_output_device()
            .context("output device not found")?;

        println!("input device: {}", input_device.name()?);
        println!("output device: {}", output_device.name()?);

        // 创建音频流配置
        let config = StreamConfig {
            channels: self.config.device.channels,
            sample_rate: SampleRate(self.config.device.sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        // 创建环形缓冲区
        let (producer, consumer) = {
            let ring_buf = HeapRb::new(1024 * 32); // 32KB 缓冲区
            let (prod, cons) = ring_buf.split();
            (Arc::new(Mutex::new(prod)), Arc::new(Mutex::new(cons)))
        };

        // 设置输入流
        let producer_clone = producer.clone();
        let running = self.running.clone();
        let input_stream = input_device.build_input_stream(
            &config,
            move |data: &[f32], _: &_| {
                if !running.load(Ordering::Relaxed) {
                    return;
                }

                // 将输入数据写入环形缓冲区
                let mut prod = producer_clone.lock().unwrap();
                let _ = prod.push_slice(data);
            },
            |err| eprintln!("输入错误: {}", err),
            None,
        )?;

        // 设置输出流
        let consumer_clone = consumer.clone();
        let chain = self.pipeline.clone();
        let running = self.running.clone();
        let output_stream = output_device.build_output_stream(
            &config,
            move |data: &mut [f32], _: &_| {
                if !running.load(Ordering::Relaxed) {
                    return;
                }

                // 从环形缓冲区读取数据
                let mut cons = consumer_clone.lock().unwrap();
                let mut chain = chain.lock().unwrap();
                let mut buffer = vec![0.0; data.len()];
                let n_read = cons.pop_slice(&mut buffer);

                if n_read > 0 {
                    // 创建音频帧并进行处理
                    let frame = AudioFrame::new(
                        buffer[..n_read].to_vec(),
                        config.channels,
                        config.sample_rate.0,
                    );
                    let processed = chain.process(&frame).unwrap();

                    // 写入处理后的数据到输出缓冲区
                    data[..n_read].copy_from_slice(&processed.samples);
                    data[n_read..].fill(0.0);
                } else {
                    // 如果没有数据，输出静音
                    data.fill(0.0);
                }
            },
            |err| eprintln!("输出错误: {}", err),
            None,
        )?;

        // 启动流
        input_stream.play()?;
        output_stream.play()?;

        self.running.store(true, Ordering::Relaxed);
        self.input_stream = Some(input_stream);
        self.output_stream = Some(output_stream);

        Ok(())
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        self.input_stream = None;
        self.output_stream = None;
    }
}

fn main() -> Result<()> {
    let config = AudioConfig::new(DeviceConfig::default(), AudioSystemConfig::default());
    let mut engine = AudioEngine::new(config)?;

    engine.start()?;
    println!("Audio engine started, press [Enter] to stop...");

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

    engine.stop();

    Ok(())
}
