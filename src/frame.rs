use crate::pixel::PixelFormat;
use crate::{imgutils, time, MediaType, SampleFormat};

use rsmpeg::avutil::{AVChannelLayout, AVFrame};
use rsmpeg::ffi;

use anyhow::{Context, Error, Result};
use yuv::{
    BufferStoreMut, YuvConversionMode, YuvPlanarImage, YuvPlanarImageMut, YuvRange,
    YuvStandardMatrix,
};

pub trait MediaFrameType:
    'static
    + Clone
    + Copy
    + Send
    + Sync
    + Default
    + PartialOrd
    + num_traits::Zero
    + num_traits::NumCast
    + num_traits::NumAssign
{
}

impl MediaFrameType for i8 {}
impl MediaFrameType for u8 {}
impl MediaFrameType for i16 {}
impl MediaFrameType for u16 {}
impl MediaFrameType for i32 {}
impl MediaFrameType for u32 {}
impl MediaFrameType for i64 {}
impl MediaFrameType for u64 {}
impl MediaFrameType for f32 {}
impl MediaFrameType for f64 {}

/// A frame array is the `ndarray` version of `AVFrame`
/// It is 3-dimensional array with dims `(H, W, C)` and type byte.
///
/// # Parameters
///
/// * `T` - The underlying data type for samples/pixels:
///   * For video: typically `u8`
///   * For audio: `i16`, `i32`, `f32`, or `f64`
///
/// # Safety
///
/// The type parameter `T` must match the frame's format:
/// * For RGB24: `u8`
/// * For FLTP: `f32`
#[derive(Debug, Clone)]
pub struct MediaFrame<T> {
    pub pts: i64,
    pub dts: i64,
    pub duration: i64,
    /// Video: [`PixelFormat`]
    /// Audio: [`SampleFormat`]
    pub format: i32,
    /// Video: `[height, width, channels]`
    /// Audio: `[frames, nb_samples, nb_channels]`
    pub data: ndarray::Array3<T>,
    /// Video: `1 / frame_rate`
    /// Audio: `1 / sample_rate`
    pub time_base: ffi::AVRational,
    /// only for Video / Audio: [`MediaType`]
    pub media_type: MediaType,
    // Video
    pub width: usize,
    pub height: usize,
    pub pict_type: ffi::AVPictureType,
    // Audio
    pub sample_rate: u32,
    pub nb_samples: u32,
    pub nb_channels: u32,
}

impl<T> MediaFrame<T>
where
    T: MediaFrameType,
{
    /// 创建视频帧
    pub fn new_video(
        width: usize,
        height: usize,
        format: PixelFormat,
        time_base: ffi::AVRational,
        data: ndarray::Array3<T>,
    ) -> Result<Self> {
        let (h, w, c) = data.dim();
        if h != height || w != width || c != 3 {
            return Err(Error::msg(format!(
                "Invalid dimensions: expected ({}, {}, 3), got ({}, {}, {})",
                height, width, h, w, c
            )));
        }

        Ok(Self {
            width,
            height,
            time_base,
            data,
            format: format.into(),
            pts: 0,
            dts: 0,
            duration: 0,
            media_type: MediaType::VIDEO,
            pict_type: ffi::AV_PICTURE_TYPE_NONE,
            sample_rate: 0,
            nb_samples: 0,
            nb_channels: 0,
        })
    }

    pub fn new_video_frame(
        width: usize,
        height: usize,
        format: PixelFormat,
        time_base: ffi::AVRational,
    ) -> Result<Self>
    where
        T: num_traits::Zero,
    {
        let data = ndarray::Array3::<T>::zeros((height, width, 3));
        Self::new_video(width, height, format, time_base, data)
    }

    /// 创建音频帧
    pub fn new_audio(
        format: SampleFormat,
        nb_channels: u32,
        nb_samples: u32,
        sample_rate: u32,
        time_base: ffi::AVRational,
        data: ndarray::Array3<T>,
    ) -> Result<Self> {
        let (frames, samples, ch) = data.dim();
        if frames != 1 || samples != nb_samples as usize || ch != nb_channels as usize {
            return Err(Error::msg(format!(
                "Invalid dimensions: expected (1, {}, {}), got ({}, {}, {})",
                nb_samples, nb_channels, frames, samples, ch
            )));
        }

        Ok(Self {
            format: format as _,
            data,
            time_base,
            sample_rate,
            nb_samples,
            nb_channels,
            pts: 0,
            dts: 0,
            duration: 0,
            width: 0,
            height: 0,
            media_type: MediaType::AUDIO,
            pict_type: ffi::AV_PICTURE_TYPE_NONE,
        })
    }

    pub fn new_audio_frame(
        format: SampleFormat,
        nb_channels: u32,
        nb_samples: u32,
        sample_rate: u32,
        time_base: ffi::AVRational,
    ) -> Result<Self>
    where
        T: num_traits::Zero,
    {
        let data = ndarray::Array3::<T>::zeros((1, nb_samples as usize, nb_channels as usize));
        Self::new_audio(
            format,
            nb_channels,
            nb_samples,
            sample_rate,
            time_base,
            data,
        )
    }

    pub fn set_pts(&mut self, pts: i64) {
        self.pts = pts;
    }

    pub fn set_dts(&mut self, dts: i64) {
        self.dts = dts;
    }

    pub fn set_time_base(&mut self, time_base: ffi::AVRational) {
        self.time_base = time_base;
    }

    pub fn set_sample_rate(&mut self, sample_rate: u32) {
        self.sample_rate = sample_rate;
    }

    pub fn from_avframe(frame: &AVFrame) -> Result<Self> {
        if frame.data[0].is_null() {
            return Err(Error::msg("Invalid frame data"));
        }

        let (width, height) = (frame.width as usize, frame.height as usize);
        let pts = frame.pts;
        let dts = frame.pkt_dts;
        let format = frame.format;
        let duration = frame.duration;
        let time_base = frame.time_base;

        if width == 0 && height == 0 && frame.nb_samples > 0 {
            // Audio frame
            Ok(Self {
                pts,
                dts,
                format,
                duration,
                width: 0,
                height: 0,
                time_base,
                data: audio_data(frame)?,
                media_type: MediaType::AUDIO,
                pict_type: ffi::AV_PICTURE_TYPE_NONE,
                sample_rate: frame.sample_rate as u32,
                nb_samples: frame.nb_samples as u32,
                nb_channels: frame.ch_layout.nb_channels as u32,
            })
        } else if width > 0 && height > 0 {
            // Video frame
            Ok(Self {
                width,
                height,
                pts,
                dts,
                format,
                duration,
                time_base,
                data: video_data(frame)?,
                media_type: MediaType::VIDEO,
                pict_type: frame.pict_type,
                sample_rate: 0,
                nb_samples: 0,
                nb_channels: 0,
            })
        } else {
            Err(Error::msg("Unsupported frame format"))
        }
    }

    /// 转换为新AVFrame
    pub fn to_avframe(&self) -> Result<AVFrame> {
        let mut frame = AVFrame::new();
        let mut time_base = self.time_base;
        if self.media_type == MediaType::VIDEO {
            // video frame
            frame.set_width(self.width as i32);
            frame.set_height(self.height as i32);
            frame.set_format(self.format);
            frame.set_pict_type(self.pict_type);
            fill_video_data(&mut frame, &self.data)?;
        } else {
            // audio frame
            frame.set_format(self.format);
            frame.set_nb_samples(self.nb_samples as i32);
            frame.set_sample_rate(self.sample_rate as i32);
            time_base = time::new_rational(1, self.sample_rate as i32);
            frame.set_ch_layout(
                AVChannelLayout::from_nb_channels(self.nb_channels as i32).into_inner(),
            );
            fill_audio_data(&mut frame, &self.data)?;
        };

        frame.set_pts(self.pts);
        // frame.set_dts(self.dts);
        // frame.set_duration(self.duration);
        frame.set_time_base(time_base);
        Ok(frame)
    }

    ////////////////////////////////////////////////////////////////////////////////////
    ///////////////////////////// convert //////////////////////////////////////////////
    ////////////////////////////////////////////////////////////////////////////////////

    pub fn convert_rgb_to_yuv(&self) -> Result<Self> {
        if self.media_type != MediaType::VIDEO {
            return Err(Error::msg("Only video frames can be color space converted"));
        }

        if self.format != ffi::AV_PIX_FMT_RGB24 {
            return Err(Error::msg("Only RGB24 format video frames are supported"));
        }

        let height = self.height;
        let width = self.width;

        // 1. RGB数据准备，保持完整的色彩范围
        let mut rgb_bytes = Vec::with_capacity(width * height * 3);
        if let Some(slice) = self.data.as_standard_layout().as_slice() {
            // 数据连续时，可直接转换
            rgb_bytes = slice
                .iter()
                .map(|&val| num_traits::cast::<T, u8>(val).unwrap_or(0))
                .collect();
        } else {
            // 数据不连续时需逐元素访问
            for h in 0..height {
                for w in 0..width {
                    let r: u8 = num_traits::cast(self.data[[h, w, 0]]).unwrap_or(0);
                    let g: u8 = num_traits::cast(self.data[[h, w, 1]]).unwrap_or(0);
                    let b: u8 = num_traits::cast(self.data[[h, w, 2]]).unwrap_or(0);
                    rgb_bytes.extend_from_slice(&[r, g, b]);
                }
            }
        }

        // 2. 创建YUV平面
        let y_stride = width;
        let uv_stride = width / 2;
        let mut y_plane = vec![0u8; y_stride * height];
        let mut u_plane = vec![0u8; uv_stride * (height / 2)];
        let mut v_plane = vec![0u8; uv_stride * (height / 2)];

        // 3. 创建YUV图像
        let mut planar_image = YuvPlanarImageMut {
            y_plane: BufferStoreMut::Borrowed(&mut y_plane),
            y_stride: y_stride as u32,
            u_plane: BufferStoreMut::Borrowed(&mut u_plane),
            u_stride: uv_stride as u32,
            v_plane: BufferStoreMut::Borrowed(&mut v_plane),
            v_stride: uv_stride as u32,
            width: width as u32,
            height: height as u32,
        };

        // 4. 选择转换参数
        let colorspace = {
            if height < 720 {
                // SD color space
                YuvStandardMatrix::Bt601
            } else if height < 1080 {
                // HD color space
                YuvStandardMatrix::Bt709
            } else {
                // UHD color space
                YuvStandardMatrix::Bt2020
            }
        };

        // 5. 使用Full Range进行转换
        yuv::rgb_to_yuv420(
            &mut planar_image,
            &rgb_bytes,
            (width * 3) as u32,
            YuvRange::Full,
            colorspace,
            YuvConversionMode::Professional,
        )
        .map_err(|e| Error::msg(format!("convert rgb24 to yuv420p error:{}", e)))?;

        // 6. 构建YUV数据
        let mut yuv_data = ndarray::Array3::<T>::zeros((height, width, 3));

        // 复制Y平面
        for h in 0..height {
            let y_offset = h * y_stride;
            for w in 0..width {
                yuv_data[[h, w, 0]] = num_traits::cast(y_plane[y_offset + w]).unwrap_or(T::zero());
            }
        }

        // 复制UV平面
        for h_uv in 0..height / 2 {
            let u_offset = h_uv * uv_stride;
            // 对应的Y平面高度起始位置
            let h_y = h_uv * 2;

            for w_uv in 0..width / 2 {
                let u_val = u_plane[u_offset + w_uv];
                let v_val = v_plane[u_offset + w_uv];
                // 对应的Y平面宽度起始位置
                let w_y = w_uv * 2;

                // 为2x2块中的每个像素设置相同的UV值
                // 优化: 先计算边界条件，避免内层循环中的重复检查
                let max_h = (h_y + 2).min(height);
                let max_w = (w_y + 2).min(width);

                for h_pos in h_y..max_h {
                    for w_pos in w_y..max_w {
                        yuv_data[[h_pos, w_pos, 1]] = num_traits::cast(u_val).unwrap_or(T::zero());
                        yuv_data[[h_pos, w_pos, 2]] = num_traits::cast(v_val).unwrap_or(T::zero());
                    }
                }
            }
        }

        let mut res = self.clone();
        res.format = ffi::AV_PIX_FMT_YUV420P;
        res.data = yuv_data;
        Ok(res)
    }

    pub fn convert_yuv_to_rgb(&self) -> Result<Self> {
        if self.media_type != MediaType::VIDEO {
            return Err(Error::msg("Only video frames can be color space converted"));
        }

        if self.format != ffi::AV_PIX_FMT_YUV420P {
            return Err(Error::msg("Only YUV420P format video frames are supported"));
        }

        let height = self.height;
        let width = self.width;

        // 1. 准备YUV数据
        let y_stride = width;
        let uv_stride = width / 2;
        let mut y_plane = vec![0u8; y_stride * height];
        let mut u_plane = vec![0u8; uv_stride * (height / 2)];
        let mut v_plane = vec![0u8; uv_stride * (height / 2)];

        // 复制Y平面数据
        for h in 0..height {
            for w in 0..width {
                y_plane[h * y_stride + w] = num_traits::cast(self.data[[h, w, 0]]).unwrap_or(0);
            }
        }

        // 复制UV平面数据
        for h in 0..height / 2 {
            for w in 0..width / 2 {
                let pos = h * uv_stride + w;
                u_plane[pos] = num_traits::cast(self.data[[h * 2, w * 2, 1]]).unwrap_or(128);
                v_plane[pos] = num_traits::cast(self.data[[h * 2, w * 2, 2]]).unwrap_or(128);
            }
        }

        // 2. 创建YUV图像
        let planar_image = YuvPlanarImage {
            y_plane: &y_plane,
            y_stride: y_stride as u32,
            u_plane: &u_plane,
            u_stride: uv_stride as u32,
            v_plane: &v_plane,
            v_stride: uv_stride as u32,
            width: width as u32,
            height: height as u32,
        };

        // 3. 准备RGB输出
        let mut rgb_bytes = vec![0u8; width * height * 3];

        // 4. 选择转换参数
        let colorspace = {
            if height < 720 {
                // SD color space
                YuvStandardMatrix::Bt601
            } else if height < 1080 {
                // HD color space
                YuvStandardMatrix::Bt709
            } else {
                // UHD color space
                YuvStandardMatrix::Bt2020
            }
        };

        // 5. 使用Full Range进行转换
        yuv::yuv420_to_rgb(
            &planar_image,
            &mut rgb_bytes,
            (width * 3) as u32,
            YuvRange::Full,
            colorspace,
        )
        .map_err(|e| Error::msg(format!("convert yuv420p to rgb24 error:{}", e)))?;

        // 6. 构建RGB数据
        let mut rgb_data = ndarray::Array3::<T>::zeros((height, width, 3));
        for h in 0..height {
            for w in 0..width {
                let idx = (h * width + w) * 3;
                rgb_data[[h, w, 0]] = num_traits::cast(rgb_bytes[idx]).unwrap_or(T::zero());
                rgb_data[[h, w, 1]] = num_traits::cast(rgb_bytes[idx + 1]).unwrap_or(T::zero());
                rgb_data[[h, w, 2]] = num_traits::cast(rgb_bytes[idx + 2]).unwrap_or(T::zero());
            }
        }

        let mut res = self.clone();
        res.format = ffi::AV_PIX_FMT_RGB24;
        res.data = rgb_data;
        Ok(res)
    }
}

/// 验证帧格式和类型大小的匹配关系
fn validate_format_type_size<T>(format: i32, expected_size: usize) -> Result<()> {
    let type_size = std::mem::size_of::<T>();
    if type_size != expected_size {
        return Err(Error::msg(format!(
            "format:{}, expected {}, got {}",
            format, expected_size, type_size
        )));
    }
    Ok(())
}

/// ndarray => AVFrame:
/// 对 U 和 V 进行下采样，恢复到 YUV420P 格式所需的较低分辨率
/// 减少数据的采样率，降低分辨率或数据量。
/// 在 YUV 视频格式中，通常对色度信息（U 和 V 分量）进行下采样，因为人眼对亮度信息比色度信息更敏感。
fn fill_video_data<T>(frame: &mut AVFrame, data: &ndarray::Array3<T>) -> Result<()>
where
    T: MediaFrameType,
{
    let (height, width, channel) = data.dim();

    if channel != 3 {
        return Err(Error::msg("Only support 3-channel video"));
    }

    // 分配视频缓冲区
    frame
        .alloc_buffer()
        .context("Failed to allocate video buffer")?;

    match frame.format {
        ffi::AV_PIX_FMT_RGB24 => {
            // 对于RGB格式，尝试直接使用连续内存
            if let Some(buffer) = data.as_standard_layout().as_slice() {
                unsafe {
                    let dst_ptr = frame.data[0] as *mut T;
                    std::ptr::copy_nonoverlapping(buffer.as_ptr(), dst_ptr, buffer.len());
                }
                Ok(())
            } else {
                Err(Error::msg("Non-contiguous video data"))
            }
        }
        ffi::AV_PIX_FMT_YUV420P => {
            // 提取 Y 平面数据
            let mut y_data = Vec::with_capacity(width * height);
            for y in 0..height {
                for x in 0..width {
                    y_data.push(data[[y, x, 0]].to_u8().unwrap());
                }
            }

            // U和V平面 - 从上采样数据中提取
            let uv_height = height / 2;
            let uv_width = width / 2;
            let mut u_data = Vec::with_capacity(uv_width * uv_height);
            let mut v_data = Vec::with_capacity(uv_width * uv_height);

            for y in 0..uv_height {
                for x in 0..uv_width {
                    // 注意： U 和 V 数据已经是下采样后的尺寸
                    u_data.push(data[[y * 2, x * 2, 1]].to_u8().unwrap());
                    v_data.push(data[[y * 2, x * 2, 2]].to_u8().unwrap());
                }
            }

            // 填充 Y、U、V 平面
            imgutils::fill_plane_from_buffer(frame, 0, y_data, width)?;
            imgutils::fill_plane_from_buffer(frame, 1, u_data, uv_width)?;
            imgutils::fill_plane_from_buffer(frame, 2, v_data, uv_width)?;

            Ok(())
        }

        _ => Err(Error::msg(format!(
            "Unsupported to_frame video format: {:?}",
            frame
        ))),
    }
}

/// 填充音频数据到AVFrame
fn fill_audio_data<T>(frame: &mut AVFrame, data: &ndarray::Array3<T>) -> Result<()>
where
    T: MediaFrameType,
{
    let (frames, samples, channels) = data.dim();
    if frames != 1 {
        return Err(Error::msg("Batch audio not supported"));
    }

    // 分配视频缓冲区
    frame
        .alloc_buffer()
        .context("Failed to allocate audio buffer")?;

    if let Some(buffer) = data.as_standard_layout().as_slice() {
        unsafe {
            if SampleFormat::from(frame.format).is_planar() {
                // 平面布局：每个声道单独存储
                for ch in 0..channels {
                    let dst = std::slice::from_raw_parts_mut(frame.data[ch] as *mut T, samples);
                    let src = &buffer[ch * samples..(ch + 1) * samples];
                    dst.copy_from_slice(src);
                }
            } else {
                // 交错布局：单块内存存储所有声道
                let dst =
                    std::slice::from_raw_parts_mut(frame.data[0] as *mut T, samples * channels);
                dst.copy_from_slice(buffer);
            }
        }
        Ok(())
    } else {
        Err(Error::msg("Non-contiguous audio data"))
    }
}

/// AVFrame => ndarray
/// 对 U 和 V 进行上采样，使其与 Y 具有相同分辨率
/// 增加数据的采样率，提高分辨率。
/// 在处理已经下采样的数据时，有时需要将其恢复到原始分辨率。
fn video_data<T>(frame: &AVFrame) -> Result<ndarray::Array3<T>>
where
    T: MediaFrameType,
{
    let (height, width) = (frame.height as usize, frame.width as usize);

    match frame.format {
        ffi::AV_PIX_FMT_RGB24 => {
            validate_format_type_size::<T>(frame.format, 1)?;

            let line_size = frame.linesize[0] as usize;
            let mut array = ndarray::Array3::<T>::default((height, width, 3));

            unsafe {
                let data_ptr = frame.data[0] as *const T;
                assert!(!data_ptr.is_null(), "frame data is null");

                // 逐行复制RGB数据
                for y in 0..height {
                    let src_row =
                        std::slice::from_raw_parts(data_ptr.add(y * line_size), width * 3);
                    for x in 0..width {
                        array[[y, x, 0]] = src_row[x * 3]; // R
                        array[[y, x, 1]] = src_row[x * 3 + 1]; // G
                        array[[y, x, 2]] = src_row[x * 3 + 2]; // B
                    }
                }
            }
            Ok(array)
        }

        ffi::AV_PIX_FMT_YUV420P => {
            validate_format_type_size::<T>(frame.format, 1)?;

            let y_line_size = frame.linesize[0] as usize;
            let uv_line_size = frame.linesize[1] as usize;
            let mut array = ndarray::Array3::<T>::default((height, width, 3));

            unsafe {
                // 复制 Y 平面
                let y_src = frame.data[0] as *const T;
                assert!(!y_src.is_null(), "frame data is null");

                for y in 0..height {
                    let src_row = std::slice::from_raw_parts(y_src.add(y * y_line_size), width);
                    for (x, &val) in src_row.iter().enumerate() {
                        array[[y, x, 0]] = val;
                    }
                }

                // 复制 UV 平面
                for (plane_idx, &plane_src) in [frame.data[1], frame.data[2]].iter().enumerate() {
                    let uv_src = plane_src as *const T;
                    assert!(!uv_src.is_null(), "UV plane data is null");

                    let ch = plane_idx + 1; // U 平面为 1，V 平面为 2
                    for y in 0..height / 2 {
                        let src_row =
                            std::slice::from_raw_parts(uv_src.add(y * uv_line_size), width / 2);
                        for x in 0..width / 2 {
                            let val = src_row[x];
                            array[[y * 2, x * 2, ch]] = val;
                            array[[y * 2 + 1, x * 2, ch]] = val;
                            array[[y * 2, x * 2 + 1, ch]] = val;
                            array[[y * 2 + 1, x * 2 + 1, ch]] = val;
                        }
                    }
                }
            }

            Ok(array)
        }

        _ => Err(Error::msg(format!(
            "Unsupported from_frame video format: {:?}",
            frame
        ))),
    }
}

/// 音频数据处理
fn audio_data<T>(frame: &AVFrame) -> Result<ndarray::Array3<T>>
where
    T: MediaFrameType,
{
    // 类型大小验证
    let sample_size = match frame.format {
        ffi::AV_SAMPLE_FMT_U8 | ffi::AV_SAMPLE_FMT_U8P => 1,
        ffi::AV_SAMPLE_FMT_S16 | ffi::AV_SAMPLE_FMT_S16P => 2,
        ffi::AV_SAMPLE_FMT_S32 | ffi::AV_SAMPLE_FMT_S32P => 4,
        ffi::AV_SAMPLE_FMT_FLT | ffi::AV_SAMPLE_FMT_FLTP => 4,
        ffi::AV_SAMPLE_FMT_DBL | ffi::AV_SAMPLE_FMT_DBLP => 8,
        ffi::AV_SAMPLE_FMT_S64 | ffi::AV_SAMPLE_FMT_S64P => 8,
        _ => return Err(Error::msg("Unsupported sample format")),
    };
    validate_format_type_size::<T>(frame.format, sample_size)?;

    // check
    if frame.data[0].is_null() {
        return Err(Error::msg("Frame data is null"));
    }

    let channels = frame.ch_layout.nb_channels as usize;
    let samples = frame.nb_samples as usize;
    let mut buffer = Vec::with_capacity(samples * channels);

    if SampleFormat::from(frame.format).is_planar() {
        // 平面格式 (FLTP)：
        // frame.data[0] -> [L0][L1][L2]...  // 左声道所有样本
        // frame.data[1] -> [R0][R1][R2]...  // 右声道所有样本
        // 检查所有通道
        for ch in 0..channels {
            if frame.data[ch].is_null() {
                return Err(Error::msg(format!("Channel {} data pointer is null", ch)));
            }
        }

        // linesize 在音频中的含义：
        // - 平面格式：每个通道的字节数（samples * sizeof(format)）
        // - 交错格式：所有通道的字节数（samples * channels * sizeof(format)）
        // 但在访问单个样本时，我们不需要使用 linesize，因为音频数据是连续存储的
        for s in 0..samples {
            for ch in 0..channels {
                unsafe {
                    // 获取当前通道的数据指针
                    let plane_ptr = frame.data[ch] as *const T;
                    buffer.push(*plane_ptr.add(s));
                }
            }
        }
    } else {
        // 交错格式布局 (FLT)：所有声道交错,直接复制
        // frame.data[0] -> [L0][R0][L1][R1]...
        unsafe {
            let data = std::slice::from_raw_parts(frame.data[0] as *const T, samples * channels);
            buffer.extend_from_slice(data);
        }
    }

    // 按 [1, samples, channels] 组织音频数据
    ndarray::Array3::from_shape_vec((1, samples, channels), buffer)
        .map_err(|_| Error::msg("Audio data shape mismatch"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsmpeg::avcodec::AVCodec;
    use std::time::Duration;

    fn create_test_pattern(width: usize, height: usize) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let mut r = vec![0u8; width * height];
        let mut g = vec![0u8; width * height];
        let mut b = vec![0u8; width * height];

        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                r[idx] = ((x as f32 / width as f32) * 255.0) as u8;
                g[idx] = ((y as f32 / height as f32) * 255.0) as u8;
                b[idx] = (((x + y) as f32 / (width + height) as f32) * 255.0) as u8;
            }
        }
        (r, g, b)
    }

    /// 创建测试用的 RGB AVFrame
    fn create_test_rgb_frame(width: usize, height: usize) -> AVFrame {
        let mut frame = AVFrame::new();
        frame.set_format(PixelFormat::RGB24.into());
        frame.set_width(width as i32);
        frame.set_height(height as i32);
        frame.alloc_buffer().unwrap();

        unsafe {
            // 填充测试数据
            let data = frame.data[0];
            let linesize = frame.linesize[0] as usize;
            for y in 0..height {
                for x in 0..width {
                    let offset = y * linesize + x * 3;
                    *data.add(offset) = (x % 256) as u8; // R
                    *data.add(offset + 1) = (y % 256) as u8; // G
                    *data.add(offset + 2) = ((x + y) % 256) as u8; // B
                }
            }
        }

        frame
    }

    /// 创建测试用的 YUV420P AVFrame
    fn create_test_yuv_frame(width: usize, height: usize) -> AVFrame {
        let mut frame = AVFrame::new();
        frame.set_format(PixelFormat::YUV420P.into());
        frame.set_width(width as i32);
        frame.set_height(height as i32);
        frame.alloc_buffer().unwrap();

        unsafe {
            // 填充 Y 平面
            let y_data = frame.data[0];
            let y_linesize = frame.linesize[0] as usize;
            for y in 0..height {
                for x in 0..width {
                    *y_data.add(y * y_linesize + x) = ((x + y) % 256) as u8;
                }
            }

            // 填充 U 平面
            let u_data = frame.data[1];
            let u_linesize = frame.linesize[1] as usize;
            for y in 0..height / 2 {
                for x in 0..width / 2 {
                    *u_data.add(y * u_linesize + x) = (x % 256) as u8;
                }
            }

            // 填充 V 平面
            let v_data = frame.data[2];
            let v_linesize = frame.linesize[2] as usize;
            for y in 0..height / 2 {
                for x in 0..width / 2 {
                    *v_data.add(y * v_linesize + x) = (y % 256) as u8;
                }
            }
        }

        frame
    }

    const TEST_WIDTH: usize = 320;
    const TEST_HEIGHT: usize = 240;
    const TIME_BASE: ffi::AVRational = ffi::AVRational { num: 1, den: 30 }; // 30 fps

    #[test]
    fn test_frame_data_access() -> Result<()> {
        let mut frame = MediaFrame::<u8>::new_video_frame(
            TEST_WIDTH,
            TEST_HEIGHT,
            PixelFormat::RGB24,
            TIME_BASE,
        )?;

        // 测试数据访问和修改
        let (r, g, b) = create_test_pattern(TEST_WIDTH, TEST_HEIGHT);

        for y in 0..TEST_HEIGHT {
            for x in 0..TEST_WIDTH {
                let idx = y * TEST_WIDTH + x;
                frame.data[[y, x, 0]] = r[idx];
                frame.data[[y, x, 1]] = g[idx];
                frame.data[[y, x, 2]] = b[idx];
            }
        }

        // 验证数据正确性
        for y in 0..TEST_HEIGHT {
            for x in 0..TEST_WIDTH {
                let idx = y * TEST_WIDTH + x;
                assert_eq!(frame.data[[y, x, 0]], r[idx]);
                assert_eq!(frame.data[[y, x, 1]], g[idx]);
                assert_eq!(frame.data[[y, x, 2]], b[idx]);
            }
        }

        Ok(())
    }

    #[test]
    fn test_rgb_yuv_conversion() -> Result<()> {
        // 创建带测试图案的 RGB 帧
        let mut rgb_frame = MediaFrame::<u8>::new_video_frame(
            TEST_WIDTH,
            TEST_HEIGHT,
            PixelFormat::RGB24,
            TIME_BASE,
        )?;

        let (r, g, b) = create_test_pattern(TEST_WIDTH, TEST_HEIGHT);
        for y in 0..TEST_HEIGHT {
            for x in 0..TEST_WIDTH {
                let idx = y * TEST_WIDTH + x;
                rgb_frame.data[[y, x, 0]] = r[idx];
                rgb_frame.data[[y, x, 1]] = g[idx];
                rgb_frame.data[[y, x, 2]] = b[idx];
            }
        }

        // RGB -> YUV 转换
        let yuv_frame = rgb_frame.convert_rgb_to_yuv()?;
        assert_eq!(yuv_frame.format, ffi::AV_PIX_FMT_YUV420P);

        // YUV -> RGB 转换回来
        let converted_rgb = yuv_frame.convert_yuv_to_rgb()?;
        assert_eq!(converted_rgb.format, ffi::AV_PIX_FMT_RGB24);

        // 验证转换后的颜色值（允许有小的误差）
        for h in 0..TEST_HEIGHT {
            for w in 0..TEST_WIDTH {
                for c in 0..3 {
                    let diff = (converted_rgb.data[[h, w, c]] as i16
                        - rgb_frame.data[[h, w, c]] as i16)
                        .abs();
                    assert!(
                        diff <= 3,
                        "Color difference too large: {} at [{}, {}, {}]: original={}, converted={}",
                        diff,
                        h,
                        w,
                        c,
                        rgb_frame.data[[h, w, c]],
                        converted_rgb.data[[h, w, c]]
                    );
                }
            }
        }

        Ok(())
    }

    #[test]
    fn test_different_pixel_types() -> Result<()> {
        // 测试不同像素类型
        let frame_u8 = MediaFrame::<u8>::new_video_frame(
            TEST_WIDTH,
            TEST_HEIGHT,
            PixelFormat::RGB24,
            TIME_BASE,
        )?;
        assert_eq!(std::mem::size_of_val(&frame_u8.data[[0, 0, 0]]), 1);

        let frame_u16 = MediaFrame::<u16>::new_video_frame(
            TEST_WIDTH,
            TEST_HEIGHT,
            PixelFormat::RGB24,
            TIME_BASE,
        )?;
        assert_eq!(std::mem::size_of_val(&frame_u16.data[[0, 0, 0]]), 2);

        let frame_f32 = MediaFrame::<f32>::new_video_frame(
            TEST_WIDTH,
            TEST_HEIGHT,
            PixelFormat::RGB24,
            TIME_BASE,
        )?;
        assert_eq!(std::mem::size_of_val(&frame_f32.data[[0, 0, 0]]), 4);

        Ok(())
    }

    #[test]
    fn test_format_conversion_chain() -> Result<()> {
        // 测试多次转换
        let mut original = MediaFrame::<u8>::new_video_frame(
            TEST_WIDTH,
            TEST_HEIGHT,
            PixelFormat::RGB24,
            TIME_BASE,
        )?;

        // 填充测试数据
        let (r, g, b) = create_test_pattern(TEST_WIDTH, TEST_HEIGHT);
        for y in 0..TEST_HEIGHT {
            for x in 0..TEST_WIDTH {
                let idx = y * TEST_WIDTH + x;
                original.data[[y, x, 0]] = r[idx];
                original.data[[y, x, 1]] = g[idx];
                original.data[[y, x, 2]] = b[idx];
            }
        }

        // RGB -> YUV -> RGB -> YUV -> RGB
        let converted = original
            .convert_rgb_to_yuv()?
            .convert_yuv_to_rgb()?
            .convert_rgb_to_yuv()?
            .convert_yuv_to_rgb()?;

        // 验证多次转换后的数据 , 允许稍大的误差（因为多次转换）
        for y in 0..TEST_HEIGHT {
            for x in 0..TEST_WIDTH {
                for c in 0..3 {
                    let diff =
                        (converted.data[[y, x, c]] as i16 - original.data[[y, x, c]] as i16).abs();
                    assert!(
                        diff <= 5,
                        "Color difference too large: {} at [{}, {}, {}]",
                        diff,
                        y,
                        x,
                        c
                    );
                }
            }
        }

        Ok(())
    }

    #[test]
    fn test_frame_timestamps() -> Result<()> {
        let fps = TIME_BASE.den as f64 / TIME_BASE.num as f64;
        let frame_duration = Duration::from_secs_f64(1.0 / fps);

        let mut frames = Vec::new();
        for i in 0..5 {
            let mut frame = MediaFrame::<u8>::new_video_frame(
                TEST_WIDTH,
                TEST_HEIGHT,
                PixelFormat::RGB24,
                TIME_BASE,
            )?;
            frame.set_pts(i as i64);
            frames.push(frame);
        }

        // 验证时间戳的正确性
        for (i, frame) in frames.iter().enumerate() {
            let expected_time = frame_duration * i as u32;
            let actual_time = Duration::from_secs_f64(
                frame.pts as f64 * frame.time_base.num as f64 / frame.time_base.den as f64,
            );
            assert!((actual_time - expected_time).as_secs_f32().abs() < 0.01);
        }

        Ok(())
    }

    #[test]
    fn test_rgb_value_transformations() {
        // 测试颜色值转换
        let rgb = create_test_rgb_frame(640, 640);
        let mut rgb_frame = MediaFrame::<u8>::from_avframe(&rgb).unwrap();

        // 测试一些典型的颜色值
        let test_colors = [
            (255, 0, 0),     // 红色
            (0, 255, 0),     // 绿色
            (0, 0, 255),     // 蓝色
            (255, 255, 255), // 白色
        ];

        for (i, &(r, g, b)) in test_colors.iter().enumerate() {
            let y = i / 2;
            let x = i % 2;
            rgb_frame.data[[y, x, 0]] = r;
            rgb_frame.data[[y, x, 1]] = g;
            rgb_frame.data[[y, x, 2]] = b;
        }

        // 验证颜色值
        for (i, &(r, g, b)) in test_colors.iter().enumerate() {
            let y = i / 2;
            let x = i % 2;
            assert_eq!(rgb_frame.data[[y, x, 0]], r, "Red value mismatch");
            assert_eq!(rgb_frame.data[[y, x, 1]], g, "Green value mismatch");
            assert_eq!(rgb_frame.data[[y, x, 2]], b, "Blue value mismatch");
        }
    }

    #[test]
    fn test_create_rgb24_frame() -> Result<()> {
        let width = 1920;
        let height = 1080;
        let time_base = ffi::AVRational { num: 1, den: 30 }; // 30 fps

        let mut frame =
            MediaFrame::<u8>::new_video_frame(width, height, PixelFormat::RGB24, time_base)?;

        // 填充一些测试数据
        for y in 0..height {
            for x in 0..width {
                frame.data[[y, x, 0]] = (x % 255) as u8; // R
                frame.data[[y, x, 1]] = (y % 255) as u8; // G
                frame.data[[y, x, 2]] = ((x + y) % 255) as u8; // B
            }
        }

        // 验证
        assert_eq!(frame.width, width);
        assert_eq!(frame.height, height);
        assert_eq!(frame.format, ffi::AV_PIX_FMT_RGB24);

        Ok(())
    }

    #[test]
    fn test_create_yuv420p_frame() -> Result<()> {
        let width = 1920;
        let height = 1080;

        // 创建空的YUV420P帧
        let yuv_frame = create_test_yuv_frame(width, height);

        let mut frame = MediaFrame::<u8>::from_avframe(&yuv_frame)?;

        // array 重新 填充一些测试数据
        for y in 0..height {
            for x in 0..width {
                frame.data[[y, x, 0]] = (x + y) as u8; // Y
                if y % 2 == 0 && x % 2 == 0 {
                    frame.data[[y, x, 1]] = 128u8; // U
                    frame.data[[y, x, 2]] = 128u8; // V
                }
            }
        }

        // 验证
        assert_eq!(frame.width, width);
        assert_eq!(frame.height, height);
        assert_eq!(frame.format, ffi::AV_PIX_FMT_YUV420P);

        Ok(())
    }

    #[test]
    fn test_create_audio_frame() -> Result<()> {
        let samples = 1024;
        let channels = 2;
        let sample_rate = 44100;
        let time_base = ffi::AVRational {
            num: 1,
            den: sample_rate as i32,
        };

        // 创建空的音频帧 (Float Planar format)
        let mut frame = MediaFrame::<f32>::new_audio_frame(
            SampleFormat::FLTP,
            channels,
            samples,
            sample_rate,
            time_base,
        )?;

        // 填充一些测试数据
        for s in 0..samples as usize {
            for ch in 0..channels as usize {
                // 生成简单的正弦波
                let t = s as f32 / sample_rate as f32;
                let freq = 440.0 * (ch + 1) as f32; // 不同通道使用不同频率
                frame.data[[0, s, ch]] = (2.0 * std::f32::consts::PI * freq * t).sin();
            }
        }

        // 验证
        assert_eq!(frame.nb_samples, samples);
        assert_eq!(frame.nb_channels, channels);
        assert_eq!(frame.sample_rate, sample_rate);
        assert_eq!(frame.format, ffi::AV_SAMPLE_FMT_FLTP);

        Ok(())
    }

    #[test]
    fn test_video_rgb24_frame_conversion() -> Result<()> {
        // 创建测试视频帧
        let mut frame = AVFrame::new();
        frame.set_width(320);
        frame.set_height(240);
        frame.set_format(ffi::AV_PIX_FMT_RGB24);
        frame.alloc_buffer()?;

        // 填充测试数据
        unsafe {
            let data = std::slice::from_raw_parts_mut(
                frame.data[0] as *mut u8,
                frame.height as usize * frame.width as usize * 3,
            );
            for (i, byte) in data.iter_mut().enumerate() {
                *byte = (i % 255) as u8;
            }
        }

        // 转换为 MediaFrame
        let media_frame = MediaFrame::<u8>::from_avframe(&frame)?;

        // 验证维度
        assert_eq!(media_frame.data.dim(), (240, 320, 3));
        assert_eq!(media_frame.width, 320);
        assert_eq!(media_frame.height, 240);

        // 验证数据
        let first_pixel = media_frame.data.slice(ndarray::s![0, 0, ..]);
        assert_eq!(first_pixel.to_vec(), vec![0, 1, 2]);

        Ok(())
    }

    #[test]
    fn test_video_yuv420p_frame_conversion() -> Result<()> {
        let width = 320;
        let height = 240;

        let mut frame = AVFrame::new();
        frame.set_width(width);
        frame.set_height(height);
        frame.set_format(ffi::AV_PIX_FMT_YUV420P);
        frame.alloc_buffer()?;

        // 填充测试数据
        // Y 平面使用全分辨率 (width × height)
        // U 平面使用 1/4 分辨率 ((width/2) × (height/2))
        // V 平面使用 1/4 分辨率 ((width/2) × (height/2))
        unsafe {
            // Y 平面 (全分辨率)
            let y_data = std::slice::from_raw_parts_mut(
                frame.data[0] as *mut u8,
                height as usize * width as usize,
            );
            for (i, byte) in y_data.iter_mut().enumerate() {
                *byte = (i % 255) as u8;
            }

            // U 平面 (1/4分辨率)
            let u_data = std::slice::from_raw_parts_mut(
                frame.data[1] as *mut u8,
                (height as usize / 2) * (width as usize / 2),
            );
            for (i, byte) in u_data.iter_mut().enumerate() {
                *byte = ((i + 85) % 255) as u8;
            }

            // V 平面 (1/4分辨率)
            let v_data = std::slice::from_raw_parts_mut(
                frame.data[2] as *mut u8,
                (height as usize / 2) * (width as usize / 2),
            );
            for (i, byte) in v_data.iter_mut().enumerate() {
                *byte = ((i + 170) % 255) as u8;
            }
        }

        // 转换为 MediaFrame
        let media_frame = MediaFrame::<u8>::from_avframe(&frame)?;

        // 验证维度
        assert_eq!(media_frame.data.dim(), (height as usize, width as usize, 3));
        assert_eq!(media_frame.width, width as usize);
        assert_eq!(media_frame.height, height as usize);

        // 验证数据
        unsafe {
            // 验证 Y 分量
            let y_val = *frame.data[0] as u8;
            assert_eq!(media_frame.data[[0, 0, 0]], y_val);

            // 验证 U 分量 (2x2块使用相同的值)
            let u_val = *frame.data[1] as u8;
            assert_eq!(media_frame.data[[0, 0, 1]], u_val);
            assert_eq!(media_frame.data[[0, 1, 1]], u_val);
            assert_eq!(media_frame.data[[1, 0, 1]], u_val);
            assert_eq!(media_frame.data[[1, 1, 1]], u_val);

            // 验证 V 分量 (2x2块使用相同的值)
            let v_val = *frame.data[2] as u8;
            assert_eq!(media_frame.data[[0, 0, 2]], v_val);
            assert_eq!(media_frame.data[[0, 1, 2]], v_val);
            assert_eq!(media_frame.data[[1, 0, 2]], v_val);
            assert_eq!(media_frame.data[[1, 1, 2]], v_val);
        }

        // 验证上采样是否正确
        // 检查第一个2x2块的 U 分量
        let u_block = media_frame.data.slice(ndarray::s![0..2, 0..2, 1]);
        let u_val = u_block[(0, 0)];
        for y in 0..2 {
            for x in 0..2 {
                assert_eq!(u_block[[y, x]], u_val, "U value mismatch at [{}, {}]", y, x);
            }
        }

        // 检查第一个2x2块的 V 分量
        let v_block = media_frame.data.slice(ndarray::s![0..2, 0..2, 2]);
        let v_val = v_block[(0, 0)];
        for y in 0..2 {
            for x in 0..2 {
                assert_eq!(v_block[[y, x]], v_val, "V value mismatch at [{}, {}]", y, x);
            }
        }

        // 验证转换回 AVFrame
        let converted_frame = media_frame.to_avframe()?;
        assert_eq!(converted_frame.format, ffi::AV_PIX_FMT_YUV420P);
        assert_eq!(converted_frame.width, width);
        assert_eq!(converted_frame.height, height);

        // 验证转换后的数据
        // 考虑了行步长(linesize)的影响
        // 分别处理每个平面的数据
        // 逐行比较而不是整块比较
        // 为 UV 平面使用正确的宽度和高度（原尺寸的一半）
        unsafe {
            // 验证 Y 平面
            let y_linesize = frame.linesize[0] as usize;
            let converted_y_linesize = converted_frame.linesize[0] as usize;

            // 逐行比较 Y 平面数据
            for y in 0..height as usize {
                let original_line = std::slice::from_raw_parts(
                    frame.data[0].add(y * y_linesize) as *const u8,
                    width as usize,
                );
                let converted_line = std::slice::from_raw_parts(
                    converted_frame.data[0].add(y * converted_y_linesize) as *const u8,
                    width as usize,
                );
                assert_eq!(
                    original_line, converted_line,
                    "Y plane mismatch at line {}",
                    y
                );
            }

            // 验证 U 平面
            let u_linesize = frame.linesize[1] as usize;
            let converted_u_linesize = converted_frame.linesize[1] as usize;
            let uv_height = height as usize / 2;
            let uv_width = width as usize / 2;

            // 逐行比较 U 平面数据
            for y in 0..uv_height {
                let original_line = std::slice::from_raw_parts(
                    frame.data[1].add(y * u_linesize) as *const u8,
                    uv_width,
                );
                let converted_line = std::slice::from_raw_parts(
                    converted_frame.data[1].add(y * converted_u_linesize) as *const u8,
                    uv_width,
                );
                assert_eq!(
                    original_line, converted_line,
                    "U plane mismatch at line {}",
                    y
                );
            }

            // 验证 V 平面
            let v_linesize = frame.linesize[2] as usize;
            let converted_v_linesize = converted_frame.linesize[2] as usize;

            // 逐行比较 V 平面数据
            for y in 0..uv_height {
                let original_line = std::slice::from_raw_parts(
                    frame.data[2].add(y * v_linesize) as *const u8,
                    uv_width,
                );
                let converted_line = std::slice::from_raw_parts(
                    converted_frame.data[2].add(y * converted_v_linesize) as *const u8,
                    uv_width,
                );
                assert_eq!(
                    original_line, converted_line,
                    "V plane mismatch at line {}",
                    y
                );
            }
        }

        Ok(())
    }

    /// * nb_samples: 是音频数据的逻辑单位,表示一帧音频中包含的采样点数量,
    ///   用于音频处理和时间计算, 与音频格式无关, 与时间相关：nb_samples/sample_rate = 帧的持续时间
    /// * frame_size: 是内存/存储的物理单位,表示一帧音频数据的实际字节大小,
    ///   用于内存分配和缓冲区管理,依赖于具体的音频格式（平面/非平面）
    ///
    /// (1)对于非平面格式（如 AV_SAMPLE_FMT_FLT）
    /// frame_size = nb_samples * nb_channels * bytes_per_sample
    /// 例如：1024 * 2 * 4 = 8192 bytes
    ///
    /// (2)对于平面格式（如 AV_SAMPLE_FMT_FLTP）
    /// frame_size = nb_samples * bytes_per_sample
    /// 每个通道分别: 1024 * 4 = 4096 bytes
    #[test]
    fn test_audio_planar_frame_conversion() -> Result<()> {
        let nb_channels = 2;
        let nb_samples = 1024;
        let sample_rate = 44100;

        // 创建测试音频帧
        let mut frame = AVFrame::new();
        frame.set_format(ffi::AV_SAMPLE_FMT_FLTP);
        frame.set_nb_samples(nb_samples);
        frame.set_sample_rate(sample_rate);
        // frame.set_time_base(avutil::ra(1, sample_rate));
        frame.set_ch_layout(AVChannelLayout::from_nb_channels(nb_channels).into_inner());
        frame
            .alloc_buffer()
            .context("Failed to allocate buffer for AVFrame")?;

        // 填充测试数据
        // 平面格式 (AV_SAMPLE_FMT_FLTP) 的数据布局：
        // data[0]: [L1 L2 L3 ...] (左声道所有样本)
        // data[1]: [R1 R2 R3 ...] (右声道所有样本)
        let total_samples = (nb_samples * nb_channels) as usize;
        unsafe {
            for ch in 0..nb_channels as usize {
                let data =
                    std::slice::from_raw_parts_mut(frame.data[ch] as *mut f32, nb_samples as usize);
                for i in 0..data.len() {
                    data[i] = (i * nb_channels as usize + ch) as f32 / total_samples as f32;
                }
            }
        }

        // 转换为 MediaFrame
        let media_frame = MediaFrame::<f32>::from_avframe(&frame)?;

        // 验证维度
        assert_eq!(
            media_frame.data.dim(),
            (1, nb_samples as usize, nb_channels as usize)
        );
        assert_eq!(media_frame.nb_samples, nb_samples as u32);
        assert_eq!(media_frame.nb_channels, nb_channels as u32);

        // 验证数据
        let first_sample = media_frame.data.slice(ndarray::s![0, 0, ..]);
        println!("{:#?}", first_sample);
        assert_eq!(first_sample.to_vec(), vec![0.0, 1.0 / total_samples as f32]);

        let converted_frame = media_frame.to_avframe().unwrap();
        assert_eq!(converted_frame.format, ffi::AV_SAMPLE_FMT_FLTP);
        assert_eq!(converted_frame.nb_samples, nb_samples);
        assert_eq!(converted_frame.sample_rate, sample_rate);
        assert_eq!(converted_frame.linesize, frame.linesize);
        assert_eq!(converted_frame.data.len(), frame.data.len());

        // 验证转换后的数据
        unsafe {
            for ch in 0..nb_channels as usize {
                let original_data =
                    std::slice::from_raw_parts(frame.data[ch] as *const f32, nb_samples as usize);
                let converted_data = std::slice::from_raw_parts(
                    converted_frame.data[ch] as *const f32,
                    nb_samples as usize,
                );

                // 验证每个样本
                for i in 0..nb_samples as usize {
                    let orig = original_data[i];
                    let conv = converted_data[i];
                    let diff = (orig - conv).abs();
                    assert!(
                        diff < 1.0,
                        "Mismatch at channel {} sample {}: expected {}, got {}, diff {}",
                        ch,
                        i,
                        orig,
                        conv,
                        diff
                    );
                }
            }
        }

        Ok(())
    }

    #[test]
    fn test_audio_interleaved_frame_conversion() -> Result<()> {
        let nb_channels = 2;
        let nb_samples = 1024;
        let sample_rate = 44100;

        let mut frame = AVFrame::new();
        frame.set_format(ffi::AV_SAMPLE_FMT_FLT);
        frame.set_nb_samples(nb_samples);
        frame.set_sample_rate(sample_rate);
        // frame.set_time_base(avutil::ra(1, sample_rate));
        frame.set_ch_layout(AVChannelLayout::from_nb_channels(nb_channels).into_inner());
        frame
            .alloc_buffer()
            .context("Failed to allocate buffer for AVFrame")?;

        // 填充测试数据
        // 交错格式 (AV_SAMPLE_FMT_FLT) 的数据布局：
        // data[0]: [L1 R1 L2 R2 L3 R3 ...] (左右声道交错)
        let total_samples = (nb_samples * nb_channels) as usize;
        unsafe {
            let data = std::slice::from_raw_parts_mut(frame.data[0] as *mut f32, total_samples);
            for i in 0..data.len() {
                // 交错格式本身就是按照样本点交错排列的
                data[i] = (i / total_samples) as f32;
            }
        }

        // 转换为 MediaFrame
        let media_frame = MediaFrame::<f32>::from_avframe(&frame)?;

        // 验证维度
        assert_eq!(
            media_frame.data.dim(),
            (1, nb_samples as usize, nb_channels as usize)
        );
        assert_eq!(media_frame.nb_samples, nb_samples as u32);
        assert_eq!(media_frame.nb_channels, nb_channels as u32);

        // 验证数据
        let first_sample = media_frame.data.slice(ndarray::s![0, 0, ..]);
        println!("{:#?}", first_sample);
        assert_eq!(first_sample.to_vec(), vec![0.0, 0.0]);

        let converted_frame = media_frame.to_avframe().unwrap();
        assert_eq!(converted_frame.format, ffi::AV_SAMPLE_FMT_FLT);
        assert_eq!(converted_frame.nb_samples, nb_samples);
        assert_eq!(converted_frame.sample_rate, sample_rate);
        assert_eq!(converted_frame.linesize, frame.linesize);
        assert_eq!(converted_frame.data.len(), frame.data.len());

        // 验证转换后的数据
        unsafe {
            let original_data =
                std::slice::from_raw_parts(frame.data[0] as *const f32, total_samples);
            let converted_data =
                std::slice::from_raw_parts(converted_frame.data[0] as *const f32, total_samples);

            // 直接比较所有数据
            for i in 0..original_data.len() {
                assert_eq!(
                    original_data[i], converted_data[i],
                    "Mismatch at index {}",
                    i
                );
            }
        }

        Ok(())
    }

    #[test]
    fn test_get_audio_buffer() {
        let encoder = AVCodec::find_encoder(ffi::AV_CODEC_ID_AAC).unwrap();
        println!("aac sample_fmts:{:#?}", encoder.sample_fmts());
        let mut frame = AVFrame::new();
        frame.set_nb_samples(1024);
        frame.set_format(encoder.sample_fmts().unwrap()[0]);
        frame.set_ch_layout(AVChannelLayout::from_nb_channels(2).into_inner());
        assert!(frame.alloc_buffer().is_ok());
    }
}
