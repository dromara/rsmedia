use crate::PixelFormat;

use anyhow::Error;
use rsmpeg::avutil::AVImage;
use rsmpeg::ffi;

///////////////////////////////////////////////////////////////////////////////////////////////
/////////////////////////////////// MyAVImage /////////////////////////////////////////////////
///////////////////////////////////////////////////////////////////////////////////////////////

pub struct MyAVImage(pub AVImage);

impl MyAVImage {
    /// 封装 `av_image_alloc`
    pub fn alloc(
        pix_fmt: PixelFormat,
        width: i32,
        height: i32,
        align: i32,
    ) -> anyhow::Result<Self> {
        let mut data: [*mut u8; ffi::AV_NUM_DATA_POINTERS as usize] =
            [std::ptr::null_mut(); ffi::AV_NUM_DATA_POINTERS as usize];
        let mut line_sizes: [i32; ffi::AV_NUM_DATA_POINTERS as usize] =
            [0; ffi::AV_NUM_DATA_POINTERS as usize];

        let buffer_size = unsafe {
            ffi::av_image_alloc(
                data.as_mut_ptr(),
                line_sizes.as_mut_ptr(),
                width,
                height,
                pix_fmt.into(),
                align,
            )
        };

        if buffer_size < 0 {
            return Err(Error::msg("Failed to allocate image buffer"));
        }

        // 将分配的内存转换为 Vec<u8> 管理
        let buffer =
            unsafe { Vec::from_raw_parts(data[0], buffer_size as usize, buffer_size as usize) };

        // Here we leak a vector to "pin" it.
        let linear = Box::leak(Box::new(buffer));

        let mut img = unsafe { AVImage::from_raw(std::ptr::NonNull::new(linear).unwrap()) };
        img.data = data;
        img.linesizes = line_sizes;
        img.width = width;
        img.height = height;
        img.pix_fmt = pix_fmt.into();
        Ok(MyAVImage(img))
    }

    pub fn new(img: AVImage) -> Self {
        Self(img)
    }

    pub fn into_inner(self) -> AVImage {
        self.0
    }
}

/// See ffi::av_image_fill_linesizes
pub fn fill_linesizes(pix_fmt: PixelFormat, width: i32) -> anyhow::Result<[i32; 4]> {
    let mut linesizes = [0; 4];
    let ret =
        unsafe { ffi::av_image_fill_linesizes(linesizes.as_mut_ptr(), pix_fmt.into(), width) };

    // >= 0 in case of success, a negative error code otherwise
    if ret < 0 {
        return Err(Error::msg(format!("Failed to fill linesizes: {}", ret)));
    }

    Ok(linesizes)
}

/// See ffi::av_image_get_linesize
pub fn get_linesize(pix_fmt: PixelFormat, width: u32, plane: usize) -> anyhow::Result<usize> {
    // Safe because format is a valid format and this function is pure computation.
    let ret = unsafe { ffi::av_image_get_linesize(pix_fmt.into(), width as _, plane as _) };

    // returns the computed size in bytes
    if ret <= 0 {
        return Err(Error::msg(format!("Failed to get line size, ret: {}", ret)));
    }

    Ok(ret as usize)
}

/// See ffi::av_image_fill_plane_sizes.
pub fn fill_plane_sizes<I: IntoIterator<Item = u32>>(
    format: PixelFormat,
    linesizes: I,
    height: u32,
) -> anyhow::Result<Vec<usize>> {
    const MAX_FFMPEG_PLANES: usize = 4;

    let mut linesizes_buf = [0; MAX_FFMPEG_PLANES];
    let mut planes = 0;
    for (i, linesize) in linesizes.into_iter().take(MAX_FFMPEG_PLANES).enumerate() {
        linesizes_buf[i] = linesize as _;
        planes += 1;
    }
    let mut plane_sizes_buf = [0; MAX_FFMPEG_PLANES];

    // Safe because plane_sizes_buf and linesizes_buf have the size specified by the API, format is
    // valid, and this function doesn't have any side effects other than writing to plane_sizes_buf.
    let ret = unsafe {
        ffi::av_image_fill_plane_sizes(
            plane_sizes_buf.as_mut_ptr(),
            format.into(),
            height as _,
            linesizes_buf.as_ptr(),
        )
    };

    // >= 0 in case of success, a negative error code otherwise
    if ret < 0 {
        return Err(Error::msg(format!(
            "Failed to fill plane sizes, ret: {}",
            ret
        )));
    }

    Ok(plane_sizes_buf
        .into_iter()
        .map(|x| x as _)
        .take(planes)
        .collect())
}

/// Check if the given dimension of an image is valid, meaning that all
/// bytes of a plane of an image with the specified pix_fmt can be addressed with a signed int.
///
/// # Arguments
///
/// * @param w the width of the picture
/// * @param h the height of the picture
/// * @param max_pixels the maximum number of pixels the user wants to accept
/// * @param pix_fmt the pixel format, can be AV_PIX_FMT_NONE if unknown.
/// * @param log_offset the offset to sum to the log level for logging with log_ctx
/// * @param log_ctx the parent logging context, it may be NULL
/// * @return >= 0 if valid, a negative error code otherwise
pub fn check_size2(
    width: u32,
    height: u32,
    max_pixels: i64,
    pix_fmt: PixelFormat,
) -> anyhow::Result<()> {
    let ret = unsafe {
        ffi::av_image_check_size2(
            width,
            height,
            max_pixels,
            pix_fmt.into(),
            0,
            std::ptr::null_mut(),
        )
    };

    // >= 0 if valid, a negative error code otherwise
    if ret < 0 {
        return Err(Error::msg(format!("Failed to check size2, ret: {}", ret)));
    }
    Ok(())
}

impl std::ops::Deref for MyAVImage {
    type Target = AVImage;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for MyAVImage {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_my_avimage() -> anyhow::Result<()> {
        // 测试参数
        let width = 1920;
        let height = 1080;
        let pix_fmt = PixelFormat::RGB24;
        let align = 32;

        // 1. 测试图像大小检查
        check_size2(
            width as u32,
            height as u32,
            (width * height * 3) as i64,
            pix_fmt,
        )?;

        // 2. 创建图像
        let img = MyAVImage::alloc(pix_fmt, width, height, align)?;
        assert_eq!(img.width, width);
        assert_eq!(img.height, height);
        assert_eq!(img.pix_fmt, pix_fmt.into());

        // 打印调试信息
        println!("Test configuration:");
        println!("Width: {}, Height: {}", width, height);
        println!("Pixel format: {:?}", pix_fmt);
        println!("Alignment: {}", align);

        Ok(())
    }

    #[test]
    fn test_image_linesize_planar() -> anyhow::Result<()> {
        // --------------------------
        // 测试用例1: YUV420P 格式
        // --------------------------
        // 输入：3个平面（Y/U/V）的行大小 [640, 320, 320]，高度 480。
        // 输出：平面大小计算规则：
        //      Y平面：行大小 * 高度 → 640 * 480 = 307200
        //      U/V平面：行大小 * (高度 / 2) → 320 * 240 = 76800（因色度子采样）
        let yuv_fmt = PixelFormat::YUV420P;
        let yuv_width = 640;
        let yuv_height = 480;

        // 步骤1：获取各平面行大小
        let yuv_linesizes = fill_linesizes(yuv_fmt, yuv_width)?;
        assert_eq!(
            yuv_linesizes,
            [640, 320, 320, 0],
            "YUV420P linesizes mismatch"
        );

        // 步骤2：验证 av_image_line_size 返回值
        assert_eq!(
            get_linesize(yuv_fmt, yuv_width as u32, 0)?,
            640,
            "Y plane linesize incorrect"
        );
        assert_eq!(
            get_linesize(yuv_fmt, yuv_width as u32, 1)?,
            320,
            "U plane linesize incorrect"
        );
        assert_eq!(
            get_linesize(yuv_fmt, yuv_width as u32, 2)?,
            320,
            "V plane linesize incorrect"
        );

        // 步骤3：计算平面大小
        let plane_sizes = fill_plane_sizes(
            yuv_fmt,
            yuv_linesizes[..3].iter().map(|&x| x as u32),
            yuv_height as u32,
        )?;
        // 预期结果：
        // Y: 640 * 480 = 307200
        // U: 320 * 240 = 76800
        // V: 320 * 240 = 76800
        assert_eq!(plane_sizes.len(), 3);
        assert_eq!(plane_sizes[0], 307200);
        assert_eq!(plane_sizes[1], 76800);
        assert_eq!(plane_sizes[2], 76800);

        // --------------------------
        // 测试用例2: RGBA 格式
        // --------------------------
        // 输入：单平面行大小 1280
        // 输出：单平面大小 1280 * 720 = 921600
        let rgba_fmt = PixelFormat::RGBA;
        let rgba_width = 320;
        let rgba_height = 720;

        // 步骤1：获取行大小（单平面）
        let rgba_linesizes = fill_linesizes(rgba_fmt, rgba_width)?;
        assert_eq!(rgba_linesizes, [1280, 0, 0, 0], "RGBA linesizes mismatch");

        // 步骤2：验证 av_image_line_size
        assert_eq!(
            get_linesize(rgba_fmt, rgba_width as u32, 0)?,
            1280,
            "RGBA plane linesize incorrect"
        );

        // 步骤3：计算平面大小
        let plane_sizes =
            fill_plane_sizes(rgba_fmt, vec![rgba_linesizes[0] as u32], rgba_height as u32)?;
        // 预期结果：1280 * 720 = 921600
        assert_eq!(plane_sizes.len(), 1);
        assert_eq!(plane_sizes[0], 921600);

        // --------------------------
        // 测试用例3: NV12 格式（YUV420半平面，UV交错）
        // --------------------------
        let nv12_fmt = PixelFormat::NV12;
        let nv12_width = 640;
        let nv12_height = 480;

        // 步骤1：获取各平面行大小
        let linesizes = fill_linesizes(nv12_fmt, nv12_width)?;
        assert_eq!(
            linesizes,
            [640, 640, 0, 0], // NV12只有两个平面：Y（行640）、UV（行640）
            "NV12 linesizes mismatch"
        );

        // 步骤2：验证 av_image_line_size 返回值
        assert_eq!(
            get_linesize(nv12_fmt, nv12_width as u32, 0)?,
            640,
            "NV12 Y plane linesize incorrect"
        );
        assert_eq!(
            get_linesize(nv12_fmt, nv12_width as u32, 1)?,
            640,
            "NV12 UV plane linesize incorrect"
        );

        // 错误测试：访问不存在的平面（索引2）
        assert!(
            get_linesize(nv12_fmt, nv12_width as u32, 2).is_err(),
            "NV12 should reject plane index 2"
        );

        // 步骤3：计算平面大小
        let plane_sizes = fill_plane_sizes(
            nv12_fmt,
            vec![linesizes[0] as u32, linesizes[1] as u32], // 传入两个平面
            nv12_height as u32,
        )?;

        // 预期结果：
        // Y平面：640 * 480 = 307200
        // UV平面：640 * (480 / 2) = 153600
        assert_eq!(plane_sizes.len(), 2);
        assert_eq!(plane_sizes[0], 307200);
        assert_eq!(plane_sizes[1], 153600);

        Ok(())
    }

    #[test]
    fn test_image_linesize_error() -> anyhow::Result<()> {
        let yuv_fmt = PixelFormat::YUV420P;

        // --------------------------
        // 测试用例3: 错误场景
        // --------------------------
        // 错误1：无效像素格式
        assert!(
            get_linesize(PixelFormat::NONE, 640, 0).is_err(),
            "None format should fail"
        );

        // 错误2：越界平面索引（YUV420P只有3个平面）
        assert!(
            get_linesize(yuv_fmt, 640, 3).is_err(),
            "Plane index 3 should be invalid for YUV420P"
        );

        // 错误3：非法宽度（0或负数）
        assert!(get_linesize(yuv_fmt, 0, 0).is_err(), "Width 0 should fail");

        // 错误4：传入过多平面（超过4个）
        let oversized_input = vec![640, 320, 320, 128, 64];
        assert!(
            fill_plane_sizes(yuv_fmt, oversized_input, 480).is_ok(),
            "Should truncate to first 4 planes"
        );

        Ok(())
    }

    #[test]
    fn test_image_operations() -> anyhow::Result<()> {
        // 创建测试图像
        let width = 640;
        let height = 480;
        let pix_fmt = PixelFormat::RGB24;

        let image = MyAVImage::alloc(pix_fmt, width, height, 1)?;
        let img = image.into_inner();
        println!(
            "img width: {}, height: {}, pix_fmt: {:?}",
            img.width, img.height, img.pix_fmt
        );

        Ok(())
    }

    #[test]
    fn test_memory_management() -> anyhow::Result<()> {
        // 测试大尺寸图像的内存分配和释放
        let sizes = [(1920, 1080), (3840, 2160), (7680, 4320)];

        for &(width, height) in &sizes {
            let image = MyAVImage::alloc(PixelFormat::RGB24, width, height, 32)?;

            // 验证内存对齐
            for &linesize in &image.linesizes {
                if linesize > 0 {
                    assert_eq!(linesize % 32, 0);
                }
            }
        }

        Ok(())
    }
}
