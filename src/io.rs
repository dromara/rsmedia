use crate::error::{Context, Result, RsmediaError};
use crate::location::Location;
use crate::options::Options;
use crate::stream::MediaType;
use crate::{strutils, time};

use rsmpeg::avcodec::{AVCodecParameters, AVPacket};
use rsmpeg::avformat::{
    AVFormatContextInput, AVFormatContextOutput, AVIOContextContainer, AVIOContextCustom,
    AVInputFormat, ReadPacketCallback, SeekCallback, WritePacketCallback,
};
use rsmpeg::avutil::{AVDictionary, AVMem};
use rsmpeg::ffi;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// `AVERROR(EIO)`：FFmpeg 以负 errno 报错，即 `AVERROR(e) = -e`。
/// errno 真值由平台头文件提供，不手写数字。
const AVERROR_EIO: i32 = -libc::EIO;

/// avio 内部缓冲大小（读写回调模式的滚动窗口）。
const AVIO_BUFFER_SIZE: usize = 4096;

ffi_enum!(
    /// Flags for [`Seekable::seek_to_frame`] (FFmpeg `AVSEEK_FLAG_*`).
    ///
    /// Combinable: `AVSeekFlag::BACKWARD | AVSeekFlag::ANY` yields the raw `i32` mask that
    /// `seek_to_frame` accepts, since its parameter is `impl Into<i32>`.
    AVSeekFlag, i32 {
        BACKWARD => ffi::AVSEEK_FLAG_BACKWARD;
        BYTE => ffi::AVSEEK_FLAG_BYTE;
        ANY => ffi::AVSEEK_FLAG_ANY;
        FRAME => ffi::AVSEEK_FLAG_FRAME;
    }
);

pub trait Reader {
    fn input(&self) -> &AVFormatContextInput;
    fn input_mut(&mut self) -> &mut AVFormatContextInput;

    /// Read the next packet. Returns `None` on EOF.
    ///
    /// 成功时返回 `(packet 所属流的 index, packet)`；调用方如需更多流信息
    /// （time_base、metadata 等），可通过 `self.input().streams().get(index)`
    /// 直接使用 rsmpeg 的 [`AVStream`]。
    fn read_packet(&mut self) -> Result<Option<(usize, AVPacket)>> {
        match self.input_mut().read_packet() {
            Ok(Some(pkt)) => Ok(Some((pkt.stream_index as usize, pkt))),
            Ok(None) => Ok(None),
            Err(e) => Err(RsmediaError::from(e)),
        }
    }

    /// Find the best stream
    ///
    /// # Arguments
    ///
    /// * `media_type` - MediaType maybe Video, Audio, etc.
    fn find_best_stream(&self, media_type: MediaType) -> Result<(usize, String)> {
        self.input()
            .find_best_stream(media_type as _)?
            .map(|(index, codec)| (index, strutils::cstr_to_string_lossy(codec.name())))
            .ok_or(RsmediaError::custom(format!(
                "No stream found for MediaType:{media_type:?}"
            )))
    }
}

/// Random-access (seek) capability interface.
///
/// This trait expresses "**seek operations are available**", not "a seek is
/// guaranteed to succeed". Whether a seek works depends on the **runtime
/// source**, which is why every method returns [`Result`] and reports the
/// underlying FFmpeg error on failure. A single [`StreamReader`] can read both a
/// local mp4 and an http/rtsp source, so both share this one API; the only
/// difference is whether a seek succeeds at runtime:
///
/// | Source | Seekable | Notes |
/// |--------|----------|-------|
/// | Local file (`file` protocol) | yes | regular files are always byte-seekable |
/// | In-memory buffer ([`BufferReader`]) | yes | a seek callback is installed, so seeking always works |
/// | HTTP(S) | server-dependent | seekable when the server returns `Accept-Ranges: bytes`; chunked/live responses usually are not |
/// | RTSP / RTP | server-dependent | depends on whether the server supports Range / PLAY jumps |
///
/// Use [`is_byte_seekable`](Seekable::is_byte_seekable) for a cheap pre-check
/// of the IO layer; container-level demuxer seek (e.g. RTSP) is opaque in
/// FFmpeg 5.0+ and cannot be probed, so treat the return value of each
/// `seek_*` call as authoritative.
///
/// # Example
///
/// ```no_run
/// use rsmedia::io::Seekable;
/// use rsmedia::StreamReader;
///
/// # fn main() -> rsmedia::error::Result<()> {
/// // Network source: a plain string literal is enough -- anything with a
/// // network scheme becomes a URL, everything else a filesystem path.
/// let mut reader = StreamReader::new("https://example.com/video.mp4")?;
/// if reader.is_byte_seekable() {
///     reader.seek_to_timestamp(10_000)?; // seek to the keyframe near 10s
/// }
/// # Ok(())
/// # }
/// ```
pub trait Seekable: Reader {
    /// Whether the underlying IO is byte-addressable, i.e. FFmpeg's
    /// `AVIOContext::seekable` has the `AVIO_SEEKABLE_NORMAL` bit set.
    ///
    /// - Local files and in-memory buffers ([`BufferReader`]): `true`.
    /// - HTTP(S): `true` when the server returns `Accept-Ranges: bytes`.
    /// - Live streams / pipes / most RTSP: `false`.
    ///
    /// This is the strongest **cheaply detectable** signal for "will a seek
    /// work" (e.g. whether to enable a draggable progress bar without doing an
    /// actual seek, which resets decoders and drops buffered data), but it is
    /// still **neither necessary nor sufficient**:
    ///
    /// - Demuxer-level container seek is opaque since FFmpeg 5.0
    ///   (`AVInputFormat` no longer exposes `read_seek`), so e.g. RTSP reports
    ///   `false` yet may still seek by timestamp through its own `read_seek`.
    /// - Conversely `AVFMT_NO_BYTE_SEEK` (set even by mp4/mov) only forbids
    ///   `AVSEEK_FLAG_BYTE`, not timestamp seeks, so it is deliberately *not*
    ///   consulted here.
    ///
    /// Treat the return value of each `seek_*` call as the authority.
    fn is_byte_seekable(&self) -> bool {
        let pb = self.input().pb;
        unsafe { !pb.is_null() && ((*pb).seekable & ffi::AVIO_SEEKABLE_NORMAL as i32) != 0 }
    }

    /// Seek in reader. This will change the reader head so that it points to a location within one
    /// second of the target timestamp or it will return an error.
    ///
    /// # Arguments
    ///
    /// * `timestamp_ms` - Number of millisecond from start of video to seek to.
    fn seek_to_timestamp(&mut self, timestamp_ms: i64) -> Result<()> {
        // Conversion factor from timestamp in milliseconds to `TIME_BASE` units.
        const CONVERSION_FACTOR: i64 = (time::TIME_BASE.den / 1000) as i64;
        // One second left and right leeway when seeking.
        const LEEWAY: i64 = time::TIME_BASE.den as i64;
        let timestamp = CONVERSION_FACTOR * timestamp_ms;
        // 注意区间必须不对称（max 比 min 更贴近 ts）：`avformat_seek_file` 会
        // 忽略调用方的 BACKWARD 标志，并对不支持 read_seek2 的 demuxer 依据
        // `(ts - min) > (max - ts)` 推导回退方向（见 libavformat/seek.c）。
        // 若区间完全对称则恰好平局，会退化为 FORWARD seek——当 ts 之后没有
        // 关键帧时 seek 直接失败。这里让 min 侧留出 1μs 余量，保证始终
        // BACKWARD（定位到 ts 之前最近的关键帧，语义同 `av_seek_frame`）。
        seek_file(
            self.input_mut(),
            -1,
            timestamp - LEEWAY,
            timestamp,
            timestamp + LEEWAY - 1,
        )
        .context("Failed to seek timestamp in reader")?;
        Ok(())
    }

    /// Seek to start of reader. This function performs best effort seeking to the start of the
    /// file.
    fn seek_to_start(&mut self) -> Result<()> {
        // min=ts=INT64_MIN、max=INT64_MAX：目标即"最早可定位点"，
        // 强制 BACKWARD 回退到容器开头（best effort，不支持 seek 的源会报错）。
        seek_file(self.input_mut(), -1, i64::MIN, i64::MIN, i64::MAX)
            .context("Failed to seek to start of reader")?;
        Ok(())
    }

    /// Seek to a specific frame in the video stream.
    ///
    /// Wraps FFmpeg's `av_seek_frame`, which by default (without `AVSeekFlag::BYTE`
    /// or `AVSeekFlag::FRAME`) treats `frame_ts` as a **timestamp in stream
    /// time-base units**.
    ///
    /// # Frame-based seeking (`AVSeekFlag::FRAME`)
    ///
    /// Passing `AVSeekFlag::FRAME` asks for **frame-precise** seeking: `frame_ts`
    /// is interpreted as a frame **index** (not a timestamp). Support is entirely
    /// **source-dependent** and most containers cannot honour it:
    ///
    /// - **Not supported** by common containers like **MP4/MOV/MKV/AVI** (their
    ///   demuxers index by timestamp, not by frame), nor by network/live sources.
    ///   For these, `seek_to_frame(.., N, AVSeekFlag::FRAME)` returns `Err` with a
    ///   negative FFmpeg code — even though [`Seekable::seek_to_timestamp`] works fine.
    /// - **Supported** by a handful of raw / low-level demuxers such as **`.h264` /
    ///   `.hevc` / raw PCM**, which are the intended consumers of frame-index seeking.
    ///
    /// So treat this call as **fallible** (propagate the `Result`, don't `.unwrap()`)
    /// and prefer [`Seekable::seek_to_timestamp`] for container formats. Note also that
    /// seeking is best-effort: it lands on the nearest **keyframe** unless
    /// `AVSeekFlag::ANY` is combined.
    ///
    /// # Arguments
    ///
    /// * `stream_index` - The index of the stream to seek to.
    /// * `frame_ts` - The timestamp of the target frame, or a frame **index** when
    ///   `AVSeekFlag::FRAME` is set. In the timestamp case this is typically derived
    ///   from the frame's presentation timestamp (PTS) in the stream's time base.
    /// * `flags` - [`AVSeekFlag`] bit flags, combinable with `|`, e.g.
    ///   `AVSeekFlag::BACKWARD | AVSeekFlag::ANY` (or mixed with a raw mask:
    ///   `AVSeekFlag::ANY` `| 2`); [`AVSeekFlag::FRAME`] seeks by frame index,
    ///   [`AVSeekFlag::BYTE`] seeks by byte position, and [`AVSeekFlag::ANY`]
    ///   allows landing on a non-keyframe.
    fn seek_to_frame(
        &mut self,
        stream_index: usize,
        frame_ts: i64,
        flags: AVSeekFlag,
    ) -> Result<()> {
        unsafe {
            let res = ffi::av_seek_frame(
                self.input_mut().as_mut_ptr(),
                stream_index as i32,
                frame_ts,
                flags.into(),
            );
            if res < 0 {
                return Err(RsmediaError::custom(format!(
                    "Seek to frame failed: stream={stream_index}, ts={frame_ts}, flags={:?}, err={res}",
                    flags
                )));
            }
            Ok(())
        }
    }
}

/// Thin wrapper over `avformat_seek_file`: `min`/`ts`/`max` bound the target interval.
///
/// # Arguments
///
/// * `input` - The input context.
/// * `stream_index` - Target stream index; `-1` means timestamps are in `AV_TIME_BASE` units.
/// * `min` / `ts` / `max` - Target interval, inclusive on both ends.
///
/// Always called with `flags = 0`: `avformat_seek_file` ignores
/// `AVSEEK_FLAG_BACKWARD` and derives the seek direction from the **asymmetry**
/// of the interval instead (see [`Seekable::seek_to_timestamp`]). Use
/// [`Seekable::seek_to_frame`] when `AVSeekFlag` values such as `FRAME` / `BYTE`
/// are needed.
fn seek_file(
    input: &mut AVFormatContextInput,
    stream_index: i32,
    min: i64,
    ts: i64,
    max: i64,
) -> Result<()> {
    let res = unsafe { ffi::avformat_seek_file(input.as_mut_ptr(), stream_index, min, ts, max, 0) };
    if res < 0 {
        // >=0 on success, error code otherwise
        return Err(RsmediaError::custom(format!("Seek file failed: {res}")));
    }
    Ok(())
}

/// 内存 seek 共享逻辑：基于 `pos` 与数据总长 `len` 计算 whence 语义。
///
/// 返回定位后的偏移；`AVSEEK_SIZE` 时返回数据总长。
fn memory_seek(pos: &AtomicUsize, len: usize, offset: i64, whence: i32) -> i64 {
    let len = len as i64;
    if whence & (ffi::AVSEEK_SIZE as i32) != 0 {
        return len;
    }
    // 去掉 AVSEEK_FORCE 位后按 POSIX whence 解释（常量取自 libc，禁止手写）
    let base = whence & !(ffi::AVSEEK_FORCE as i32);
    let target = match base {
        libc::SEEK_SET => offset,
        libc::SEEK_CUR => pos.load(Ordering::Relaxed) as i64 + offset,
        libc::SEEK_END => len + offset,
        _ => return -1,
    };
    let target = target.clamp(0, len);
    pos.store(target as usize, Ordering::Relaxed);
    target
}

////////////////////////////////////////
// 自定义 AVIO 上下文装配（Reader/Writer 共享）
////////////////////////////////////////

/// 用自定义 AVIO 打开输入上下文并完成格式探测（内部已含
/// `avformat_open_input` + `avformat_find_stream_info`）。
///
/// 文件/URL 输入不走这里：应使用 `AVFormatContextInput::builder().url()`，
/// 由 FFmpeg 协议层（file/http/rtmp...）处理。本函数只服务于
/// `BufferReader`、`IoReader` 等非路径源。
///
/// 返回的 context 已安装 `interrupt`（若有）；调用方须保证 `interrupt`
/// 存活至 context drop。
fn open_input_custom(
    io_context: AVIOContextCustom,
    format: Option<&str>,
    options: Option<Options>,
    interrupt: Option<&Interrupt>,
    dump_name: &std::ffi::CStr,
) -> Result<AVFormatContextInput> {
    let fmt_opt = format.and_then(|name| AVInputFormat::find(&strutils::str_to_cstring(name)));
    let mut dict = options.and_then(|opts| opts.into_dict());
    let mut ctx = AVFormatContextInput::builder()
        .maybe_format(fmt_opt.as_deref())
        .options(&mut dict)
        .io_context(AVIOContextContainer::Custom(io_context))
        .open()
        .context("Create input format context with custom IO failed.")?;
    if let Some(interrupt) = interrupt {
        install_interrupt(&mut ctx, interrupt);
    }
    ctx.dump(0, dump_name)
        .context("Dump input format context failed.")?;
    Ok(ctx)
}

/// 用自定义 AVIO 构建输出上下文。
///
/// 构建阶段的 muxer 私有选项不会在此消费（FFmpeg 在
/// `avformat_write_header` 时才读取），由各 Writer 的 `write_header`
/// 经 [`write_header_with_options`] 透传。
fn build_output_custom(
    io_context: AVIOContextCustom,
    format: &str,
) -> Result<AVFormatContextOutput> {
    let format_cstr = strutils::str_to_cstring(format);
    AVFormatContextOutput::builder()
        .format_name(&format_cstr)
        .io_context(AVIOContextContainer::Custom(io_context))
        .build()
        .context("Create output format context with custom IO failed.")
}

////////////////////////////////////////
// Interrupt（阻塞操作取消/超时）
////////////////////////////////////////

/// FFmpeg 阻塞操作（网络读、seek 等）的中断控制。
///
/// FFmpeg 在每次可能阻塞的操作前调用 `AVFormatContext.interrupt_callback`；
/// 回调返回非 0 时操作立即中止并返回错误。将同一个句柄传给
/// `ReaderBuilder::with_interrupt` 后，可从任意线程 [`abort`](Interrupt::abort)
/// 或设置 [`timeout`](Interrupt::set_timeout) 来取消卡住的读取。
///
/// 注意：中断回调在 `avformat_open_input` 之后安装，因此打开/探测阶段的
/// 阻塞不受保护；运行时读包、seek 的阻塞可以取消（网络流的主要场景）。
#[derive(Clone)]
pub struct Interrupt {
    data: Arc<InterruptData>,
}

struct InterruptData {
    abort: AtomicBool,
    deadline: Mutex<Option<std::time::Instant>>,
}

impl Interrupt {
    /// 创建一个未触发、无超时的中断句柄。
    pub fn new() -> Self {
        Self {
            data: Arc::new(InterruptData {
                abort: AtomicBool::new(false),
                deadline: Mutex::new(None),
            }),
        }
    }

    /// 立即中止所有阻塞操作（可跨线程调用）。
    pub fn abort(&self) {
        self.data.abort.store(true, Ordering::Relaxed);
    }

    /// 设置超时：从现在起 `timeout` 后自动触发中断。
    /// 对每次"打开 reader → 读取"的生命周期只生效一次，需要复用时重新设置。
    pub fn set_timeout(&self, timeout: std::time::Duration) {
        *self.data.deadline.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(std::time::Instant::now() + timeout);
    }

    /// 是否已触发（abort 或超时到期）。
    pub fn triggered(&self) -> bool {
        if self.data.abort.load(Ordering::Relaxed) {
            return true;
        }
        self.data
            .deadline
            .lock()
            .map(|d| d.is_some_and(|t| std::time::Instant::now() >= t))
            .unwrap_or(false)
    }
}

impl Default for Interrupt {
    fn default() -> Self {
        Self::new()
    }
}

/// 中断回调：返回 1 通知 FFmpeg 中止当前阻塞操作。
///
/// SAFETY: `opaque` 指向由 Reader 持有的 `InterruptData`，其生命周期
/// 覆盖整个 format context（context 先于数据 drop），回调期间指针有效。
unsafe extern "C" fn interrupt_callback(opaque: *mut std::ffi::c_void) -> std::ffi::c_int {
    let data = unsafe { &*(opaque as *const InterruptData) };
    let aborted = data.abort.load(Ordering::Relaxed);
    let timed_out = data
        .deadline
        .lock()
        .map(|d| d.is_some_and(|t| std::time::Instant::now() >= t))
        .unwrap_or(false);
    i32::from(aborted || timed_out)
}

/// 将中断回调安装到已打开的输入上下文。
///
/// 回调的 `opaque` 指向 `interrupt` 内部 `Arc<InterruptData>` 的堆内容；
/// 调用方必须让该 `Interrupt` 存活至 context drop（Reader 将其作为字段
/// 持有，且 context 字段先于 interrupt 字段 drop）。
fn install_interrupt(ctx: &mut AVFormatContextInput, interrupt: &Interrupt) {
    // SAFETY: context 独占；opaque 指向的 Arc 目标由 Reader 的
    // `interrupt` 字段保活，context drop 后不会再有回调。
    unsafe {
        (*ctx.as_mut_ptr()).interrupt_callback = ffi::AVIOInterruptCB {
            callback: Some(interrupt_callback),
            opaque: Arc::as_ptr(&interrupt.data) as *mut std::ffi::c_void,
        };
    }
}

////////////////////////////////////////
// StreamReader（Location/URL 输入）
////////////////////////////////////////

/// Builds a [`StreamReader`].
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
/// use rsmedia::io::StreamReaderBuilder;
/// use rsmedia::Options;
/// let mut options = Options::new();
/// options.insert("rtsp_transport", "tcp");
///
/// let mut reader = StreamReaderBuilder::new("my_file.mp4")
///    .with_options(Some(options))
///    .build()
///    .unwrap();
/// ```
pub struct StreamReaderBuilder<'a> {
    source: Location,
    format: Option<&'a str>,
    options: Option<Options>,
    interrupt: Option<Interrupt>,
}

impl<'a> StreamReaderBuilder<'a> {
    /// Create a new reader with the specified locator.
    ///
    /// # Arguments
    ///
    /// * `source` - Source to read.
    pub fn new(source: impl Into<Location>) -> Self {
        Self {
            source: source.into(),
            format: None,
            options: None,
            interrupt: None,
        }
    }

    /// Specify a custom format for the reader.
    ///
    /// # Arguments
    ///
    /// * `format` - Container format to use.
    pub fn with_format(mut self, format: &'a str) -> Self {
        self.format = Some(format);
        self
    }

    /// Specify options for the backend.
    ///
    /// # Arguments
    ///
    /// * `options` - Options to pass on to input.
    pub fn with_options(mut self, options: impl Into<Option<Options>>) -> Self {
        self.options = options.into();
        self
    }

    /// Attach an [`Interrupt`] handle for cancelling blocked reads/seeks
    /// (network streams). See [`Interrupt`] for coverage limits.
    pub fn with_interrupt(mut self, interrupt: impl Into<Option<Interrupt>>) -> Self {
        self.interrupt = interrupt.into();
        self
    }

    /// Build [`StreamReader`].
    pub fn build(self) -> Result<StreamReader> {
        let filename = strutils::path_to_cstring(&self.source.as_path());
        let protocol = unsafe { ffi::avio_find_protocol_name(filename.as_ptr()) };
        if protocol.is_null() {
            return Err(RsmediaError::custom(format!(
                "Unsupported input source protocol: {}",
                self.source
            )));
        }
        log::debug!(
            "Using input protocol: [{}], source: {}",
            unsafe { strutils::c_char_to_str(protocol) },
            self.source
        );

        let fmt_opt = self
            .format
            .and_then(|str| AVInputFormat::find(&strutils::str_to_cstring(str)));
        let mut dict = self.options.and_then(|opts| opts.into_dict());
        let mut ctx_input = AVFormatContextInput::builder()
            .url(&filename)
            .maybe_format(fmt_opt.as_deref())
            .options(&mut dict)
            .open()
            .context("Create input format context failed.")?;
        if let Some(interrupt) = &self.interrupt {
            install_interrupt(&mut ctx_input, interrupt);
        }
        ctx_input
            .dump(0, &filename)
            .context("Dump input format context failed.")?;
        Ok(StreamReader {
            source: self.source,
            input: ctx_input,
            interrupt: self.interrupt,
        })
    }
}

/// Video reader that can read from files or URLs.
///
/// Implements [`Seekable`]: local files are always byte-seekable, whereas
/// network sources (http/rtsp) depend on the protocol and the server — see the
/// capability table on [`Seekable`].
pub struct StreamReader {
    pub source: Location,
    pub input: AVFormatContextInput,
    // 仅在构建期安装到 context 的 interrupt_callback，其 `opaque` 指向 Arc
    // 堆内容；本字段负责在 context 存活期间保活该 Arc（只持有、不读取）。
    // 不能提前 drop，否则后续阻塞操作触发回调时会悬挂指针（use-after-free）。
    #[allow(dead_code)]
    interrupt: Option<Interrupt>,
}

impl StreamReader {
    /// Create a new video file reader on a given source (path, URL, etc.).
    ///
    /// # Arguments
    ///
    /// * `source` - Source to read from.
    #[inline]
    pub fn new(source: impl Into<Location>) -> Result<Self> {
        StreamReaderBuilder::new(source).build()
    }
}

impl Reader for StreamReader {
    fn input(&self) -> &AVFormatContextInput {
        &self.input
    }

    fn input_mut(&mut self) -> &mut AVFormatContextInput {
        &mut self.input
    }
}

impl Seekable for StreamReader {}

/// 线程安全性说明：`AVFormatContext` 本身非线程安全，这里仅承诺可以**移动**
/// 到其他线程独占使用（`Send`），不承诺 `&Self` 跨线程共享（不实现 `Sync`）。
unsafe impl Send for StreamReader {}

////////////////////////////////////////
// BufferReader（内存输入）
////////////////////////////////////////

/// Builds a [`BufferReader`].
///
/// # Example
///
/// ```no_run
/// use rsmedia::io::BufferReaderBuilder;
/// let data = std::fs::read("my_file.mp4").unwrap();
/// let reader = BufferReaderBuilder::new(data).build().unwrap();
/// ```
pub struct BufferReaderBuilder<'a> {
    data: Vec<u8>,
    format: Option<&'a str>,
    options: Option<Options>,
    interrupt: Option<Interrupt>,
}

impl<'a> BufferReaderBuilder<'a> {
    /// Create a new reader over an in-memory buffer.
    ///
    /// # Arguments
    ///
    /// * `data` - Media file contents to read.
    pub fn new(data: Vec<u8>) -> Self {
        Self {
            data,
            format: None,
            options: None,
            interrupt: None,
        }
    }

    /// Specify a custom format for the reader.
    ///
    /// # Arguments
    ///
    /// * `format` - Container format to use.
    pub fn with_format(mut self, format: &'a str) -> Self {
        self.format = Some(format);
        self
    }

    /// Specify options for the backend.
    ///
    /// # Arguments
    ///
    /// * `options` - Options to pass on to input.
    pub fn with_options(mut self, options: impl Into<Option<Options>>) -> Self {
        self.options = options.into();
        self
    }

    /// Attach an [`Interrupt`] handle. Mainly useful when the buffer backs
    /// a blocking custom source; pure in-memory reads never block.
    pub fn with_interrupt(mut self, interrupt: impl Into<Option<Interrupt>>) -> Self {
        self.interrupt = interrupt.into();
        self
    }

    /// Build [`BufferReader`].
    pub fn build(self) -> Result<BufferReader> {
        let data = Arc::new(self.data);
        let pos = Arc::new(AtomicUsize::new(0));

        let read_data = data.clone();
        let read_pos = pos.clone();
        let read_packet: ReadPacketCallback = Box::new(move |_opaque, buf: &mut [u8]| {
            let p = read_pos.load(Ordering::Relaxed);
            let n = read_data.len().saturating_sub(p).min(buf.len());
            buf[..n].copy_from_slice(&read_data[p..p + n]);
            read_pos.store(p + n, Ordering::Relaxed);
            if n == 0 { ffi::AVERROR_EOF } else { n as i32 }
        });

        let seek_data = data.clone();
        let seek_pos = pos.clone();
        let seek: SeekCallback = Box::new(move |_opaque, offset, whence| {
            memory_seek(&seek_pos, seek_data.len(), offset, whence)
        });

        let io_context = AVIOContextCustom::alloc_context(
            AVMem::new(AVIO_BUFFER_SIZE),
            false,
            Vec::new(),
            Some(read_packet),
            None,
            Some(seek),
        );
        let ctx_input = open_input_custom(
            io_context,
            self.format,
            self.options,
            self.interrupt.as_ref(),
            c"memory",
        )?;
        Ok(BufferReader {
            input: ctx_input,
            interrupt: self.interrupt,
        })
    }
}

/// Video reader that reads from an in-memory buffer.
///
/// Implements [`Seekable`]: the whole input lives in memory and the custom AVIO
/// installs a seek callback, so FFmpeg sets `AVIO_SEEKABLE_NORMAL` and
/// [`Seekable::is_byte_seekable`] is always `true`.
pub struct BufferReader {
    input: AVFormatContextInput,
    // 保活 interrupt_callback 的 opaque 数据，见 [`StreamReader::interrupt`]。
    #[allow(dead_code)]
    interrupt: Option<Interrupt>,
}

impl BufferReader {
    /// Create a new video reader over an in-memory buffer.
    ///
    /// # Arguments
    ///
    /// * `data` - Media file contents to read.
    #[inline]
    pub fn new(data: Vec<u8>) -> Result<Self> {
        BufferReaderBuilder::new(data).build()
    }
}

impl Reader for BufferReader {
    fn input(&self) -> &AVFormatContextInput {
        &self.input
    }

    fn input_mut(&mut self) -> &mut AVFormatContextInput {
        &mut self.input
    }
}

impl Seekable for BufferReader {}

/// 仅承诺可移动到其他线程独占使用（内部回调均为 `Send`）。
unsafe impl Send for BufferReader {}

////////////////////////////////////////
// IoReader（任意 std::io::Read 输入）
////////////////////////////////////////

/// Builds an [`IoReader`].
pub struct IoReaderBuilder<'a, R> {
    reader: R,
    format: Option<&'a str>,
    options: Option<Options>,
    interrupt: Option<Interrupt>,
}

impl<'a, R: std::io::Read + Send + 'static> IoReaderBuilder<'a, R> {
    /// Create a new reader wrapping any [`std::io::Read`] implementor.
    ///
    /// # Arguments
    ///
    /// * `reader` - Source stream to read from (e.g. socket, pipe, decryptor).
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            format: None,
            options: None,
            interrupt: None,
        }
    }

    /// Specify a custom format for the reader.
    ///
    /// # Arguments
    ///
    /// * `format` - Container format to use.
    pub fn with_format(mut self, format: &'a str) -> Self {
        self.format = Some(format);
        self
    }

    /// Specify options for the backend.
    ///
    /// # Arguments
    ///
    /// * `options` - Options to pass on to input.
    pub fn with_options(mut self, options: impl Into<Option<Options>>) -> Self {
        self.options = options.into();
        self
    }

    /// Attach an [`Interrupt`] handle for cancelling blocked reads on the
    /// underlying stream (sockets, pipes, ...).
    pub fn with_interrupt(mut self, interrupt: impl Into<Option<Interrupt>>) -> Self {
        self.interrupt = interrupt.into();
        self
    }

    /// Build [`IoReader`].
    pub fn build(self) -> Result<IoReader> {
        let mut reader = self.reader;
        let read_packet: ReadPacketCallback = Box::new(move |_opaque, buf: &mut [u8]| {
            loop {
                match reader.read(buf) {
                    Ok(0) => return ffi::AVERROR_EOF,
                    Ok(n) => return n as i32,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) => {
                        log::error!("IoReader read error: {e}");
                        return AVERROR_EIO;
                    }
                }
            }
        });

        let io_context = AVIOContextCustom::alloc_context(
            AVMem::new(AVIO_BUFFER_SIZE),
            false,
            Vec::new(),
            Some(read_packet),
            None,
            None,
        );
        let ctx_input = open_input_custom(
            io_context,
            self.format,
            self.options,
            self.interrupt.as_ref(),
            c"stream",
        )?;
        Ok(IoReader {
            input: ctx_input,
            interrupt: self.interrupt,
        })
    }
}

/// Video reader that reads from any [`std::io::Read`] implementor.
///
/// For streaming inputs (sockets, pipes, decryptors, ...). Does **not** implement
/// [`Seekable`]. Because `avformat_open_input` needs to probe, `reader` must be
/// re-readable; no seeking is required since probing only moves forward.
pub struct IoReader {
    input: AVFormatContextInput,
    // 保活 interrupt_callback 的 opaque 数据，见 [`StreamReader::interrupt`]。
    #[allow(dead_code)]
    interrupt: Option<Interrupt>,
}

impl IoReader {
    /// Create a new video reader wrapping any [`std::io::Read`] implementor.
    ///
    /// # Arguments
    ///
    /// * `reader` - Source stream to read from.
    #[inline]
    pub fn new(reader: impl std::io::Read + Send + 'static) -> Result<Self> {
        IoReaderBuilder::new(reader).build()
    }
}

impl Reader for IoReader {
    fn input(&self) -> &AVFormatContextInput {
        &self.input
    }

    fn input_mut(&mut self) -> &mut AVFormatContextInput {
        &mut self.input
    }
}

/// 仅承诺可移动到其他线程独占使用（内部 reader 与回调均为 `Send`）。
unsafe impl Send for IoReader {}

////////////////////////////////////////
// Writer trait
////////////////////////////////////////

/// Any type that implements this can write video packets.
///
/// 该 trait 是公开的扩展点：可以为任意目标（socket、channel、加密流等）
/// 实现自定义 Writer。
pub trait Writer {
    /// 单次 `write_*` 调用产生的输出类型：
    /// [`StreamWriter`] 为 `()`（数据直接写出），
    /// [`BufferWriter`] 为 `Vec<u8>`（本次调用新增的字节），
    /// [`PacketizedBufWriter`] 为 `Vec<Vec<u8>>`（按包切分的字节块）。
    type Out;

    /// Write the container header.
    fn write_header(&mut self) -> Result<Self::Out>;

    /// Write a packet into the container.
    ///
    /// # Arguments
    ///
    /// * `packet` - AVPacket to write.
    fn write_frame(&mut self, packet: &mut AVPacket) -> Result<Self::Out>;

    /// Write a packet into the container and take care of interleaving.
    ///
    /// # Arguments
    ///
    /// * `packet` - AVPacket to write.
    fn write_interleaved(&mut self, packet: &mut AVPacket) -> Result<Self::Out>;

    /// Write the container trailer.
    fn write_trailer(&mut self) -> Result<Self::Out>;

    /// Obtain reference to output context.
    fn output(&self) -> &AVFormatContextOutput;

    /// Obtain mutable reference to output context.
    fn output_mut(&mut self) -> &mut AVFormatContextOutput;

    /// new stream
    fn add_stream(&mut self, codecpar: AVCodecParameters, timebase: ffi::AVRational) -> usize {
        let mut av_stream = self.output_mut().new_stream();
        av_stream.set_codecpar(codecpar);
        av_stream.set_time_base(timebase);
        av_stream.index as usize
    }

    /// 获取输出流当前的时间基。
    ///
    /// 注意：`write_header` 之后 muxer 可能调整 stream 的时间基（例如 MP4 的
    /// movenc 会重设 timescale）。因此写包时应**实时获取**，不要缓存 write 前
    /// 的值，否则 packet 的 pts/duration 会按错误的 time_base 解析。
    fn stream_time_base(&self, stream_index: usize) -> ffi::AVRational {
        self.output()
            .streams()
            .get(stream_index)
            .map(|s| s.time_base)
            .unwrap_or(crate::time::TIME_BASE)
    }
}

/// 将 builder 阶段未消费的 options 在 `write_header` 时传给 muxer。
///
/// FFmpeg 的 muxer 私有选项（如 `movflags`）在 `avformat_write_header` 时
/// 才被消费，构建阶段传入的 options 若不透传到这里会被静默丢弃。
fn write_header_with_options(
    output: &mut AVFormatContextOutput,
    options: &mut Option<AVDictionary>,
) -> Result<()> {
    output
        .write_header(options)
        .context("Failed to write header")
}

/// Flush avio 内部缓冲，确保字节立即送达 write 回调（内存/流式 Writer 用）。
fn flush_avio(output: &mut AVFormatContextOutput) {
    unsafe {
        let pb = (*output.as_mut_ptr()).pb;
        if !pb.is_null() {
            ffi::avio_flush(pb);
        }
    }
}

////////////////////////////////////////
// StreamWriter（Location/URL 输出）
////////////////////////////////////////

/// Build a [`StreamWriter`].
pub struct StreamWriterBuilder<'a> {
    destination: Location,
    format: Option<&'a str>,
    options: Option<Options>,
}

impl<'a> StreamWriterBuilder<'a> {
    /// Create a new writer with the specified destination.
    ///
    /// # Arguments
    ///
    /// * `destination` - Destination to write to.
    pub fn new(destination: impl Into<Location>) -> Self {
        Self {
            destination: destination.into(),
            format: None,
            options: None,
        }
    }

    /// Specify a container format for the writer.
    ///
    /// # Arguments
    ///
    /// * `format` - Container format to use. eg. `"mp4"`, `"mkv"`, `"mov"`, `"avi"`, `"flv"`.
    ///
    /// reference: https://trac.ffmpeg.org/wiki/HWAccelIntro
    ///
    /// | Format                          | Filename Extension | H.264/AVC | H.265/HEVC | AV1   |
    /// |---------------------------------|--------------------|-----------|------------|-------|
    /// | Matroska                        | .mkv               | Y         | Y          | Y     |
    /// | MPEG-4 Part 14 (MP4)            | .mp4               | Y         | Y          | Y     |
    /// | Audio Video Interleave (AVI)    | .avi               | Y         | N          | Y     |
    /// | Material Exchange Format (MXF)  | .mxf               | Y         | n/a        | n/a   |
    /// | MPEG transport stream (TS)      | .ts                | Y         | Y          | N     |
    /// | 3GPP (3GP)                      | .3gp               | Y         | n/a        | n/a   |
    /// | Flash Video (FLV)               | .flv               | Y         | n/a        | n/a   |
    /// | WebM                            | .webm              | n/a       | n/a        | Y     |
    /// | Advanced Systems Format (ASF)   | .asf, .wmv         | Y         | Y          | Y     |
    /// | QuickTime File Format (QTFF)    | .mov               | Y         | Y          | n/a   |
    pub fn with_format(mut self, format: &'a str) -> Self {
        self.format = Some(format);
        self
    }

    /// Specify options for the backend.
    ///
    /// # Arguments
    ///
    /// * `options` - Options to pass on to output.
    pub fn with_options(mut self, options: impl Into<Option<Options>>) -> Self {
        self.options = options.into();
        self
    }

    /// Build [`StreamWriter`].
    pub fn build(self) -> Result<StreamWriter> {
        let filename = strutils::path_to_cstring(&self.destination.as_path());
        let format = self.format.map(strutils::str_to_cstring);
        let mut dict = self.options.and_then(|opts| opts.into_dict());
        let output_ctx = AVFormatContextOutput::builder()
            .filename(&filename)
            .maybe_format_name(format.as_deref())
            // options 先由 avio/protocol 层消费（avio_open2），未被消费的
            // （如 movflags 等 muxer 私有选项）留在 dict 中，待 write_header
            // 时传给 muxer。
            .options(&mut dict)
            .build()
            .context("Create output format context failed.")?;
        Ok(StreamWriter {
            destination: self.destination,
            output: output_ctx,
            options: dict,
        })
    }
}

/// File writer for video files.
///
/// # Example
///
/// Create a video writer that produces fragmented MP4:
///
/// ```ignore
/// let mut options = HashMap::new();
/// options.insert(
///     "movflags".to_string(),
///     "frag_keyframe+empty_moov".to_string(),
/// );
///
/// let mut writer = StreamWriterBuilder::new("my_file.mp4")
///     .with_options(Some(options))
///     .build()
///     .unwrap();
/// ```
pub struct StreamWriter {
    pub destination: Location,
    pub output: AVFormatContextOutput,
    /// 构建阶段未被 avio 层消费的 options，write_header 时传给 muxer。
    pub(crate) options: Option<AVDictionary>,
}

impl StreamWriter {
    /// Create a new file writer for video files.
    ///
    /// # Arguments
    ///
    /// * `dest` - Where to write to.
    #[inline]
    pub fn new(destination: impl Into<Location>) -> Result<Self> {
        StreamWriterBuilder::new(destination).build()
    }
}

impl Writer for StreamWriter {
    type Out = ();

    fn write_header(&mut self) -> Result<()> {
        let mut dict = self.options.take();
        write_header_with_options(&mut self.output, &mut dict)
    }

    fn write_frame(&mut self, packet: &mut AVPacket) -> Result<()> {
        self.output.write_frame(packet)?;
        Ok(())
    }

    fn write_interleaved(&mut self, packet: &mut AVPacket) -> Result<()> {
        self.output.interleaved_write_frame(packet)?;
        Ok(())
    }

    fn write_trailer(&mut self) -> Result<()> {
        self.output
            .write_trailer()
            .context("Failed to write trailer")?;
        Ok(())
    }

    fn output(&self) -> &AVFormatContextOutput {
        &self.output
    }

    fn output_mut(&mut self) -> &mut AVFormatContextOutput {
        &mut self.output
    }
}

/// 仅承诺可移动到其他线程独占使用。
unsafe impl Send for StreamWriter {}

////////////////////////////////////////
// BufferWriter（内存输出，持久可 seek 的 custom IO）
////////////////////////////////////////

/// 内存写状态：`data` 为累计输出，`pos` 为 avio 当前写位置（支持 seek 回退
/// 重写），`delivered` 为已通过增量接口返回给调用方的字节数。
#[derive(Default)]
struct MemWriterState {
    data: Vec<u8>,
    pos: usize,
    delivered: usize,
}

/// 自定义 IO 写回调共享逻辑。
///
/// 该函数在 FFmpeg 的 `extern "C"` 调用栈中执行，**严禁 panic**
/// （unwind 穿过 FFI 边界是 UB）：锁中毒时返回 `AVERROR(EIO)`。
fn mem_write(state: &Mutex<MemWriterState>, buf: &[u8]) -> i32 {
    let mut st = match state.lock() {
        Ok(guard) => guard,
        Err(_) => return AVERROR_EIO,
    };
    let pos = st.pos;
    let end = pos + buf.len();
    if end > st.data.len() {
        st.data.resize(end, 0);
    }
    st.data[pos..end].copy_from_slice(buf);
    st.pos = end;
    buf.len() as i32
}

/// 自定义 IO seek 回调共享逻辑（同样禁止 panic；失败返回 -1）。
fn mem_seek(state: &Mutex<MemWriterState>, offset: i64, whence: i32) -> i64 {
    let mut st = match state.lock() {
        Ok(guard) => guard,
        Err(_) => return -1,
    };
    let pos = AtomicUsize::new(st.pos);
    let target = memory_seek(&pos, st.data.len(), offset, whence);
    st.pos = pos.load(Ordering::Relaxed);
    target
}

/// Build a [`BufferWriter`].
pub struct BufferWriterBuilder<'a> {
    format: &'a str,
    options: Option<Options>,
}

impl<'a> BufferWriterBuilder<'a> {
    /// Create a new writer that writes to a buffer.
    ///
    /// # Arguments
    ///
    /// * `format` - Container format to use.
    pub fn new(format: &'a str) -> Self {
        Self {
            format,
            options: None,
        }
    }

    /// Specify options for the backend.
    ///
    /// # Arguments
    ///
    /// * `options` - Options to pass on to output.
    pub fn with_options(mut self, options: impl Into<Option<Options>>) -> Self {
        self.options = options.into();
        self
    }

    /// Build [`BufferWriter`].
    pub fn build(self) -> Result<BufferWriter> {
        let state = Arc::new(Mutex::new(MemWriterState::default()));

        let write_state = state.clone();
        let write_packet: WritePacketCallback =
            Box::new(move |_opaque, buf: &[u8]| mem_write(&write_state, buf));

        let seek_state = state.clone();
        let seek: SeekCallback =
            Box::new(move |_opaque, offset, whence| mem_seek(&seek_state, offset, whence));

        // 持久且可 seek 的内存 IO：avio 的 pos 跨 write_* 调用连续，
        // 支持 mp4 等 muxer 在 trailer 阶段回写 header 区域。
        let io_context = AVIOContextCustom::alloc_context(
            AVMem::new(AVIO_BUFFER_SIZE),
            true,
            Vec::new(),
            None,
            Some(write_packet),
            Some(seek),
        );
        let output = build_output_custom(io_context, self.format)?;
        Ok(BufferWriter {
            output,
            state,
            options: self.options.and_then(|opts| opts.into_dict()),
        })
    }
}

/// Video writer that writes to a buffer.
///
/// 每次写入操作（write_header/write_frame/write_trailer）返回**本次新增**的
/// 字节增量，适合流式格式（mpegts、fmp4 等）的分段发送；对会回写 header 的
/// 格式（普通 mp4），请在 write_trailer 后用 [`Self::into_bytes`] 取完整输出。
///
/// # Example
///
/// ```ignore
/// let mut writer = BufferWriter::new("mp4").unwrap();
/// let bytes = writer.write_header()?;
/// ```
pub struct BufferWriter {
    pub(crate) output: AVFormatContextOutput,
    state: Arc<Mutex<MemWriterState>>,
    options: Option<AVDictionary>,
}

impl BufferWriter {
    /// Create a video writer that writes to a buffer and returns the resulting bytes.
    ///
    /// # Arguments
    ///
    /// * `format` - Container format to use.
    #[inline]
    pub fn new(format: &str) -> Result<Self> {
        BufferWriterBuilder::new(format).build()
    }

    /// 取出本次写入操作新增的字节增量。
    fn take_written(&mut self) -> Vec<u8> {
        let mut st = self.state.lock().expect("mem writer state poisoned");
        let delta = st.data[st.delivered..].to_vec();
        st.delivered = st.data.len();
        delta
    }

    /// 消耗 writer 并返回**完整**的输出字节。
    ///
    /// 对会在 trailer 阶段回写 header 的格式（如普通 mp4），增量接口拿不到
    /// 回写的字节，必须用本方法获取最终完整结果。应在 `write_trailer` 之后
    /// 调用。
    pub fn into_bytes(self) -> Vec<u8> {
        let Self {
            output,
            state,
            options: _,
        } = self;
        // 先释放 format context（连带 IO 回调释放其持有的 state 引用）
        drop(output);
        match Arc::try_unwrap(state) {
            Ok(st) => st.into_inner().expect("mem writer state poisoned").data,
            // 不可达：output 已 drop，回调持有的 Arc 引用随之释放。
            // 用 panic（fail-fast）而非静默返回空 Vec，避免数据无声丢失。
            Err(_) => panic!("BufferWriter: state still referenced after context drop"),
        }
    }
}

impl Writer for BufferWriter {
    type Out = Vec<u8>;

    fn write_header(&mut self) -> Result<Vec<u8>> {
        let mut dict = self.options.take();
        write_header_with_options(&mut self.output, &mut dict)?;
        flush_avio(&mut self.output);
        Ok(self.take_written())
    }

    fn write_frame(&mut self, packet: &mut AVPacket) -> Result<Vec<u8>> {
        self.output.write_frame(packet)?;
        flush_avio(&mut self.output);
        Ok(self.take_written())
    }

    fn write_interleaved(&mut self, packet: &mut AVPacket) -> Result<Vec<u8>> {
        self.output.interleaved_write_frame(packet)?;
        flush_avio(&mut self.output);
        Ok(self.take_written())
    }

    fn write_trailer(&mut self) -> Result<Vec<u8>> {
        self.output.write_trailer()?;
        flush_avio(&mut self.output);
        Ok(self.take_written())
    }

    fn output(&self) -> &AVFormatContextOutput {
        &self.output
    }

    fn output_mut(&mut self) -> &mut AVFormatContextOutput {
        &mut self.output
    }
}

/// 仅承诺可移动到其他线程独占使用。
unsafe impl Send for BufferWriter {}

////////////////////////////////////////
// PacketizedBufWriter（按包切分的内存输出）
////////////////////////////////////////

/// Build a [`PacketizedBufWriter`].
pub struct PacketizedBufWriterBuilder<'a> {
    format: &'a str,
    options: Option<Options>,
}

impl<'a> PacketizedBufWriterBuilder<'a> {
    /// Create a new writer that writes to a packetized buffer.
    ///
    /// # Arguments
    ///
    /// * `format` - Container format to use.
    pub fn new(format: &'a str) -> Self {
        Self {
            format,
            options: None,
        }
    }

    /// Specify options for the backend.
    ///
    /// # Arguments
    ///
    /// * `options` - Options to pass on to output.
    pub fn with_options(mut self, options: impl Into<Option<Options>>) -> Self {
        self.options = options.into();
        self
    }

    /// Build [`PacketizedBufWriter`].
    pub fn build(self) -> Result<PacketizedBufWriter> {
        let buffers = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));

        // 回调在 FFI 调用栈中执行，严禁 panic：锁中毒返回 AVERROR(EIO)。
        let write_buffers = buffers.clone();
        let write_packet: WritePacketCallback =
            Box::new(move |_opaque, buf: &[u8]| match write_buffers.lock() {
                Ok(mut guard) => {
                    guard.push(buf.to_vec());
                    buf.len() as i32
                }
                Err(_) => AVERROR_EIO,
            });

        // avio 缓冲大小与 max_packet_size 一致（分包写出的前提）
        let mut io_context = AVIOContextCustom::alloc_context(
            AVMem::new(PacketizedBufWriter::PACKET_SIZE),
            true,
            Vec::new(),
            None,
            Some(write_packet),
            None,
        );
        // rsmpeg 未暴露可变字段访问，经裸指针设置 max_packet_size。
        // SAFETY: io_context 独占持有该 context，此处处于挂载前的初始化阶段。
        unsafe {
            (*io_context.as_mut_ptr()).max_packet_size = PacketizedBufWriter::PACKET_SIZE as _;
        }
        let output = build_output_custom(io_context, self.format)?;
        Ok(PacketizedBufWriter {
            output,
            buffers,
            options: self.options.and_then(|opts| opts.into_dict()),
        })
    }
}

/// Video writer that writes multiple packets to a buffer and returns the resulting
/// bytes for each packet.
///
/// # Example
///
/// ```ignore
/// let mut writer = BufPacketizedWriter::new("rtp").unwrap();
/// let bytes = writer.write_header()?;
/// ```
pub struct PacketizedBufWriter {
    pub(crate) output: AVFormatContextOutput,
    buffers: Arc<Mutex<Vec<Vec<u8>>>>,
    options: Option<AVDictionary>,
}

impl PacketizedBufWriter {
    /// Actual packet size. Value should be below MTU.
    const PACKET_SIZE: usize = 1024;

    /// Create a video writer that writes multiple packets to a buffer and returns the resulting
    /// bytes for each packet.
    ///
    /// # Arguments
    ///
    /// * `format` - Container format to use.
    #[inline]
    pub fn new(format: &str) -> Result<Self> {
        PacketizedBufWriterBuilder::new(format).build()
    }

    #[inline]
    fn take_buffers(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut *self.buffers.lock().expect("packet buffers poisoned"))
    }
}

impl Writer for PacketizedBufWriter {
    type Out = Vec<Vec<u8>>;

    fn write_header(&mut self) -> Result<Vec<Vec<u8>>> {
        let mut dict = self.options.take();
        write_header_with_options(&mut self.output, &mut dict)?;
        flush_avio(&mut self.output);
        Ok(self.take_buffers())
    }

    fn write_frame(&mut self, packet: &mut AVPacket) -> Result<Vec<Vec<u8>>> {
        self.output.write_frame(packet)?;
        flush_avio(&mut self.output);
        Ok(self.take_buffers())
    }

    fn write_interleaved(&mut self, packet: &mut AVPacket) -> Result<Vec<Vec<u8>>> {
        self.output.interleaved_write_frame(packet)?;
        flush_avio(&mut self.output);
        Ok(self.take_buffers())
    }

    fn write_trailer(&mut self) -> Result<Vec<Vec<u8>>> {
        self.output.write_trailer()?;
        flush_avio(&mut self.output);
        Ok(self.take_buffers())
    }

    fn output(&self) -> &AVFormatContextOutput {
        &self.output
    }

    fn output_mut(&mut self) -> &mut AVFormatContextOutput {
        &mut self.output
    }
}

/// 仅承诺可移动到其他线程独占使用。
unsafe impl Send for PacketizedBufWriter {}

////////////////////////////////////////
// CustomIoWriter（任意 std::io::Write 输出）
////////////////////////////////////////

/// Builds a [`CustomIoWriter`].
pub struct CustomIoWriterBuilder<'a, W> {
    writer: W,
    format: &'a str,
    options: Option<Options>,
}

impl<'a, W: std::io::Write + Send + 'static> CustomIoWriterBuilder<'a, W> {
    /// Create a new writer wrapping any [`std::io::Write`] implementor.
    ///
    /// # Arguments
    ///
    /// * `format` - Container format to use.
    /// * `writer` - Destination stream to write to (e.g. socket, pipe, encryptor).
    pub fn new(format: &'a str, writer: W) -> Self {
        Self {
            writer,
            format,
            options: None,
        }
    }

    /// Specify options for the backend.
    ///
    /// # Arguments
    ///
    /// * `options` - Options to pass on to output.
    pub fn with_options(mut self, options: impl Into<Option<Options>>) -> Self {
        self.options = options.into();
        self
    }

    /// Build [`CustomIoWriter`].
    pub fn build(self) -> Result<CustomIoWriter<W>> {
        let inner = Arc::new(Mutex::new(self.writer));

        let write_inner = inner.clone();
        // 回调在 FFI 调用栈中执行，严禁 panic：锁中毒或写入失败均返回 AVERROR(EIO)。
        let write_packet: WritePacketCallback = Box::new(move |_opaque, buf: &[u8]| {
            let mut w = match write_inner.lock() {
                Ok(guard) => guard,
                Err(_) => return AVERROR_EIO,
            };
            match w.write_all(buf) {
                Ok(()) => buf.len() as i32,
                Err(e) => {
                    log::error!("CustomIoWriter write error: {e}");
                    AVERROR_EIO
                }
            }
        });

        let io_context = AVIOContextCustom::alloc_context(
            AVMem::new(AVIO_BUFFER_SIZE),
            true,
            Vec::new(),
            None,
            Some(write_packet),
            None,
        );
        let output = build_output_custom(io_context, self.format)?;
        Ok(CustomIoWriter {
            output,
            inner,
            options: self.options.and_then(|opts| opts.into_dict()),
        })
    }
}

/// Video writer that writes to any [`std::io::Write`] implementor.
///
/// 每次 `write_*` 调用后会 flush avio 缓冲，字节及时送达底层流。
pub struct CustomIoWriter<W: std::io::Write + Send + 'static> {
    pub(crate) output: AVFormatContextOutput,
    inner: Arc<Mutex<W>>,
    options: Option<AVDictionary>,
}

impl<W: std::io::Write + Send + 'static> CustomIoWriter<W> {
    /// Create a video writer wrapping any [`std::io::Write`] implementor.
    ///
    /// # Arguments
    ///
    /// * `format` - Container format to use.
    /// * `writer` - Destination stream to write to.
    #[inline]
    pub fn new(format: &str, writer: W) -> Result<Self> {
        CustomIoWriterBuilder::new(format, writer).build()
    }

    /// 消耗 writer 并取回底层 [`std::io::Write`] 实现（应在 `write_trailer` 之后调用）。
    pub fn into_inner(self) -> std::io::Result<W> {
        let Self {
            output,
            inner,
            options: _,
        } = self;
        // 先释放 format context（连带 IO 回调释放其持有的 inner 引用）
        drop(output);
        match Arc::try_unwrap(inner) {
            // 锁中毒（回调 panic 过）时仍取回 writer 本体
            Ok(w) => Ok(w
                .into_inner()
                .unwrap_or_else(|poisoned| poisoned.into_inner())),
            // 理论不可达：output 已释放，唯一引用在此
            Err(_) => Err(std::io::Error::other(
                "custom io writer still has live references",
            )),
        }
    }
}

impl<W: std::io::Write + Send + 'static> Writer for CustomIoWriter<W> {
    type Out = ();

    fn write_header(&mut self) -> Result<()> {
        let mut dict = self.options.take();
        write_header_with_options(&mut self.output, &mut dict)?;
        flush_avio(&mut self.output);
        Ok(())
    }

    fn write_frame(&mut self, packet: &mut AVPacket) -> Result<()> {
        self.output.write_frame(packet)?;
        flush_avio(&mut self.output);
        Ok(())
    }

    fn write_interleaved(&mut self, packet: &mut AVPacket) -> Result<()> {
        self.output.interleaved_write_frame(packet)?;
        flush_avio(&mut self.output);
        Ok(())
    }

    fn write_trailer(&mut self) -> Result<()> {
        self.output.write_trailer()?;
        flush_avio(&mut self.output);
        Ok(())
    }

    fn output(&self) -> &AVFormatContextOutput {
        &self.output
    }

    fn output_mut(&mut self) -> &mut AVFormatContextOutput {
        &mut self.output
    }
}

/// 仅承诺可移动到其他线程独占使用（内部 writer 与回调均为 `Send`）。
unsafe impl<W: std::io::Write + Send + 'static> Send for CustomIoWriter<W> {}

////////////////////////////////////////
// Logging
////////////////////////////////////////

/// Initialize the logging handler. This will redirect all ffmpeg logging to the Rust `tracing`
/// crate and any subscribers to it.
pub fn init_logging() {
    unsafe {
        ffi::av_log_set_callback(Some(log_callback));
        ffi::av_log_set_level(ffi::AV_LOG_TRACE as _);
        // ffi::av_log_set_flags()
    }
}

/// Internal function with C-style callback behavior that receives all log messages from ffmpeg and
/// handles them with the `log` crate, the Rust way.
///
/// # Arguments
///
/// * `avcl` - Internal struct with log message data.
/// * `level_no` - Log message level integer.
/// * `fmt` - Log message format string.
/// * `vl` - Variable list with format string items.
unsafe extern "C" fn log_callback(
    avcl: *mut std::ffi::c_void,
    level_no: std::ffi::c_int,
    fmt: *const std::ffi::c_char,
    #[cfg(all(target_arch = "x86_64", target_family = "unix"))] vl: *mut ffi::__va_list_tag,
    #[cfg(not(all(target_arch = "x86_64", target_family = "unix")))] vl: ffi::va_list,
) {
    // Check whether or not the message would be printed at all.
    let event_would_log = match level_no as u32 {
        // These are all error states.
        ffi::AV_LOG_PANIC | ffi::AV_LOG_FATAL | ffi::AV_LOG_ERROR => {
            tracing::enabled!(tracing::Level::ERROR)
        }
        ffi::AV_LOG_WARNING => tracing::enabled!(tracing::Level::WARN),
        ffi::AV_LOG_INFO => tracing::enabled!(tracing::Level::INFO),
        // There is no "verbose" in `log`, so we just put it in the "debug" category.
        ffi::AV_LOG_VERBOSE | ffi::AV_LOG_DEBUG => tracing::enabled!(tracing::Level::DEBUG),
        ffi::AV_LOG_TRACE => tracing::enabled!(tracing::Level::TRACE),
        _ => {
            return;
        }
    };

    if event_would_log {
        // Allocate some memory for the log line (might be truncated). 1024 bytes is the number used
        // by ffmpeg itself, so it should be mostly fine.
        let mut line = [0; 1024];
        let mut print_prefix: std::ffi::c_int = 1;
        // Use the ffmpeg default formatting.
        let ret = unsafe {
            ffi::av_log_format_line2(
                avcl,
                level_no,
                fmt,
                vl,
                line.as_mut_ptr(),
                (line.len()) as std::ffi::c_int,
                (&mut print_prefix) as *mut std::ffi::c_int,
            )
        };
        // Simply discard the log message if formatting fails.
        if ret > 0
            && let Ok(line) = unsafe { std::ffi::CStr::from_ptr(line.as_mut_ptr()) }.to_str()
        {
            let line = line.trim();
            if log_filter_hacks(line) {
                match level_no as u32 {
                    // These are all error states.
                    ffi::AV_LOG_PANIC | ffi::AV_LOG_FATAL | ffi::AV_LOG_ERROR => {
                        tracing::error!(target: "rsmedia", "{}", line)
                    }
                    ffi::AV_LOG_WARNING => tracing::warn!(target: "rsmedia", "{}", line),
                    ffi::AV_LOG_INFO => tracing::info!(target: "rsmedia", "{}", line),
                    // There is no "verbose" in `log`, so we just put it in the "debug" category.
                    ffi::AV_LOG_VERBOSE | ffi::AV_LOG_DEBUG => {
                        tracing::debug!(target: "rsmedia", "{}", line)
                    }
                    ffi::AV_LOG_TRACE => tracing::trace!(target: "rsmedia", "{}", line),
                    _ => {}
                };
            }
        }
    }
}

/// Helper function to filter out any lines that we don't want to log because they contaminate.
/// Currently, it includes the following log line hacks:
///
/// * **Pelco H264 encoding issue**. Pelco cameras and encoders have a problem with their SEI NALs
///   that causes ffmpeg to complain but does not hurt the stream. It does cause continuous error
///   messages though which we filter out here.
fn log_filter_hacks(line: &str) -> bool {
    /* Hack 1 */
    const HACK_1_PELCO_NEEDLE_1: &str = "SEI type 5 size";
    const HACK_1_PELCO_NEEDLE_2: &str = "truncated at";
    !(line.contains(HACK_1_PELCO_NEEDLE_1) && line.contains(HACK_1_PELCO_NEEDLE_2))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EncoderBuilder;
    use crate::mux::{Demuxer, Muxer};
    use crate::options::Options;
    use crate::pixel::PixelFormat;
    use crate::{DecoderBuilder, MediaType};
    use rsmpeg::avutil::AVFrame;

    /// `ffi_enum!` 生成的位集合能力：`|` 组合与 `Into<i32>` 转换。
    #[test]
    fn test_avseek_flag_bitops() {
        // 注意：rsmpeg 侧的 AVSEEK_FLAG_* 常量类型为 u32，断言时归一化到 i32。
        let combined = AVSeekFlag::BACKWARD | AVSeekFlag::ANY;
        assert_eq!(
            combined,
            ffi::AVSEEK_FLAG_BACKWARD as i32 | ffi::AVSEEK_FLAG_ANY as i32
        );

        // 与 rsmpeg 的 u32 常量混合时需显式 `as i32`（宏只实现 BitOr<i32>，
        // 以免 repr=u32 的旗标枚举生成重复 impl）。
        let mixed = AVSeekFlag::FRAME | (ffi::AVSEEK_FLAG_BYTE as i32);
        assert_eq!(
            mixed,
            ffi::AVSEEK_FLAG_FRAME as i32 | ffi::AVSEEK_FLAG_BYTE as i32
        );

        let raw: i32 = AVSeekFlag::FRAME.into();
        assert_eq!(raw, ffi::AVSEEK_FLAG_FRAME as i32);
        // as_raw 与 Into 结果一致
        assert_eq!(AVSeekFlag::BYTE.as_raw(), i32::from(AVSeekFlag::BYTE));
    }

    /// 生成 RGB24 渐变测试帧（image2 序列写入用）。
    fn generate_rgb_frame(width: usize, height: usize, index: i64) -> AVFrame {
        let mut frame = AVFrame::new();
        frame.set_width(width as i32);
        frame.set_height(height as i32);
        frame.set_format(PixelFormat::RGB24.into());
        frame.alloc_buffer().expect("alloc rgb frame buffer");

        let plane = frame.data_mut()[0];
        let linesize = frame.linesize[0];
        for y in 0..height {
            for x in 0..width {
                let i = y * linesize as usize + x * 3;
                unsafe {
                    *plane.add(i) = (x * 255 / width) as u8;
                    *plane.add(i + 1) = (y * 255 / height) as u8;
                    *plane.add(i + 2) = ((index * 25) % 256) as u8;
                }
            }
        }
        frame
    }

    /// 图片序列写入（image2 muxer + png 编码器）：写入 N 帧 => 磁盘上生成
    /// N 个按 `%03d` 模式编号的 PNG 文件。
    #[test]
    fn test_write_image_sequence() -> Result<()> {
        let pattern = crate::test_support::test_output_path("images", "img_%03d.png");
        let n_frames = 8;

        let writer = StreamWriterBuilder::new(&pattern)
            .with_format("image2")
            .build()?;
        let mut muxer = Muxer::new_from_writer(writer);

        let encoder = EncoderBuilder::new_video(64, 48)
            .with_codec_name("png".to_string())
            .build()?;
        let video_index = muxer.add_encoder(encoder)?;

        for i in 0..n_frames {
            let mut frame = generate_rgb_frame(64, 48, i);
            frame.set_pts(i);
            muxer.mux(frame, video_index)?;
        }
        muxer.finish()?;

        // 校验每个编号文件都存在且非空（image2 从 start_number=1 开始编号）
        let dir = pattern.parent().unwrap();
        for i in 1..=n_frames {
            let file = dir.join(format!("img_{i:03}.png"));
            let meta = std::fs::metadata(&file).map_err(|e| {
                RsmediaError::custom(format!("expected sequence file {}: {e}", file.display()))
            })?;
            assert!(meta.len() > 0, "sequence file {} is empty", file.display());
        }
        // 未写入的下一个编号不应存在
        assert!(!dir.join(format!("img_{:03}.png", n_frames + 1)).exists());

        Ok(())
    }

    /// 图片序列读取（image2 demuxer）：按 `%03d` 模式打开序列，解码帧数应与
    /// 写入帧数一致，且尺寸正确。
    #[test]
    fn test_read_image_sequence() -> Result<()> {
        // 复用写入测试生成的序列；若不存在则现场生成
        let pattern = crate::test_support::test_output_path("images", "img_%03d.png");
        if !pattern.with_file_name("img_001.png").exists() {
            let writer = StreamWriterBuilder::new(&pattern)
                .with_format("image2")
                .build()?;
            let mut muxer = Muxer::new_from_writer(writer);
            let encoder = EncoderBuilder::new_video(64, 48)
                .with_codec_name("png".to_string())
                .build()?;
            let video_index = muxer.add_encoder(encoder)?;
            for i in 0..8 {
                let mut frame = generate_rgb_frame(64, 48, i);
                frame.set_pts(i);
                muxer.mux(frame, video_index)?;
            }
            muxer.finish()?;
        }

        let reader = StreamReaderBuilder::new(&pattern)
            .with_format("image2")
            .with_options(Options::from_iter([(
                "framerate".to_string(),
                "5".to_string(),
            )]))
            .build()?;

        let demuxer = Demuxer::new_from_reader(reader, None, None)?;
        let decoded: Vec<_> = demuxer.filter_map(|res| res.ok()).collect();
        assert_eq!(
            decoded.len(),
            8,
            "expected 8 decoded frames from image sequence"
        );
        for (_, frame) in &decoded {
            assert_eq!(frame.width, 64);
            assert_eq!(frame.height, 48);
        }

        Ok(())
    }

    /// 单张图片读取：jpg 由 FFmpeg 自动探测（image2/mjpeg demuxer），应能
    /// 解码出至少一帧且尺寸与源图一致。
    #[test]
    fn test_read_single_image() -> Result<()> {
        let demuxer = Demuxer::new(std::path::Path::new("assets/cat.jpg"))?;
        let decoded: Vec<_> = demuxer.filter_map(|res| res.ok()).collect();
        assert!(
            !decoded.is_empty(),
            "expected at least one decoded frame from a single image"
        );
        let (_, frame) = &decoded[0];
        assert!(frame.width > 0 && frame.height > 0);

        Ok(())
    }

    /// 内存写入 -> 内存读取 -> 解码 + seek 的完整往返：
    /// 验证 BufferWriter（持久 custom IO）、BufferReader、Seekable 实现协同工作。
    #[test]
    fn test_buffer_writer_reader_roundtrip() -> Result<()> {
        // 1. 编码 mpegts 到 BufferWriter（流式格式）
        let writer = BufferWriter::new("mpegts")?;
        let mut muxer = Muxer::new_from_writer(writer);
        let encoder = EncoderBuilder::new_video(64, 48).build()?;
        let tb = encoder.time_base();
        let video_index = muxer.add_encoder(encoder)?;
        let mut total = 0usize;
        for i in 0..8 {
            let mut frame = generate_rgb_frame(64, 48, i);
            frame.set_pts(i);
            frame.set_time_base(tb);
            if let Some(chunk) = muxer.mux(frame, video_index)? {
                total += chunk.len();
            }
        }
        muxer.finish()?;
        let bytes = muxer.into_writer().into_bytes();
        assert!(!bytes.is_empty(), "buffer writer produced no output");
        assert!(
            bytes.len() >= total,
            "into_bytes should contain at least all incremental chunks"
        );

        // 2. BufferReader 读回并解码全部帧
        let mut reader = BufferReader::new(bytes)?;
        let mut decoder = DecoderBuilder::new(MediaType::VIDEO).build_from_reader(&reader)?;
        let mut frames = 0;
        while decoder.decode_raw(&mut reader)?.is_some() {
            frames += 1;
        }
        assert_eq!(frames, 8, "expected 8 decoded frames from buffer reader");

        // 3. seek 到起点后再解一帧（验证 BufferReader 的 Seekable 实现）
        let source = {
            let writer = BufferWriter::new("mpegts")?;
            let mut muxer = Muxer::new_from_writer(writer);
            let encoder = EncoderBuilder::new_video(64, 48).build()?;
            let tb = encoder.time_base();
            let video_index = muxer.add_encoder(encoder)?;
            for i in 0..8 {
                let mut frame = generate_rgb_frame(64, 48, i);
                frame.set_pts(i);
                frame.set_time_base(tb);
                muxer.mux(frame, video_index)?;
            }
            muxer.finish()?;
            muxer.into_writer().into_bytes()
        };
        let mut reader = BufferReader::new(source)?;
        let mut decoder = DecoderBuilder::new(MediaType::VIDEO).build_from_reader(&reader)?;
        reader.seek_to_start()?;
        let frame = decoder
            .decode_raw(&mut reader)?
            .expect("expected a frame after seek to start");
        assert!(frame.width > 0 && frame.height > 0);

        Ok(())
    }

    /// Local mp4: `StreamReader`'s underlying IO is byte-seekable, so
    /// `is_byte_seekable` must be `true` and `seek_to_start` must succeed.
    #[test]
    fn test_stream_reader_local_is_byte_seekable() -> Result<()> {
        let mut reader = StreamReader::new(std::path::Path::new("assets/mp4.mp4"))?;
        assert!(
            reader.is_byte_seekable(),
            "local file IO should report byte-seekable"
        );
        reader.seek_to_start()?;
        Ok(())
    }

    /// In-memory input: `BufferReader` installs a seek callback, so FFmpeg sets
    /// `AVIO_SEEKABLE_NORMAL` and `is_byte_seekable` is always `true`.
    #[test]
    fn test_buffer_reader_is_byte_seekable() -> Result<()> {
        let data = std::fs::read("assets/mp4.mp4")?;
        let mut reader = BufferReader::new(data)?;
        assert!(
            reader.is_byte_seekable(),
            "in-memory reader with a seek callback should report byte-seekable"
        );
        reader.seek_to_start()?;
        Ok(())
    }

    /// IoReader：任意 std::io::Read 流读取媒体。
    ///
    /// 使用 mpegts（流式容器，demux 无需 seek）；moov 在文件尾部的 mp4
    /// 不适用于非 seek 输入。
    #[test]
    fn test_io_reader() -> Result<()> {
        // 先生成一个 mpegts 测试文件
        let path = crate::test_support::test_output_path("io", "rsmedia_io_reader.ts");
        crate::test_support::remove_test_output(&path);
        {
            let mut muxer = Muxer::new(&path)?;
            let encoder = EncoderBuilder::new_video(64, 48).build()?;
            let tb = encoder.time_base();
            let video_index = muxer.add_encoder(encoder)?;
            for i in 0..4 {
                let mut frame = generate_rgb_frame(64, 48, i);
                frame.set_pts(i);
                frame.set_time_base(tb);
                muxer.mux(frame, video_index)?;
            }
            muxer.finish()?;
        }

        let file = std::fs::File::open(&path)?;
        let reader = IoReader::new(std::io::BufReader::new(file))?;
        let demuxer = Demuxer::new_from_reader(reader, None, None)?;
        let n = demuxer.filter_map(|res| res.ok()).count();
        assert!(n > 0, "expected packets from IoReader");

        crate::test_support::remove_test_output(&path);
        Ok(())
    }

    /// CustomIoWriter：写任意 std::io::Write 汇（Vec<u8>），结束后取回数据。
    #[test]
    fn test_custom_io_writer() -> Result<()> {
        let writer = CustomIoWriter::new("mpegts", Vec::new())?;
        let mut muxer = Muxer::new_from_writer(writer);
        let encoder = EncoderBuilder::new_video(64, 48).build()?;
        let tb = encoder.time_base();
        let video_index = muxer.add_encoder(encoder)?;
        for i in 0..4 {
            let mut frame = generate_rgb_frame(64, 48, i);
            frame.set_pts(i);
            frame.set_time_base(tb);
            muxer.mux(frame, video_index)?;
        }
        muxer.finish()?;
        let sink = muxer.into_writer().into_inner()?;
        assert!(!sink.is_empty(), "custom io writer produced no output");

        Ok(())
    }
}
