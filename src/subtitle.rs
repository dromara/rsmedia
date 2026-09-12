//! Subtitle stream handling: passthrough and encoding helpers.
//!
//! Subtitle encoding is provided by the generic [`crate::encode::Encoder`]
//! (see [`EncoderBuilder::new_subtitle`](crate::encode::EncoderBuilder::new_subtitle)),
//! which uses the standard `avcodec_open2` + `avcodec_encode_subtitle` path
//! with an auto-generated ASS script header. This module provides:
//!
//! * [`copy_subtitle_stream`] — passthrough: copy subtitle packets from a
//!   reader to a writer without decode/encode (same codec, e.g. subrip→subrip).
//! * [`SubtitleSegment`] — a plain text segment used as the input to the
//!   generic encoder's `encode_subtitle_segment`.
//!
//! # Note on subtitle encoders
//!
//! Subtitle encoders (mov_text, subrip) require `subtitle_header` (an ASS
//! `[Script Info]`/`[V4+ Styles]` header) to be set before `avcodec_open2`,
//! otherwise init fails with `AVERROR_INVALIDDATA`. The generic encoder
//! generates a default header automatically.
//!
//! FFmpeg Documentation: <https://ffmpeg.org/doxygen/trunk/group__lavc__subtitle.html>

use crate::error::Result;
use crate::io::{Reader, Writer};

use rsmpeg::avcodec::AVSubtitle;
use rsmpeg::ffi;

use std::ffi::CStr;

/// Copy subtitle packets from a reader stream to a writer stream (passthrough).
///
/// Reads packets from `reader`, filters for `src_index`, rescales timestamps
/// to the output stream's time_base, and writes them to `writer` at
/// `out_index`. No decode/encode — the codec is preserved as-is.
///
/// Returns the number of packets copied.
pub fn copy_subtitle_stream<R: Reader, W: Writer>(
    reader: &mut R,
    writer: &mut W,
    src_index: usize,
    out_index: usize,
) -> Result<usize> {
    let src_tb = reader
        .input()
        .streams()
        .get(src_index)
        .map(|s| s.time_base)
        .unwrap_or(ffi::AVRational { num: 1, den: 1000 });

    let out_tb = writer
        .output()
        .streams()
        .get(out_index)
        .map(|s| s.time_base)
        .unwrap_or(src_tb);

    let mut count = 0usize;
    while let Some((stream_index, mut packet)) = reader.read_packet()? {
        if stream_index != src_index {
            continue;
        }
        packet.rescale_ts(src_tb, out_tb);
        packet.set_stream_index(out_index as i32);
        packet.set_pos(-1);
        writer.write_interleaved(&mut packet)?;
        count += 1;
    }
    Ok(count)
}

/// Encode a list of subtitle segments through a bare [`Encoder`] into the
/// container written by `writer` (see [`crate::io::StreamWriter`]).
///
/// 字幕编码走同步 API（`encode_subtitle_segment`，无 send/receive 缓冲），
/// 不能复用 [`crate::mux::Muxer::mux`]（其内部调用 `encoder.encode_raw`
/// 仅面向音视频编码流）。此处直接：
///   1. 向 writer 添加字幕流（编码器 codec par + time_base）；
///   2. 写 header；
///   3. 逐段编码并把包 pts 从编码器 time_base 换算到输出流 time_base 后写盘；
///   4. flush 编码器（字幕路径为 no-op）+ 写 trailer。
pub fn encode_subtitle_segments(
    writer: &mut impl Writer,
    encoder: &mut crate::encode::Encoder,
    segments: &[SubtitleSegment],
) -> Result<()> {
    let enc_tb = encoder.time_base();
    let index = writer.add_stream(encoder.codecpar(), enc_tb);
    writer.write_header()?;
    let out_tb = writer.stream_time_base(index);

    for segment in segments {
        for mut packet in encoder.encode_subtitle_segment(segment)? {
            packet.set_stream_index(index as i32);
            packet.set_pos(-1);
            packet.rescale_ts(enc_tb, out_tb);
            writer.write_interleaved(&mut packet)?;
        }
    }

    // 字幕编码器同步路径无内部缓冲，flush 为 no-op（置为 Flushed），但仍须
    // 写 trailer 以封闭容器。
    encoder.flush(writer, false, index, out_tb)?;
    writer.write_trailer()?;
    Ok(())
}

/// A single subtitle text segment with start/end times.
#[derive(Debug, Clone)]
pub struct SubtitleSegment {
    /// Start time in milliseconds.
    pub start_ms: i64,
    /// End time in milliseconds.
    pub end_ms: i64,
    /// Subtitle text (plain UTF-8 text).
    pub text: String,
}

impl SubtitleSegment {
    /// Create a new subtitle segment.
    pub fn new(start_ms: i64, end_ms: i64, text: impl Into<String>) -> Self {
        Self {
            start_ms,
            end_ms,
            text: text.into(),
        }
    }

    /// Duration in milliseconds.
    pub fn duration_ms(&self) -> i64 {
        self.end_ms - self.start_ms
    }

    /// 从解码得到的 [`AVSubtitle`] 提取纯文本段落。
    ///
    /// 时间戳语义（FFmpeg 文档）：`AVSubtitle.pts` 为 packet pts 换算成的
    /// `AV_TIME_BASE`（微秒）；`start/end_display_time` 为相对 pts 的毫秒偏移：
    /// `start_ms = pts/1000 + start_display_time`，`end_ms` 同理。
    ///
    /// 文本提取：优先 `SUBTITLE_ASS` rect（取 Dialogue 行第 10 个字段，即
    /// 第 9 个逗号之后的部分，并将 ASS 硬换行 `\N` 转为换行符），其次
    /// `SUBTITLE_TEXT` rect（纯文本）。`SUBTITLE_BITMAP`（如 DVB 字幕）无法
    /// 表示为文本，跳过该 rect。
    ///
    /// 与 [`crate::encode::Encoder::encode_subtitle_segment`] 对称：编码时把
    /// 纯文本放进 Dialogue 行第 10 字段，此处逆变换还原。
    ///
    /// 返回 [`None`] 表示该 subtitle 无文本 rect（如位图字幕或空段落）。
    pub fn from_avsubtitle(subtitle: &AVSubtitle) -> Option<Self> {
        if subtitle.num_rects() == 0 {
            return None;
        }

        let pts_ms = subtitle.pts / 1000;
        let start_ms = pts_ms + subtitle.start_display_time as i64;
        let end_ms = pts_ms + subtitle.end_display_time as i64;

        let mut texts: Vec<String> = Vec::new();
        for rect in subtitle.rect_iter() {
            match rect.type_() {
                ffi::SUBTITLE_ASS => {
                    if let Some(ass) = rect.ass()
                        && let Some(text) = ass_dialogue_text(ass)
                    {
                        texts.push(text);
                    }
                }
                ffi::SUBTITLE_TEXT => {
                    if let Some(text) = rect.text() {
                        texts.push(text.to_string_lossy().into_owned());
                    }
                }
                ty => {
                    // SUBTITLE_BITMAP / SUBTITLE_NONE：无法表示为文本
                    log::debug!("Skip non-text subtitle rect type: {ty}");
                }
            }
        }
        if texts.is_empty() {
            return None;
        }
        Some(Self {
            start_ms,
            end_ms,
            text: texts.join("\n"),
        })
    }
}

/// Extracts the text payload from an ASS rect string.
///
/// Two shapes occur in practice, distinguished by the optional prefix:
///
/// - **Full `Dialogue:` line** (`Dialogue: layer,start,end,style,name,ML,MR,MV,effect,text`,
///   produced by ASS/SRT decoders in transcode pipelines) — text starts after
///   the 9th comma.
/// - **`AVCodecContext`-internal shape** (`readorder,layer,style,name,ML,MR,MV,effect,text`,
///   produced by the mov_text decoder) — no prefix and no timestamps, so text
///   starts after the 8th comma.
///
/// The text itself may contain commas, so only the fixed leading fields are
/// split. ASS hard/soft line breaks (`\N` / `\n`) are normalised to newlines.
fn ass_dialogue_text(ass: &CStr) -> Option<String> {
    let line = ass.to_str().ok()?;
    let fields_before_text = if line.starts_with("Dialogue:") { 9 } else { 8 };
    let mut commas = 0usize;
    let mut text_start = None;
    for (i, ch) in line.char_indices() {
        if ch == ',' {
            commas += 1;
            if commas == fields_before_text {
                text_start = Some(i + 1);
                break;
            }
        }
    }
    let text = &line[text_start?..];
    if text.is_empty() {
        return None;
    }
    // ASS 硬换行 `\N` 与软换行 `\n` 统一转换为换行符
    Some(text.replace("\\N", "\n").replace("\\n", "\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode::EncoderBuilder;
    use crate::error::RsmediaError;
    use crate::io::StreamReader;
    use crate::test_support;
    use crate::time::Rescale;

    /// 测试用完整 ASS 脚本头：字幕编码器（ass/subrip/mov_text）open 时以此
    /// 初始化样式解析（ff_ass_split），缺失或残缺会导致 open 失败或
    /// Dialogue 字段错位。真实转码流程应从解码侧传递 header。
    /// `subtitle_header` 必须包含完整的 `[Script Info]` 和 `[V4+ Styles]` 段
    const ASS_HEADER: &str = "[Script Info]\n\
         ScriptType: v4.00+\n\
         \n\
         [V4+ Styles]\n\
         Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, \
         OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, \
         ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, \
         Alignment, MarginL, MarginR, MarginV, Encoding\n\
         Style: Default,Arial,16,&Hffffff,&Hffffff,&H0,&H0,0,0,0,0,100,100,0,0,1,1,0,2,10,10,10,1\n\
         \n\
         [Events]\n\
         Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n";

    /// Create a few sample subtitle segments for testing.
    fn sample_segments() -> Vec<SubtitleSegment> {
        vec![
            SubtitleSegment::new(0, 2000, "Hello World"),
            SubtitleSegment::new(2000, 4000, "Second subtitle"),
            SubtitleSegment::new(4000, 6000, "Third subtitle"),
        ]
    }

    /// 通用 Decoder 字幕解码全链路验证：编码（mov_text）→ mux → demux →
    /// [`Decoder::decode_subtitle_segment`] 解码 → 还原 [`SubtitleSegment`]。
    ///
    /// 严格验证：段落数、文本内容、start_ms 与编码输入一致（时间戳语义：
    /// AVSubtitle.pts 为 AV_TIME_BASE 微秒，start_display_time 相对 pts 毫秒）。
    #[test]
    fn test_subtitle_decode_roundtrip() -> Result<()> {
        use crate::MediaType;
        use crate::decode::DecoderBuilder;

        let path = test_support::test_output_path("subtitle", "rsmedia_decode_rt.mp4");
        test_support::remove_test_output(&path);

        // 1) Encode: mov_text segments into an MP4
        let segments = sample_segments();
        let mut encoder = EncoderBuilder::new_subtitle()
            .with_codec_name(Some("mov_text".to_string()))
            .with_subtitle_header(ASS_HEADER)
            .build()?;
        let mut writer = crate::io::StreamWriter::new(&path)?;
        encode_subtitle_segments(&mut writer, &mut encoder, &segments)?;

        // 2) Decode: via the generic Decoder subtitle channel
        let mut reader = crate::io::StreamReader::new(&path)?;
        let mut decoder = DecoderBuilder::new(MediaType::SUBTITLE)
            .with_codec_name(Some("mov_text".to_string()))
            .build_from_reader(&reader)?;
        let mut decoded: Vec<SubtitleSegment> = Vec::new();
        while let Some(segment) = decoder.decode_subtitle_segment(&mut reader)? {
            decoded.push(segment);
        }

        // 3) Verify roundtrip
        assert_eq!(decoded.len(), segments.len(), "decoded: {decoded:?}");
        for (got, want) in decoded.iter().zip(segments.iter()) {
            assert_eq!(got.text, want.text, "text mismatch");
            assert_eq!(got.start_ms, want.start_ms, "start_ms mismatch");
            assert_eq!(got.end_ms, want.end_ms, "end_ms mismatch");
        }

        // 4) Flushed 之后继续解码应报错（与音视频路径语义一致）
        assert!(decoder.decode_subtitle_segment(&mut reader).is_err());

        Ok(())
    }

    /// `SubtitleSegment::from_avsubtitle` 的单元测试：ASS rect 文本提取
    /// （含逗号的正文不能被字段切分截断）、`\N` 硬换行转换、时间戳换算。
    #[test]
    fn test_from_avsubtitle_ass_rect() -> Result<()> {
        use rsmpeg::avcodec::AVSubtitle;

        let mut sub = AVSubtitle::new();
        // 正文 "Hello, world, with, commas" 含 4 个逗号，若按逗号无脑切分会截断
        sub.push_ass_rect(
            c"Dialogue: 0,0:00:01.00,0:00:03.50,Default,,0,0,0,,Hello, world, with, commas",
        )?;

        let seg =
            SubtitleSegment::from_avsubtitle(&sub).expect("ASS rect should convert to a segment");
        assert_eq!(seg.text, "Hello, world, with, commas");
        // Dialogue 行内的时间戳不参与换算：展示时间来自 pts + display_time
        // （此处 AVSubtitle::new 的 pts 为 0，仅验证 display_time 路径）
        assert_eq!(seg.start_ms, 0);
        assert_eq!(seg.end_ms, 0);

        // \N 硬换行 → 换行符
        let mut sub2 = AVSubtitle::new();
        sub2.push_ass_rect(
            c"Dialogue: 0,0:00:00.00,0:00:02.00,Default,,0,0,0,,line one\\Nline two",
        )?;
        let seg2 = SubtitleSegment::from_avsubtitle(&sub2).expect("should convert");
        assert_eq!(seg2.text, "line one\nline two");

        // 无文本 rect（如空 subtitle）→ None
        let empty = AVSubtitle::new();
        assert!(SubtitleSegment::from_avsubtitle(&empty).is_none());

        Ok(())
    }

    /// Encode subtitles as mov_text into an MP4 file via the generic Encoder,
    /// then read back and **strictly verify** via decode roundtrip:
    /// 1. subtitle stream exists with codec id `AV_CODEC_ID_MOV_TEXT`;
    /// 2. every segment survives encode → mux → demux → decode (text intact);
    /// 3. packet pts, rescaled from the output stream time_base to the
    ///    encoder's 1/1000 time base, equals the segment start time (ms).
    #[test]
    fn test_mov_text_encode_and_readback() -> Result<()> {
        use rsmpeg::avcodec::{AVCodec, AVCodecContext};

        let path = test_support::test_output_path("subtitle", "rsmedia_mov_text.mp4");
        test_support::remove_test_output(&path);

        // 1) Write: create an MP4 with a mov_text subtitle stream
        let segments = sample_segments();
        let mut encoder = EncoderBuilder::new_subtitle()
            .with_codec_name(Some("mov_text".to_string()))
            .with_subtitle_header(ASS_HEADER)
            .build()?;
        let mut writer = crate::io::StreamWriter::new(&path)?;
        encode_subtitle_segments(&mut writer, &mut encoder, &segments)?;

        // 2) Read back: verify the subtitle stream exists
        let mut reader = StreamReader::new(&path)?;
        let (index, _) = reader.find_best_stream(crate::MediaType::SUBTITLE)?;
        let (codec_id, stream_tb) = {
            let stream = reader.input().streams().get(index).unwrap();
            (stream.codecpar().codec_id, stream.time_base)
        };
        assert_eq!(
            codec_id,
            ffi::AV_CODEC_ID_MOV_TEXT,
            "subtitle codec should be mov_text, got codec_id={codec_id}"
        );

        // 3) Decode roundtrip: demux packets -> decode_subtitle -> rect payload
        let decoder = AVCodec::find_decoder(codec_id)
            .ok_or_else(|| RsmediaError::custom("mov_text decoder not available"))?;
        let mut dctx = AVCodecContext::new(&decoder);
        dctx.open(None)?;

        let mut texts: Vec<String> = Vec::new();
        let mut start_ms: Vec<i64> = Vec::new();
        while let Some((stream_index, mut packet)) = reader.read_packet()? {
            if stream_index != index {
                continue;
            }
            // pts (stream time_base) -> milliseconds (encoder time base 1/1000)
            start_ms.push(
                packet
                    .pts
                    .rescale(stream_tb, crate::time::new_rational(1, 1000)),
            );
            if let Some(subtitle) = dctx.decode_subtitle(Some(&mut packet))? {
                for rect in subtitle.rect_iter() {
                    if let Some(ass) = rect.ass() {
                        texts.push(ass.to_string_lossy().to_string());
                    }
                }
            }
        }

        assert_eq!(
            start_ms,
            vec![0, 2000, 4000],
            "packet pts must round-trip to the segment start times"
        );
        let all = texts.join("\n");
        assert!(all.contains("Hello World"), "decoded text: {all:?}");
        assert!(all.contains("Second subtitle"), "decoded text: {all:?}");
        assert!(all.contains("Third subtitle"), "decoded text: {all:?}");

        test_support::remove_test_output(&path);
        Ok(())
    }

    /// Encode subtitles as subrip into a .srt file via the generic Encoder,
    /// and **strictly verify** the complete SRT structure: serial numbers,
    /// timestamp lines (`HH:MM:SS,mmm --> HH:MM:SS,mmm`, generated end-to-end
    /// from packet pts/duration) and payload text.
    #[test]
    fn test_subrip_encode() -> Result<()> {
        let path = test_support::test_output_path("subtitle", "rsmedia_subrip.srt");
        test_support::remove_test_output(&path);

        let segments = sample_segments();
        let mut encoder = EncoderBuilder::new_subtitle()
            .with_subtitle_header(ASS_HEADER)
            .build()?;
        let mut writer = crate::io::StreamWriter::new(&path)?;
        encode_subtitle_segments(&mut writer, &mut encoder, &segments)?;

        let content = std::fs::read_to_string(&path)?;
        // 时间戳行：由 packet pts/duration（毫秒）端到端生成，验证整条时间戳链路
        assert!(
            content.contains("00:00:00,000 --> 00:00:02,000"),
            "missing first timestamp line, content:\n{content}"
        );
        assert!(
            content.contains("00:00:02,000 --> 00:00:04,000"),
            "missing second timestamp line, content:\n{content}"
        );
        assert!(
            content.contains("00:00:04,000 --> 00:00:06,000"),
            "missing third timestamp line, content:\n{content}"
        );
        // 文本行
        assert!(content.contains("Hello World"), "content:\n{content}");
        assert!(content.contains("Second subtitle"), "content:\n{content}");
        assert!(content.contains("Third subtitle"), "content:\n{content}");
        // 序号行（SRT 序号 1..N）
        for serial in ["1", "2", "3"] {
            assert!(
                content.lines().any(|l| l.trim() == serial),
                "missing SRT serial {serial}, content:\n{content}"
            );
        }

        test_support::remove_test_output(&path);
        Ok(())
    }

    /// Passthrough test: encode subrip → MKV via the generic Encoder, then
    /// copy the subtitle stream to another MKV.
    #[test]
    fn test_subtitle_passthrough() -> Result<()> {
        // 1) Create an MKV with subrip subtitles
        let input_path = test_support::test_output_path("subtitle", "rsmedia_passthrough_in.mkv");
        test_support::remove_test_output(&input_path);

        let segments = sample_segments();
        let mut encoder = EncoderBuilder::new_subtitle()
            .with_subtitle_header(ASS_HEADER)
            .build()?;
        let mut writer = crate::io::StreamWriter::new(&input_path)?;
        encode_subtitle_segments(&mut writer, &mut encoder, &segments)?;

        // 2) Read the MKV and copy the subtitle stream to another MKV
        let output_path = test_support::test_output_path("subtitle", "rsmedia_passthrough_out.mkv");
        test_support::remove_test_output(&output_path);

        let mut reader = StreamReader::new(&input_path)?;
        let mut out_writer = crate::io::StreamWriter::new(&output_path)?;

        // Find subtitle stream in input
        let (src_index, _) = reader.find_best_stream(crate::MediaType::SUBTITLE)?;

        // Copy codec parameters to output stream (clone to release the borrow
        // on reader before calling copy_subtitle_stream which needs &mut reader).
        let (codecpar, src_tb) = {
            let src_stream = reader.input().streams().get(src_index).unwrap();
            (src_stream.codecpar().clone(), src_stream.time_base)
        };
        let out_index = out_writer.add_stream(codecpar, src_tb);

        // Write header, copy packets, write trailer
        out_writer.write_header()?;
        let count = copy_subtitle_stream(&mut reader, &mut out_writer, src_index, out_index)?;
        out_writer.write_trailer()?;

        assert_eq!(count, segments.len(), "should copy all subtitle packets");

        // 3) Verify output has the subtitle stream with correct codec
        let out_reader = StreamReader::new(&output_path)?;
        let subtitle_streams: Vec<_> = out_reader
            .input()
            .streams()
            .iter()
            .filter(|s| s.codecpar().codec_type().is_subtitle())
            .collect();
        assert!(
            !subtitle_streams.is_empty(),
            "output should have subtitle stream"
        );

        test_support::remove_test_output(&input_path);
        test_support::remove_test_output(&output_path);
        Ok(())
    }

    /// Encode subtitles with the **ass** encoder into an MKV
    /// (S_TEXT/ASS), then strictly verify via decode roundtrip, mirroring the
    /// mov_text test: codec id, text payload, and pts round-trip.
    #[test]
    fn test_ass_encode_and_readback() -> Result<()> {
        use rsmpeg::avcodec::{AVCodec, AVCodecContext};

        let path = test_support::test_output_path("subtitle", "rsmedia_ass.mkv");
        test_support::remove_test_output(&path);

        // 1) Write: create an MKV with an ASS subtitle stream
        let segments = sample_segments();
        let mut encoder = EncoderBuilder::new_subtitle()
            .with_codec_name(Some("ass".to_string()))
            .with_subtitle_header(ASS_HEADER)
            .build()?;
        let mut writer = crate::io::StreamWriter::new(&path)?;
        encode_subtitle_segments(&mut writer, &mut encoder, &segments)?;

        // 2) Read back: verify the subtitle stream exists
        let mut reader = StreamReader::new(&path)?;
        let (index, _) = reader.find_best_stream(crate::MediaType::SUBTITLE)?;
        let (codec_id, stream_tb) = {
            let stream = reader.input().streams().get(index).unwrap();
            (stream.codecpar().codec_id, stream.time_base)
        };
        assert_eq!(
            codec_id,
            ffi::AV_CODEC_ID_ASS,
            "subtitle codec should be ass, got codec_id={codec_id}"
        );

        // 3) Decode roundtrip: demux packets -> decode_subtitle -> rect payload
        let decoder = AVCodec::find_decoder(codec_id)
            .ok_or_else(|| RsmediaError::custom("ass decoder not available"))?;
        let mut dctx = AVCodecContext::new(&decoder);
        dctx.open(None)?;

        let mut texts: Vec<String> = Vec::new();
        let mut start_ms: Vec<i64> = Vec::new();
        while let Some((stream_index, mut packet)) = reader.read_packet()? {
            if stream_index != index {
                continue;
            }
            // pts (stream time_base) -> milliseconds (encoder time base 1/1000)
            start_ms.push(
                packet
                    .pts
                    .rescale(stream_tb, crate::time::new_rational(1, 1000)),
            );
            if let Some(subtitle) = dctx.decode_subtitle(Some(&mut packet))? {
                for rect in subtitle.rect_iter() {
                    if let Some(ass) = rect.ass() {
                        texts.push(ass.to_string_lossy().to_string());
                    }
                }
            }
        }

        assert_eq!(
            start_ms,
            vec![0, 2000, 4000],
            "packet pts must round-trip to the segment start times"
        );
        let all = texts.join("\n");
        assert!(all.contains("Hello World"), "decoded text: {all:?}");
        assert!(all.contains("Second subtitle"), "decoded text: {all:?}");
        assert!(all.contains("Third subtitle"), "decoded text: {all:?}");

        test_support::remove_test_output(&path);
        Ok(())
    }

    /// 编码器类型不匹配时应报错而非 panic。
    #[test]
    fn test_encode_subtitle_segment_wrong_media_type() -> Result<()> {
        let mut encoder = crate::encode::Encoder::new_video(64, 64)?;
        let err = encoder
            .encode_subtitle_segment(&SubtitleSegment::new(0, 1000, "x"))
            .unwrap_err();
        assert!(err.to_string().contains("subtitle encoder"));
        Ok(())
    }

    /// 未提供 ASS header 时 build() 应报明确错误（而非 avcodec_open2 的
    /// AVERROR_INVALIDDATA）。
    #[test]
    fn test_missing_subtitle_header_is_rejected() -> Result<()> {
        let err = match EncoderBuilder::new_subtitle().build() {
            Err(e) => e,
            Ok(_) => return Err(RsmediaError::custom("build should fail without header")),
        };
        assert!(err.to_string().contains("header"));
        Ok(())
    }
}
