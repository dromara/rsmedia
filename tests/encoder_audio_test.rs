#[cfg(test)]
mod tests {
    use anyhow::{Context, Result};
    use dasp::Signal;
    use rsmedia::encode::EncodeRawResult;
    use rsmedia::io::private::{Output, Write};
    use rsmedia::stream::StreamInfo;
    use rsmedia::{utils, EncoderBuilder, SampleFormat, StreamWriterBuilder};
    use rsmpeg::avcodec::AVCodec;
    use rsmpeg::avutil::{AVChannelLayout, AVFrame};
    use rsmpeg::ffi;
    use std::collections::HashMap;
    use std::path::Path;

    /// 音频格式参数结构体 - 支持多种参数选项
    #[allow(dead_code)]
    struct AudioFormatParams {
        /// 支持的帧率列表（VIDEO）
        supported_frame_rates: Option<Vec<ffi::AVRational>>,
        /// 支持的像素格式列表（VIDEO）
        supported_pix_fmts: Vec<ffi::AVPixelFormat>,
        /// 支持的采样率列表（AUDIO）
        pub supported_sample_rates: Vec<i32>,
        /// 支持的采样格式列表（AUDIO）
        pub supported_sample_fmts: Vec<ffi::AVSampleFormat>,
        /// 典型帧大小（单位：样本数/视频帧）
        pub frame_size: usize,
        /// 典型帧大小
        pub bitrate: i64,
        /// 编码器名称
        codec_name: String,
        /// 典型值支持通道数
        channels: usize,
        /// 特定编码器选项
        codec_options: Option<HashMap<String, String>>,
        /// 特定格式选项
        format_options: Option<HashMap<String, String>>,
    }

    /// 获取特定音频格式的详细参数
    fn get_format_parameters(container_type: &str, requested_channels: usize) -> AudioFormatParams {
        // 限制在1-8通道范围内
        let channels = requested_channels.max(1).min(8);

        // 1、通过 container_type 获取特定的编码器和默认比特率
        let (encoder_name, default_bitrate, codec_options, format_options) =
            match container_type.trim().to_lowercase().as_str() {
                // 无损音频容器
                "wav" | "bwf" => {
                    let mut format_opts = HashMap::new();
                    format_opts.insert("write_bext".to_string(), "1".to_string());
                    format_opts.insert("rf64".to_string(), "auto".to_string());

                    ("pcm_s16le", 1_411_200, None, Some(format_opts))
                }

                "aiff" | "aif" => {
                    let mut format_opts = HashMap::new();
                    format_opts.insert("write_id3v2".to_string(), "1".to_string());

                    ("pcm_s16be", 1_411_200, None, Some(format_opts))
                }

                "flac" => ("flac", 0, None, None),

                "alac" => ("alac", 0, None, None),

                "ape" => ("ape", 0, None, None),

                "wv" => ("wavpack", 0, None, None),

                "tta" => ("tta", 0, None, None),

                // 有损音频容器
                "mp3" => ("libmp3lame", 192_000, None, None),

                "aac" | "adts" | "m4a" => ("aac", 128_000, None, None),

                "ogg" => ("libvorbis", 160_000, None, None),

                "wma" => ("wmav2", 128_000, None, None),

                "ac3" => ("ac3", 384_000, None, None),

                "dts" => {
                    let mut codec_opts = HashMap::new();
                    codec_opts.insert("dca_channels".to_string(), "6".to_string());
                    codec_opts.insert("strict".to_string(), "experimental".to_string());

                    ("dca", 768_000, Some(codec_opts), None)
                }

                "mp2" => ("mp2", 192_000, None, None),

                "amr" => ("libopencore_amrnb", 12_200, None, None),

                // 专业音频容器
                "au" | "snd" => ("pcm_mulaw", 64_000, None, None),

                "pcm" | "raw" => ("pcm_s24le", 2_116_800, None, None),

                "rf64" => ("pcm_s16le", 1_411_200, None, None),

                "caf" => ("alac", 0, None, None),

                "aes" => ("pcm_s24le", 2_116_800, None, None),

                "sd2" => ("pcm_s16be", 1_411_200, None, None),

                "mpc" => ("mpc", 192_000, None, None),

                // 流媒体音频容器
                "opus" => ("libopus", 128_000, None, None),

                "mka" => ("libvorbis", 160_000, None, None),

                "ra" => ("real_144", 96_000, None, None),

                "asx" => ("wmav2", 128_000, None, None),

                // 特殊音频格式
                "midi" | "mid" => ("midi", 0, None, None),

                "mod" | "s3m" | "xm" | "it" => ("pcm_s16le", 1_411_200, None, None),

                "sid" => ("pcm_s16le", 1_411_200, None, None),

                "spx" => ("libspeex", 32_000, None, None),

                "gsm" => ("gsm", 13_000, None, None),

                "aax" => ("aac", 64_000, None, None),

                "voc" => ("pcm_u8", 44_100, None, None),

                "maud" => ("pcm_s8", 22_050, None, None),

                // 默认格式
                _ => ("aac", 128_000, None, None),
            };

        // 2、通过编码器获取支持的参数
        let codec = AVCodec::find_encoder_by_name(&utils::from_str(encoder_name))
            .expect(&format!("Failed to find encoder: {}", encoder_name));

        // 获取视频相关参数
        let supported_frame_rates = codec.supported_framerates().map(|rates| rates.to_vec());

        let supported_pix_fmts = codec
            .pix_fmts()
            .unwrap_or(&[])
            .iter()
            .filter(|&&fmt| fmt != ffi::AV_PIX_FMT_NONE)
            .cloned()
            .collect();

        // 获取音频相关参数
        let supported_sample_fmts = codec
            .sample_fmts()
            .unwrap_or(&[])
            .iter()
            .filter(|&&fmt| fmt != ffi::AV_SAMPLE_FMT_NONE)
            .cloned()
            .collect();

        let supported_sample_rates = codec
            .supported_samplerates()
            .unwrap_or(&[44_100, 48_000])
            .to_vec();

        // 根据编码器类型确定帧大小，如果编码器支持可变帧大小，使用默认值
        let frame_size = if codec.capabilities & ffi::AV_CODEC_CAP_VARIABLE_FRAME_SIZE as i32 != 0 {
            1024
        } else {
            match codec.id {
                ffi::AV_CODEC_ID_FLAC => 4608,
                ffi::AV_CODEC_ID_ALAC => 4096,
                ffi::AV_CODEC_ID_WMAV2 => 2048,
                ffi::AV_CODEC_ID_AC3 => 1536,
                ffi::AV_CODEC_ID_MP2 => 1152,
                ffi::AV_CODEC_ID_MP3 => 1152,
                ffi::AV_CODEC_ID_OPUS => 960,
                ffi::AV_CODEC_ID_DTS => 512,
                ffi::AV_CODEC_ID_AMR_NB => 160,
                ffi::AV_CODEC_ID_VORBIS => 64,
                _ => 1024,
            }
        };

        // 根据编码器特性调整通道数
        let adjusted_channels = match codec.id {
            ffi::AV_CODEC_ID_AMR_NB | ffi::AV_CODEC_ID_GSM => 1, // 这些只支持单声道
            ffi::AV_CODEC_ID_MP3 | ffi::AV_CODEC_ID_MP2 | ffi::AV_CODEC_ID_WMAV2 => channels.min(2), // 最多支持双声道
            ffi::AV_CODEC_ID_AC3 => channels.min(6), // 最多支持6声道
            _ => channels,
        };

        let codec_name = codec.name().to_string_lossy().to_string();

        // 3、创建并返回参数结构体
        AudioFormatParams {
            supported_frame_rates,
            supported_pix_fmts,
            supported_sample_rates,
            supported_sample_fmts,
            frame_size,
            bitrate: default_bitrate,
            codec_name,
            channels: adjusted_channels,
            codec_options,
            format_options,
        }
    }

    /// 生成多声道正弦波PCM数据
    ///
    /// # 参数
    /// - `freq_hz`: 基频频率(Hz)，立体声时可传元组 (左声道频率, 右声道频率)
    /// - `sample_rate`: 采样率(如44100)
    /// - `duration_secs`: 持续时间(秒)
    /// - `channels`: 声道数 (1=单声道, 2=立体声)
    /// - `sample_format`: 采样格式 (f32/i16等)
    /// - `amplitude`: 振幅系数 (0.0-1.0)
    #[allow(dead_code)]
    pub fn gen_sine_wave_pcm<
        T: dasp::sample::Sample
            + dasp::sample::FromSample<f32>
            + dasp::sample::ToSample<f32>
            + dasp::sample::FromSample<f64>
            + dasp::sample::ToSample<f64>
            + dasp::sample::FromSample<i64>,
    >(
        freq_hz: &[f32],
        channels: usize,
        sample_rate: u32,
        sample_format: SampleFormat,
        duration_secs: f32,
        amplitude: f32,
        is_planar: bool,
    ) -> Vec<T> {
        // 参数校验
        assert!(
            amplitude > 0.0 && amplitude <= 1.0,
            "振幅必须在(0.0, 1.0]范围内"
        );
        assert!((1..=8).contains(&channels), "声道数必须是1-8");
        assert_eq!(freq_hz.len(), channels, "频率数量必须与声道数匹配");

        let total_samples = (sample_rate as f32 * duration_secs) as usize;

        use dasp::Sample;
        // 生成各声道数据
        let buffers: Vec<Vec<f32>> = freq_hz
            .iter()
            .map(|&hz| {
                dasp::signal::rate(sample_rate as f64)
                    .const_hz(hz as f64)
                    .sine()
                    .scale_amp(amplitude as f64)
                    .take(total_samples)
                    .map(|s| s.to_sample::<f32>())
                    .collect()
            })
            .collect();

        if is_planar {
            // 平面格式处理
            let mut planar = Vec::with_capacity(channels * total_samples);
            for ch in 0..channels {
                planar.extend(convert_samples::<T>(&buffers[ch], sample_format));
            }
            planar
        } else {
            let mut interleaved = Vec::with_capacity(total_samples * channels);
            for i in 0..total_samples {
                for ch in 0..channels {
                    interleaved.push(buffers[ch][i]);
                }
            }
            convert_samples::<T>(&interleaved, sample_format)
        }
    }

    /// 采样格式转换核心
    fn convert_samples<T>(samples: &[f32], format: SampleFormat) -> Vec<T>
    where
        T: dasp::sample::FromSample<f32>
            + dasp::sample::ToSample<f32>
            + dasp::sample::FromSample<f64>
            + dasp::sample::ToSample<f64>
            + dasp::sample::FromSample<i64>,
    {
        use dasp::Sample;
        match format {
            // 无符号8位 (需要0.5偏移)
            SampleFormat::U8 | SampleFormat::U8P => samples
                .iter()
                .map(|&s| ((s * 0.5 + 0.5) * u8::MAX as f32).clamp(0.0, u8::MAX as f32))
                .map(|s| s.to_sample())
                .collect(),

            // 有符号16位
            SampleFormat::S16 | SampleFormat::S16P => samples
                .iter()
                .map(|&s| (s * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32))
                .map(|s| s.to_sample())
                .collect(),

            // 有符号32位 (降低振幅避免溢出)
            SampleFormat::S32 | SampleFormat::S32P => samples
                .iter()
                .map(|&s| (s * i32::MAX as f32 * 0.5).clamp(i32::MIN as f32, i32::MAX as f32))
                .map(|s| s.to_sample())
                .collect(),

            // 32位浮点 (直接转换)
            SampleFormat::FLT | SampleFormat::FLTP => {
                samples.iter().map(|&s| s.to_sample()).collect()
            }

            // 64位浮点 (提升精度)
            SampleFormat::DBL | SampleFormat::DBLP => samples
                .iter()
                .map(|&s| s.to_sample::<f64>())
                .map(|s| s.to_sample::<T>())
                .collect(),

            // 有符号64位 (超高精度整型)
            SampleFormat::S64 | SampleFormat::S64P => samples
                .iter()
                .map(|&s| {
                    (s as f64 * i64::MAX as f64 * 0.5).clamp(i64::MIN as f64, i64::MAX as f64)
                })
                .map(|s| s.to_sample::<i64>())
                .map(|s| s.to_sample::<T>())
                .collect(),

            _ => panic!("Unsupported sample format: {:?}", format),
        }
    }

    /// 生成正弦波音频样本（优化内存访问）
    fn generate_sine_wave(frame: &mut AVFrame, frequency: f64, sample_rate: i32) -> Result<()> {
        let sample_fmt = frame.format;
        let channels = frame.ch_layout.nb_channels as usize;
        let sample_count = frame.nb_samples as usize;
        let is_planar = matches!(
            sample_fmt,
            ffi::AV_SAMPLE_FMT_U8P
                | ffi::AV_SAMPLE_FMT_S16P
                | ffi::AV_SAMPLE_FMT_S32P
                | ffi::AV_SAMPLE_FMT_S64P
                | ffi::AV_SAMPLE_FMT_FLTP
                | ffi::AV_SAMPLE_FMT_DBLP
        );

        // 公共样本生成逻辑
        let generate_samples = |buffer: &mut [f64], channel_offset: usize| {
            for (i, sample) in buffer.iter_mut().enumerate() {
                let t = (i * channels + channel_offset) as f64 / sample_rate as f64;
                *sample = (2.0 * std::f64::consts::PI * frequency * t).sin() * 0.5;
            }
        };

        match sample_fmt {
            ffi::AV_SAMPLE_FMT_U8 | ffi::AV_SAMPLE_FMT_U8P => process_sample_data::<u8>(
                frame,
                is_planar,
                channels,
                sample_count,
                generate_samples,
                |v| (v * 127.5 + 127.5).round() as u8,
            ),

            ffi::AV_SAMPLE_FMT_S16 | ffi::AV_SAMPLE_FMT_S16P => process_sample_data::<i16>(
                frame,
                is_planar,
                channels,
                sample_count,
                generate_samples,
                |v| (v * i16::MAX as f64).round() as i16,
            ),

            ffi::AV_SAMPLE_FMT_S32 | ffi::AV_SAMPLE_FMT_S32P => process_sample_data::<i32>(
                frame,
                is_planar,
                channels,
                sample_count,
                generate_samples,
                |v| (v * i32::MAX as f64).round() as i32,
            ),

            ffi::AV_SAMPLE_FMT_S64 | ffi::AV_SAMPLE_FMT_S64P => process_sample_data::<i64>(
                frame,
                is_planar,
                channels,
                sample_count,
                generate_samples,
                |v| (v * i64::MAX as f64).round() as i64,
            ),

            ffi::AV_SAMPLE_FMT_FLT | ffi::AV_SAMPLE_FMT_FLTP => process_sample_data::<f32>(
                frame,
                is_planar,
                channels,
                sample_count,
                generate_samples,
                |v| v as f32,
            ),

            ffi::AV_SAMPLE_FMT_DBL | ffi::AV_SAMPLE_FMT_DBLP => process_sample_data::<f64>(
                frame,
                is_planar,
                channels,
                sample_count,
                |buf, ch| {
                    for (i, v) in buf.iter_mut().enumerate() {
                        let t = (i * channels + ch) as f64 / sample_rate as f64;
                        *v = (2.0 * std::f64::consts::PI * frequency * t).sin() * 0.5;
                    }
                },
                |v| v,
            ),

            _ => return Err(anyhow::anyhow!("Unsupported format")),
        }

        Ok(())
    }

    /// 通用样本数据处理函数
    fn process_sample_data<T: Copy>(
        frame: &mut AVFrame,
        is_planar: bool,
        channels: usize,
        sample_count: usize,
        generate_samples: impl Fn(&mut [f64], usize),
        scale: impl Fn(f64) -> T,
    ) {
        let frame_ptr = frame.as_mut_ptr();

        if is_planar {
            // 平面格式处理
            for channel in 0..channels {
                let data_ptr = unsafe { (*frame_ptr).data[channel] };
                assert!(!data_ptr.is_null(), "Channel {} data is null", channel);

                let buffer =
                    unsafe { std::slice::from_raw_parts_mut(data_ptr as *mut T, sample_count) };

                let mut float_buffer = vec![0.0; sample_count];
                generate_samples(&mut float_buffer, channel);

                for (i, &v) in float_buffer.iter().enumerate() {
                    buffer[i] = scale(v);
                }
            }
        } else {
            // 打包格式处理
            let buffer = unsafe {
                std::slice::from_raw_parts_mut(
                    (*frame_ptr).data[0] as *mut T,
                    sample_count * channels,
                )
            };

            let mut float_buffer = vec![0.0; sample_count * channels];
            for channel in 0..channels {
                generate_samples(&mut float_buffer[channel..], channel);
            }

            for (i, &v) in float_buffer.iter().enumerate() {
                buffer[i] = scale(v);
            }
        }
    }

    /// 新的sine wave生成函数
    fn test_encode_audio_for_container(container_type: &str) -> Result<()> {
        let output_file = format!("/tmp/test_encode_audio.{}", container_type);
        let output_path = Path::new(output_file.as_str());

        // 获取特定格式参数
        let audio_params = get_format_parameters(container_type, 2);
        let sample_rate = audio_params.supported_sample_rates[0];
        let sample_format = SampleFormat::from(audio_params.supported_sample_fmts[0]);
        let frame_size = audio_params.frame_size;
        let bitrate = audio_params.bitrate;
        let channels = audio_params.channels as u32;

        println!(
            "encode_audio type: {}, sample_rate: {}, sample_format: {:?}",
            container_type, sample_rate, sample_format
        );

        // 创建适合当前格式的编码器
        let mut encoder =
            EncoderBuilder::new_audio(bitrate, channels, sample_rate as u32, sample_format)
                .with_codec_name(Some(audio_params.codec_name))
                .with_options(audio_params.codec_options.map(|opts| opts.into()))
                .build()?;

        let mut stream_writer = StreamWriterBuilder::new(output_path)
            .with_options(audio_params.format_options.map(|opts| opts.into()))
            .build()?;
        let audio_index = stream_writer.add_stream(encoder.codecpar(), encoder.time_base());
        let stream_info = StreamInfo::from_writer(&stream_writer, audio_index)?;

        // 写入文件头
        stream_writer.write_header()?;

        // 音频生成参数
        let duration_secs = 1.0; // 总时长5秒
        let total_samples = (duration_secs as i32 * sample_rate) as i64; // 总采样数

        let mut frame = AVFrame::new();
        frame.set_nb_samples(frame_size as i32);
        frame.set_ch_layout(AVChannelLayout::from_nb_channels(channels as i32).into_inner());
        frame.set_sample_rate(sample_rate);
        frame.set_format(sample_format as _);
        frame
            .alloc_buffer()
            .context("Failed to allocate frame buffer")?;

        for frame_idx in 0..(total_samples / frame_size as i64) {
            generate_sine_wave(&mut frame, 440.0, sample_rate).unwrap();

            // 设置精确时间戳
            frame.set_pts(frame_idx * frame_size as i64);

            match encoder.encode_raw(&frame) {
                EncodeRawResult::Packet(mut packet) => {
                    packet.set_pos(-1);
                    packet.set_stream_index(audio_index as i32);
                    packet.rescale_ts(encoder.time_base(), stream_info.time_base);
                    stream_writer.write_frame(&mut packet)?;
                }
                EncodeRawResult::Drain => {
                    println!("Encoder drained, try send new frame again.");
                    continue;
                }
                EncodeRawResult::Flushed => {
                    println!("Encoder flushed, EOF reached.");
                    break;
                }
                EncodeRawResult::Error(e) => {
                    println!("Encode error: {}", e);
                    break;
                }
            }
        }

        // 处理剩余不足一帧的样本
        let remaining = (total_samples % frame_size as i64) as i32;
        if remaining > 0 {
            generate_sine_wave(&mut frame, 440.0, sample_rate).unwrap();

            // 设置最后帧的时间戳
            frame.set_pts(total_samples - remaining as i64);

            // write last frame
            if let EncodeRawResult::Packet(mut packet) = encoder.encode_raw(&frame) {
                packet.set_pos(-1);
                packet.set_stream_index(audio_index as i32);
                // 将编码器输出的数据包时间戳，从编码器时间基转换到输出流时间基
                // encode_ctx_timebase => out_stream_time_base
                packet.rescale_ts(encoder.time_base(), stream_info.time_base);
                stream_writer.write_frame(&mut packet)?;
            }
        }

        // flush encoder and write trailer
        encoder.flush(
            &mut stream_writer,
            false,
            audio_index,
            stream_info.time_base,
        )?;

        // write trailer
        stream_writer.write_trailer()?;

        Ok(())
    }

    #[test]
    #[rustfmt::skip]
    #[ignore = "ignore encode audio output files"]
    fn test_encode_audio() -> Result<()> {

        let audio_formats = [
            // 无损音频容器
            "wav",   // Waveform Audio，通用无损格式
            "aiff",  // Audio Interchange File Format
            "aif",   // 同上简称
            "flac",  // Free Lossless Audio Codec
            "alac",  // Apple Lossless (通常在m4a中)
            // "ape",   // Monkey's Audio (只支持解码，不支持编码)
            // "wv",    // WavPack (有限编码支持)
            // "tta",   // True Audio (有限支持)

            // 有损音频容器
            "mp3",   // MPEG-1/2 Audio Layer III
            "aac",   // Advanced Audio Coding
            "m4a",   // MPEG-4 Audio (通常用于AAC或ALAC)
            "ogg",   // Ogg Vorbis
            "wma",   // Windows Media Audio
            "ac3",   // Dolby Digital
            "dts",   // Digital Theater Systems
            "mp2",   // MPEG-1 Audio Layer II
            "amr",   // Adaptive Multi-Rate

            // 专业音频容器
            "au",    // Sun/NeXT音频格式
            "snd",   // 同上
            "pcm",   // 脉冲编码调制原始数据
            "rf64",  // 欧洲广播联盟大文件格式
            "caf",   // Core Audio Format (Apple)
            "aes",   // AES/EBU数据
            "bwf",   // Broadcast Wave Format (实际使用wav扩展)
            "sd2",   // Sound Designer II
            // "mpc",   // Musepack (只支持解码)
            "raw",   // 原始音频数据

            // 流媒体音频容器
            "opus",  // Opus Interactive Audio Codec
            "mka",   // Matroska Audio
            "ra",    // RealAudio
            "asx",   // Advanced Stream Redirector

            // 特殊音频格式
            // "midi",  // 乐器数字接口
            // "mid",   // 同上简称
            "mod",   // Module音乐格式
            "s3m",   // ScreamTracker 3 Module
            "xm",    // Extended Module
            "it",    // Impulse Tracker
            "sid",   // Commodore 64声音格式
            // "spx",   // Speex (语音专用)
            // "gsm",   // Global System for Mobile (只支持解码)
            "aax",   // Audible Enhanced Audio
            "voc",   // Creative Voice
            "maud",  // Amiga音频
            "adts",  // AAC传输流
        ];

        let mut err_encoder = Vec::new();
        for format in audio_formats {
            println!("Testing format: {}...", format);

            match test_encode_audio_for_container(format) {
                Ok(_) => println!("Testing format: {} done.", format),
                Err(e) => {
                    println!("Testing format: {} failed: {}", format, e);
                    err_encoder.push(format);
                }
            }
        }

        if !err_encoder.is_empty() {
            eprintln!("Failed encoders: {:#?}", err_encoder)
        }

        Ok(())
    }
}
