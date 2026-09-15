//! Bitstream filter（`AVBSFContext`）：**不解码**地重写压缩包的封装格式。
//!
//! 与 [`Encoder`](crate::Encoder) 的转码路径相对，bitstream filter 工作在
//! **包级别**：只调整参数集位置、NALU 边界表示、音频帧头等容器约定，编码数据
//! 本身不变——零质量损失，开销可忽略。典型场景：
//!
//! - `h264_mp4toannexb` / `hevc_mp4toannexb`：MP4/MKV（avcC/hvcC）→ TS/裸流
//!   （Annex B 起始码 + 参数集内联）——转封装 TS/IPTV 的刚需；
//! - `aac_adtstoasc`：ADTS（TS）→ raw + ASC（MP4/M4A），不做则报
//!   `Malformed AAC bitstream detected`。
//!
//! 注意：与 `ffmpeg` CLI 不同（CLI 在 mux 时自动插入所需 bsf），库用户必须
//! 显式创建并在包通路上驱动它；过滤后输出流的 codecpar 应以
//! [`Bsf::par_out`] 为准再交给 [`Muxer`](crate::Muxer)。
//!
//! # 用法
//!
//! ```no_run
//! use rsmedia::bsf::Bsf;
//! use rsmedia::mux::Demuxer;
//!
//! # fn main() -> rsmedia::error::Result<()> {
//! # let mut demuxer = Demuxer::new("assets/mp4.mp4")?;
//! # let video_index: usize = todo!("视频流在输入容器中的下标");
//! # let (codecpar, time_base) = todo!("视频流的 codecpar 与 time_base");
//! let mut bsf = Bsf::new("h264_mp4toannexb", &codecpar, time_base)?;
//!
//! // 逐包过滤：一个输入包可能产出零或多个输出包
//! # let mut packet = todo!("demuxer.demux_packet() 的视频包");
//! for out in bsf.filter_packet(&mut packet)? {
//!     let _ = out; // 交给 Muxer::mux_packet
//! }
//!
//! // EOF 冲刷剩余输出；此后输出流参数以 `bsf.par_out()` 为准（avcC 已移除）
//! for mut out in bsf.flush_packets()? {
//!     let _ = out;
//! #   let _ = demuxer.nb_streams();
//! }
//! # Ok(())
//! # }
//! ```

use crate::error::{Result, RsmediaError};
use rsmpeg::avcodec::{
    AVBSFContext, AVBSFContextUninit, AVBitStreamFilter, AVCodecParameters, AVPacket,
};
use rsmpeg::error::RsmpegError;
use rsmpeg::ffi;
use std::ffi::CString;

/// 已初始化、可直接收发包的 bitstream filter 上下文。
///
/// 由 [`Bsf::new`] 创建；一个实例对应一路流的包通路。内部维护 FFmpeg 的
/// "send 后必须把 receive 抽干"的复用语义（见 [`Self::filter_packet`]）。
pub struct Bsf {
    inner: AVBSFContext,
}

impl std::fmt::Debug for Bsf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bsf").field("filter", &self.name()).finish()
    }
}

impl Bsf {
    /// 列出当前 FFmpeg 构建支持的全部 bitstream filter 名字。
    pub fn list() -> Vec<String> {
        AVBitStreamFilter::iterate()
            .map(|f| f.name().to_string_lossy().into_owned())
            .collect()
    }

    /// 创建并初始化一个 bitstream filter。
    ///
    /// # Arguments
    ///
    /// * `name` - filter 名（如 `h264_mp4toannexb`，完整列表见 [`Bsf::list`]）
    /// * `codecpar` - 输入流的 codec 参数（决定 filter 是否适用该 codec）
    /// * `time_base` - 输入包的时间基（写入 `AVBSFContext.time_base_in`）
    pub fn new(
        name: &str,
        codecpar: &AVCodecParameters,
        time_base: ffi::AVRational,
    ) -> Result<Self> {
        let name_c = CString::new(name)
            .map_err(|e| RsmediaError::invalid_config(format!("bsf name {name:?}: {e}")))?;
        let filter = AVBitStreamFilter::find_by_name(&name_c).ok_or_else(|| {
            RsmediaError::invalid_config(format!(
                "bitstream filter '{name}' not found in this FFmpeg build"
            ))
        })?;
        let mut ctx = AVBSFContextUninit::new(&filter);
        ctx.set_par_in(codecpar);
        ctx.set_time_base_in(time_base);
        let inner = ctx.init()?;
        Ok(Self { inner })
    }

    /// 本 filter 的名字（如 `h264_mp4toannexb`）。
    pub fn name(&self) -> String {
        self.inner.filter().name().to_string_lossy().into_owned()
    }

    /// 送入一个输入包。EOF 后（[`Self::flush_packets`] 之后）再送入会报错。
    ///
    /// 低层步骤：外部入口是 [`Self::filter_packet`]（送入并抽干，一次完成）。
    fn send_packet(&mut self, packet: &mut AVPacket) -> Result<()> {
        match self.inner.send_packet(Some(packet)) {
            Ok(()) => Ok(()),
            Err(RsmpegError::BitstreamFullError) => Err(RsmediaError::msg(
                "bitstream filter is full: drain received packets first",
            )),
            Err(e) => Err(e.into()),
        }
    }

    /// 发送 EOF 并抽出所有剩余输出包（每个输入流的通路结束时调用一次）。
    pub fn flush_packets(&mut self) -> Result<Vec<AVPacket>> {
        match self.inner.send_packet(None) {
            Ok(()) => {
                let mut holder = AVPacket::new();
                self.drain(&mut holder)
            }
            Err(e) => Err(e.into()),
        }
    }

    /// 便捷入口：送入一个包并抽干全部输出包（顺序与输入一致）。
    ///
    /// 一个输入包可能产出零或多个输出包；时间戳由 filter 保持不变。
    /// 通路结束时改用 [`Self::flush_packets`]。
    pub fn filter_packet(&mut self, packet: &mut AVPacket) -> Result<Vec<AVPacket>> {
        self.send_packet(packet)?;
        self.drain(packet)
    }

    /// 过滤后输出流的 codec 参数（以 `avcC` → Annex B 为例：extradata 被移除）。
    ///
    /// 转封装时应以它替换输出流的 codecpar，目标 muxer 才能写出正确的容器头。
    pub fn par_out(&self) -> AVCodecParameters {
        let mut out = AVCodecParameters::new();
        out.copy(&self.inner.par_out());
        out
    }

    /// 抽干当前可得的输出包。
    ///
    /// FFmpeg 依赖"receive 复用被 send 的那个包"，因此这里在收到每个输出后
    /// 通过 `av_packet_ref` 立即产出一份引用拷贝（共享 refcounted 缓冲，
    /// 零拷贝），再继续 receive。
    fn drain(&mut self, holder: &mut AVPacket) -> Result<Vec<AVPacket>> {
        let mut out = Vec::new();
        loop {
            match self.inner.receive_packet(holder) {
                Ok(()) => {
                    let mut owned = AVPacket::new();
                    // SAFETY: `holder` was filled in by `receive_packet` above.
                    // `av_packet_ref` can still fail (OOM); ignoring that would
                    // push an empty packet downstream as if it were real data.
                    let ret = unsafe { ffi::av_packet_ref(owned.as_mut_ptr(), holder.as_ptr()) };
                    if ret < 0 {
                        return Err(RsmediaError::msg(format!(
                            "av_packet_ref failed while draining the bitstream filter: {ret}"
                        )));
                    }
                    out.push(owned);
                }
                // 需要更多输入
                Err(RsmpegError::BitstreamDrainError) => return Ok(out),
                // 已无更多输出
                Err(RsmpegError::BitstreamFlushedError) => return Ok(out),
                Err(e) => return Err(e.into()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::StreamReader;
    use crate::mux::{Demuxer, Muxer};
    use crate::{DecoderBuilder, MediaType};

    /// MP4 → TS 纯复制（不重编码）：视频包经 `h264_mp4toannexb` 转为 Annex B
    /// 后与音频一起写入 TS；读回 TS 断言视频流可解码（证明 SPS/PPS 内联正确），
    /// 且帧数与源一致。
    #[test]
    fn test_bsf_mp4_to_ts_copy() -> Result<()> {
        let out = crate::test_support::test_output_path("bsf", "mp4_to_ts.ts");
        crate::test_support::remove_test_output(&out);

        let mut demuxer = Demuxer::new("assets/mp4.mp4")?;
        let nb = demuxer.nb_streams();
        let infos: Vec<_> = (0..nb).map(|i| demuxer.stream_info(i).unwrap()).collect();
        let v_in = infos
            .iter()
            .position(|i| i.media_type == MediaType::VIDEO)
            .expect("mp4 asset has a video stream");
        let a_in = infos.iter().position(|i| i.media_type == MediaType::AUDIO);

        let mut muxer = Muxer::new(&out)?;

        // 视频流：经 bsf 转换，输出流参数以 par_out 为准（avcC 已被移除）
        let mut vinfo = infos[v_in].clone();
        let mut bsf = Bsf::new("h264_mp4toannexb", &vinfo.codec_parameters, vinfo.time_base)?;
        vinfo.codec_parameters = bsf.par_out();
        let v_idx = muxer.add_copy_stream(&vinfo)?;
        // 音频流：MP4 的 raw AAC 由 mpegts muxer 自行补 ADTS，无需 bsf
        let a_idx = a_in.map(|i| muxer.add_copy_stream(&infos[i]).unwrap());

        let mut video_packets = 0usize;
        let mut filtered = 0usize;
        while let Some((idx, mut pkt)) = demuxer.demux_packet()? {
            if idx == v_in {
                video_packets += 1;
                for mut out in bsf.filter_packet(&mut pkt)? {
                    filtered += 1;
                    muxer.mux_packet(&mut out, v_idx)?;
                }
            } else if Some(idx) == a_in {
                muxer.mux_packet(&mut pkt, a_idx.unwrap())?;
            }
        }
        for mut out in bsf.flush_packets()? {
            filtered += 1;
            muxer.mux_packet(&mut out, v_idx)?;
        }
        muxer.finish()?;
        assert_eq!(video_packets, filtered, "annexb 转换应 1:1 产出包");

        // 读回 TS：视频流必须可解码——Annex B 的 SPS/PPS 内联是解码前提
        let mut reader = StreamReader::new(&out)?;
        let mut decoder = DecoderBuilder::new(MediaType::VIDEO).build_from_reader(&reader)?;
        let mut frames = 0usize;
        while decoder.decode_raw(&mut reader)?.is_some() {
            frames += 1;
        }
        drop(reader);
        assert_eq!(frames, video_packets, "TS 中解码出的帧数应与源视频包数一致");

        crate::test_support::remove_test_output(&out);
        Ok(())
    }

    /// list() 应包含常用 filter。
    #[test]
    fn test_bsf_list_contains_common_filters() {
        let list = Bsf::list();
        assert!(list.contains(&"h264_mp4toannexb".to_string()), "{list:?}");
        assert!(list.contains(&"aac_adtstoasc".to_string()), "{list:?}");
    }

    /// 不存在的 filter 名报 invalid_config，而不是 panic。
    #[test]
    fn test_bsf_unknown_name_rejected() {
        let codecpar = AVCodecParameters::new();
        let err = Bsf::new(
            "no_such_bsf",
            &codecpar,
            ffi::AVRational { num: 1, den: 30 },
        )
        .expect_err("unknown filter must be rejected");
        assert!(err.is_invalid_config(), "{err}");
    }
}
