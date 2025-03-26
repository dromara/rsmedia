use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rodio::source::{SineWave, Source};
use rodio::{dynamic_mixer, OutputStream, Sink};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

/// 音频处理器结构体
#[derive(Debug)]
#[allow(dead_code)]
struct AudioProcessor {
    sample_rate: u32,
    channels: usize,
    buffer: Arc<Mutex<Vec<f32>>>,
    volume: f32,
}

/// 音频分析结果
#[derive(Debug)]
struct AudioAnalysis {
    max_amplitude: f32,
    rms: f32,
}

impl AudioProcessor {
    fn new(channels: usize, sample_rate: u32) -> Self {
        Self {
            sample_rate,
            channels,
            buffer: Arc::new(Mutex::new(Vec::new())),
            volume: 1.0,
        }
    }

    fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
    }

    fn apply_effects(&self, data: &mut [f32]) {
        for sample in data.iter_mut() {
            *sample *= self.volume;
        }
    }

    fn analyze_audio(&self, data: &[f32]) -> AudioAnalysis {
        let mut max_amplitude = 0.0f32;
        let mut rms = 0.0f32;

        for &sample in data {
            max_amplitude = max_amplitude.max(sample.abs());
            rms += sample * sample;
        }

        rms = (rms / data.len() as f32).sqrt();

        AudioAnalysis { max_amplitude, rms }
    }

    fn process_buffer(&self, buffer: &mut [f32]) {
        self.apply_effects(buffer);
        let analysis = self.analyze_audio(buffer);
        println!(
            "Audio Analysis - Max Amplitude: {:.2}, RMS: {:.2}",
            analysis.max_amplitude, analysis.rms
        );
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 创建音频处理器
    let audio_processor = Arc::new(Mutex::new(AudioProcessor::new(2, 44_100)));

    // 2. Rodio 设置
    let (controller, mixer) = dynamic_mixer::mixer::<f32>(2, 44_100);
    let (_stream, stream_handle) = OutputStream::try_default().unwrap();
    let sink = Sink::try_new(&stream_handle)?;

    // 3. CPAL 设置
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .expect("Failed to get default output device");

    println!("Using audio device: {}", device.name()?);
    let config = device.default_output_config()?;
    println!("Default output config: {:?}", config);

    // 4. 创建音频源
    // Create four unique sources. The frequencies used here correspond
    // notes in the key of C and in octave 4: C4, or middle C on a piano,
    // E4, G4, and A4 respectively.
    let source_c = SineWave::new(261.63)
        .take_duration(Duration::from_secs_f32(5.))
        .amplify(0.20);
    let source_e = SineWave::new(329.63)
        .take_duration(Duration::from_secs_f32(5.))
        .amplify(0.20);
    let source_g = SineWave::new(392.0)
        .take_duration(Duration::from_secs_f32(5.))
        .amplify(0.20);
    let source_a = SineWave::new(440.0)
        .take_duration(Duration::from_secs_f32(5.))
        .amplify(0.20);

    // 5. 设置通信通道
    let (tx, rx) = mpsc::channel::<Vec<f32>>();
    let processor_clone = audio_processor.clone();

    // 6. 启动音频处理线程
    std::thread::spawn(move || {
        while let Ok(mut buffer) = rx.recv() {
            processor_clone.lock().unwrap().process_buffer(&mut buffer);
        }
    });

    // 7. 创建 CPAL 音频流
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => {
            create_stream::<f32>(&device, &config.into(), tx.clone(), audio_processor.clone())
        }
        cpal::SampleFormat::I16 => {
            create_stream::<i16>(&device, &config.into(), tx.clone(), audio_processor.clone())
        }
        cpal::SampleFormat::U16 => {
            create_stream::<u16>(&device, &config.into(), tx.clone(), audio_processor.clone())
        }
        _ => panic!("Unsupported sample format"),
    }?;

    // 8. 添加音源到混音器
    controller.add(source_c);
    controller.add(source_e);
    controller.add(source_g);
    controller.add(source_a);
    sink.append(mixer);

    // 9. 启动音频流和音频处理
    stream.play().unwrap();

    // 10. 演示音量控制
    std::thread::spawn(move || {
        // 等待一下，确保音频开始播放
        std::thread::sleep(Duration::from_secs(1));

        for volume in (0..=10).map(|v| v as f32 / 10.0) {
            std::thread::sleep(Duration::from_millis(1000));
            audio_processor.lock().unwrap().set_volume(volume);
            println!("Setting volume to: {:.1}", volume);
        }
    });

    println!("Playing audio... Press Ctrl+C to stop");

    // 11. 等待播放完成
    sink.sleep_until_end();

    Ok(())
}

fn create_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    tx: mpsc::Sender<Vec<f32>>,
    processor: Arc<Mutex<AudioProcessor>>,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: cpal::Sample + cpal::SizedSample + rodio::Sample,
{
    let channels = config.channels as usize;

    device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            process_audio_data(data, channels, &tx, &processor);
        },
        move |err| {
            eprintln!("Error in audio stream: {:?}", err);
        },
        None,
    )
}

fn process_audio_data<T>(
    data: &mut [T],
    channels: usize,
    tx: &mpsc::Sender<Vec<f32>>,
    processor: &Arc<Mutex<AudioProcessor>>,
) where
    T: cpal::Sample + cpal::SizedSample + rodio::Sample,
{
    let mut audio_buffer = Vec::with_capacity(data.len());

    for frame in data.chunks_mut(channels) {
        for sample in frame.iter_mut() {
            audio_buffer.push(sample.to_f32());
        }
    }

    // 应用音频处理
    if let Ok(proc) = processor.lock() {
        proc.apply_effects(&mut audio_buffer);
    }

    // 发送处理后的音频数据
    tx.send(audio_buffer).unwrap_or_else(|err| {
        eprintln!("Error sending audio data: {:?}", err);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_processor() {
        let processor = AudioProcessor::new(2, 44100);
        let mut test_data = vec![0.5f32; 1024];

        // 测试音量调整
        processor.apply_effects(&mut test_data);
        assert!(test_data.iter().all(|&x| x <= 0.5));

        // 测试音频分析
        let analysis = processor.analyze_audio(&test_data);
        assert!(analysis.max_amplitude <= 0.5);
        assert!(analysis.rms > 0.0);
    }

    #[test]
    fn test_audio_stream() {
        let host = cpal::default_host();
        let device = host.default_output_device().unwrap();
        let config = device.default_output_config().unwrap();
        let (tx, _rx) = mpsc::channel();
        let processor = Arc::new(Mutex::new(AudioProcessor::new(2, 44100)));

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                create_stream::<f32>(&device, &config.into(), tx.clone(), processor)
            }
            cpal::SampleFormat::I16 => {
                create_stream::<i16>(&device, &config.into(), tx.clone(), processor)
            }
            cpal::SampleFormat::U16 => {
                create_stream::<u16>(&device, &config.into(), tx.clone(), processor)
            }
            _ => panic!("不支持的采样格式"),
        };

        assert!(stream.is_ok(), "音频流创建失败");
    }
}
