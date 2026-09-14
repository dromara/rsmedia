# rsmedia 核心代码审查报告

> 审查范围：`src/` 全部核心模块
> 审查视角：1) 设计优雅 / 复杂度 / 冗余 / 测试覆盖；2) 全局音视频处理逻辑与功能链；3) 对外 API 合理性与使用的友好性
> 状态：本轮已完成缺陷修复与核心测试补充；以下为审查结论与后续建议

---

## 一、全局视角：整个框架处理音视频的逻辑流程与功能

### 1.1 总体架构

框架是一条对称的「视频 / 音频」双轨管线，围绕 FFmpeg 的 codec / format / filter / swscale / swresample
五个子系统封装，顶层通过 Builder 模式把参数注入到 `Decoder` / `Encoder` / `Muxer` / `Demuxer`。

```
输入源(Zen Layer Path/URL/Buffer)
        │
        │ Demuxer<R: Reader>（解封装）
        ▼
  ┌───────────────┬────────────────┐
  │ 视频流        │ 音频流         │
  ▼               ▼
Decoder   ──Video──>  SwScaler(Scaler) ┐
Decoder(音频) ───> SwResample(Resampler) ├─> FilterGraph ─> Encoder
        │                                │                    │
        │            VideoFrame/AudioID ┘                    Muxer<W: Writer>
        ▼                                                     │
  MediaFrame / PixelBuffer / ndarray / DynamicImage           ▼
                                                       输出文件/Stream
```

### 1.2 关键数据流

- **解封装** `Demuxer<R>`：io 层抽象出 [io.rs](file:///Users/admin/Workspace/rust/github.com/rsmedia/src/io.rs) 的 `Reader`/`Writer`，`find_best_stream` 定位音视频流，`StreamInfo::from_reader` 缓存流参数。
- **解码** `Decoder`：`decode()` 产出一帧，`scale_decoded_frame` 在「仅当格式/尺寸不匹配」时用 `Scaler` 转换（本轮已用 `scale_if_needed` 消除重复判定，见 [scale.rs](file:///Users/admin/Workspace/rust/github.com/rsmedia/src/scale.rs)）。
- **编码** `Encoder`：`send_frame_*` 依次做滤镜输入格式转换（video→`Scaler`，audio→`Resampler.convert_frame`）、滤镜图 `process_frame`、解码器 `send/receive`，最后交给 `Muxer`。
- **复用/复用** `Muxer<W>`：`add_stream`/`add_encoder` 绑定流，`mux` 写包，`write_header/finish` 结束时 `apply_chapters` 注入章节、`refresh_stream_info` 回读时间基。
- **缩放/重采样**：`Scaler`（持久化 `SwsContext`）、`Resampler`（持久化 `SwrContext`），两者都对「参数变化自动重建」做了惰性管理，是本框架的关键抽象。

### 1.3 功能覆盖

- 视频：`scale/crop/fade/hflip/vflip/transpose/drawtext/drawbox/delogo/pad/eq/subtitles/yadif/gif_palette` 等（[filter.rs](file:///Users/admin/Workspace/rust/github.com/rsmedia/src/filter.rs)）
- 音频：`resample/format/volume/loudnorm/equalizer/compressor/highpass/lowpass/atempo/adelay/fft_denoise/denoise/anlm_denoise` 等
- 格式：MP4/MKV/GIF 等；章节（Chapter）在 MP4 与 MKV 都已验证可回读
- 硬件：`hwaccel` 下载硬件帧

---

## 二、用户视角：对外 API 合理性与使用友好性

### 2.1 做得好的

- **Builder 模式统一**：`DecoderBuilder` / `EncoderBuilder` 采用链式 `.with_*()` 返回 `Self`，之后一次性 `build()` 返回 `Result`，参数校验从「编译期 assert」迁到「build 期 Result」（本轮已改），错误不再静默 panic。
- **缩放参数演进**：`Scaler::new_with_options(algorithm, &[quality])` + `Encoder/Decoder` 持有 `Scaler`，用户只需 set 一次算法与质量，无需每次处理帧重复传参。
- **类型安全**：`ScaleAlgorithm` / `ScaleQuality` 自定义枚举替代裸 `u32` 位标志，降低误用。
- **错误类型统一**：`RsmediaError` + `Context` trait、`cstr_to_string_lossy` 处理非关键 codec 名，公共路径已消除静默 `unwrap`。

### 2.2 可改进（后续建议，不影响本轮安全）

| 问题 | 现状 | 建议 |
|---|---|---|
| **命名不一致** | `with_pix_fmt` / `with_pixel_format` 混用；`with_sample_format`/`with_sample_fmt` | 统一为 `_format` 一族，暴露别名并标记废弃 |
| **尺寸类型混杂** | Builder 宽高 `u32`/`usize`/`i32` 混用 | 统一 `u32`，内部转换收窄到 FFmpeg `int` |
| **Builder 与原始构造并存** | 大量 `Encoder::new_*` 直构 + Builder 并存，入口分裂 | 收敛入口，Builder 作为唯一直观入口 |
| **Option 串链不足** | 部分字段 `Option`，部分默认值硬编码，语义不统一 | 明确「未设置 = FFmpeg 默认」的语义，集中文档 |
| **`MediaFrame` 音频** | 音频单帧限制，未处理跨帧缓冲 | 文档化约束或提供帧缓存层 |
| **`filter` 声明式 vs 底层** | `Filter`（组合）与 `FilterGraph`（底层）并存 | 明确分层，避免用户直面 `VideoParams/AudioParams` |

---

## 三、设计优雅 / 复杂度 / 冗余 / 测试覆盖

### 3.1 本轮已优化

- **消除重复逻辑**：decode/encode 中三处「人肉判定是否需转换」抽取为 `Scaler::scale_if_needed`（[scale.rs](file:///Users/admin/Workspace/rust/github.com/rsmedia/src/scale.rs)），单帧匹配格式/尺寸时零成本直通。
- **消除静默 panic**：io / stream / encode 公共路径的 `unwrap()`/`expect()` 改为 `?` + `cstr_to_string_lossy` / let-else，仅保留确认不可达的 fail-fast（如 `into_bytes`，均有注释）。
- **Builder 校验从 assert 迁到 Result**：`with_nb_channels` / `with_sample_rate` 不再 panic，build 期返回错误。
- **补齐顶层导出**：`Muxer` / `Demuxer` 提到 crate 顶层，与 `Decoder`/`Encoder` 一致（[lib.rs](file:///Users/admin/Workspace/rust/github.com/rsmedia/src/lib.rs)）。

### 3.2 本轮新增测试

| 测试 | 覆盖点 | 文件 |
|---|---|---|
| `test_scale_if_needed_noops_on_matching_format_and_size` | 缩放 no-op 与真实转换 | scale.rs |
| `test_filtergraph_process_frame_hflip` | 真实 `FilterGraph.process_frame` 独立路径：单帧进/单帧出、hflip 像素级断言、状态机 | filter.rs |
| `test_filter_graph_process_frame_audio` | 音频 `abuffer/abuffersink` + aformat 独立路径 | filter.rs |
| 既有 `test_scaler_quality_mask_*`、`test_scaler_rebinds_when_geometry_changes` | Scaler 重建、质量位 | scale.rs |
| 既有 `test_mux_chapters` | `apply_chapters` 内存安全 + MP4/MKV 章节回读（已核验） | mux.rs |

`cargo test --lib`：**210 passed**；`cargo clippy --lib --tests`：**无警告**。

### 3.3 核查后保持原样（安全性已确认）

- `apply_chapters` 的手动内存管理：用 `Vec<*mut AVChapter>` 暂存所有权 + 失败路径 `drain` 统一释放，FFmpeg 在 `avformat_free_context` 统一释放移交的数组与节点，**本身已安全**，无需 RAII 化。
- `scale.rs:584` `expect("bound above")`：逻辑上不可达不变基，属合理 `expect`。

### 3.4 遗留的中低风险盲区（后续补充）

- `imgutils` 的 `check_image_size`/`apply_cropping`/`fill_color` 已有覆盖，部分 `fill_plane_from_buffer` 边界已测，但**未对准误**的 `get_linesize` 负数宽度等极端用例仍可加。
- `hwaccel` 像素内容正确性在 GPU 环境未校验回读。
- `filter.rs` 的 `anlm_denoise` 有已知 upstream 堆越界警告（已文档化），需在使用文档强调存样本数为窗口整数倍的约束。
- `MemType` FFI fallback 默认值相关分支依赖 FFmpeg 行为，建议在文档标注只对 `ffmpeg8/9`。

---

## 四、总体结论

- **设计**：整体模块划分清晰，编码/解码/复用/滤镜分层合理；`Scaler`/`Resampler` 的惰性重建是亮点。冗余点（刻度判断、Builder assert、裸 unwrap）已在本轮收敛。
- **API**：Builder 链式 + `Result` 校验带来较好的使用友好性；命名一致性（`pix_fmt` vs `pixel_format`、尺寸类型）是主要遗留的国内外务项。
- **测试**：核心路径（解码/编码/缩放/重采样/滤镜/复用+章节）均有真实 FFmpeg 验证，本轮新增滤镜独立路径与缩放 no-op，覆盖度达到「核心功能严格验证」的最低要求；仍存在港中等风险由后续补充。

- **通用性缺口** ：目前对"多流同时编解码"、"动态添加流"、以及回调式可中断的`demux` /`mux` （如边解码边返回中间数据）支持有限。对通用需求（如 DASH/HLS 分段、多路流复用）是架构扩展点，不算缺陷但值得列入规划。

---

## 五、帧数据模型重构（`FrameData` / `DataLayout`）的遗留改进项

> **本轮已修**：
> ① 构造点新增元素宽度校验 —— `MediaFrame::validated` 现在同时校验平面形状与 `T` 的宽度，
> 不再等到 `to_avframe` 才报错（此前 `FrameData::<u8>` 能造出 10bit 格式或 `S16` 的帧）。
> ② 4 个格式专用转换（`rgb24_to_yuv420p` / `yuv420p_to_rgb24` / `to_dynamic_image` /
> `from_dynamic_image`）已从 `FrameData` 移回 `MediaFrame`，消除了「`(H, W, 3)` 无法区分
> RGB24 与 BGR24」导致的**静默通道错位**：格式是 `MediaFrame` 的固有属性，
> 只凭采样数组推断必然有歧义。
>
> 以下是**有意留下、暂不处理**的项，按优先级排列，供后续择机处理。

### 5.1 命名：`DataLayout` 的两个平面访问器是一对易混双胞胎

- **现象**：`plane_extent(i)` 把交错布局的 component 轴折进列（`rows x cols*components`），
  `plane_shape(i)` 不折（`rows x cols`）。两者只差一个乘法，名字只差一个单词。
- **影响**：取错不报错，只是形状不对（例如把 `cols*3` 当作 `cols` 去分配缓冲）。
- **建议**：`plane_extent` → `plane_row_extent`，或在两处文档首句写明"折 / 不折"并互链。
- **成本**：纯改名，约 6 处调用点。

### 5.2 `PixelFormat::has_data_layout()` 用 `data_layout(2, 2)` 做探针

- **现象**：用一个 magic size 调用另一个方法来判断"格式能否建模"。
- **影响**：结论正确（布局的存在性只取决于格式，与尺寸无关），但读起来像 hack；
  将来若出现"仅在特定尺寸下可表达"的格式，该假设会静默失效。
- **建议**：直接判 `AV_PIX_FMT_FLAG_BITSTREAM | AV_PIX_FMT_FLAG_PAL | AV_PIX_FMT_FLAG_HWACCEL`
  （与 `data_layout` 的前置条件同源），或至少把 `(2, 2)` 提为具名常量并注明理由。
- **成本**：约 10 行。

### 5.3 `DataLayout::Interleaved.components` 字段名

- **现象**：对音频它其实是**声道数**，不是"分量数"。
- **影响**：仅命名层面的轻微误导（字段文档已注明两种含义）。
- **建议**：更中立可改 `unit_elements`，但可读性下降 —— 故**倾向保留**，仅记录。

### 5.4 `FrameData::plane_samples` 的 "sample"

- **现象**：此处 sample 是"平面的元素"这一通用义，不是音频专有语义。
- **影响**：无（不构成"以为只能音频"的误导），仅记录。

### 5.5「零格式名分支」的边界必须说清

- **现状**：**搬运层**（`data_layout` / `read_plane` / `write_plane` / `read_samples` /
  `write_samples`）确实零格式名；但**转换层**仍有格式名分支：
  `check_format(FrameFormat::Pixel(PixelFormat::RGB24), …)` 与
  `…(PixelFormat::YUV420P)` 各一处，外加 `rgb24_to_yuv420p` / `yuv420p_to_rgb24` 两个专用函数。
- **影响**：对外表述若说成"全仓库无格式名分支"并不准确。
- **建议**：需要任意格式对转换时走 swscale（`rsmpeg` 的 `AVFrame` 转换或本仓库 `Scaler`），
  `yuv` crate 仅保留 4:2:0 快速路径。

### 5.6 转换能力目前只覆盖 `YUV420P` ↔ `RGB24`

- **现状**：`MediaFrame` 上只有这两条转换路径。`YUV422P` / `YUV444P` / `NV12` / `GBRP` / 10bit 等
  格式 `data_layout` 虽已能表达，但没有内建像素格式转换（需借道 `decode` 的 swscale 或 `filter`）。
- **建议**：列入规划，实现方式见 5.5。

### 5.7（可选，激进）用 newtype 让形状歧义在类型层面消失

- **思路**：`Rgb24<Array3<u8>>` / `Yuv420p<[Array2<u8>; 3]>` 之类，把格式编进类型，
  使格式专用转换在编译期就对格式无误。
- **影响**：彻底消除语义歧义；但与"一个通用 `FrameData`"的目标冲突，API 面积显著变大。
- **结论**：**不建议**，仅作为"格式专用 API 继续膨胀"时的备选。

### 5.8 【已修复】`stream.rs` 把「未知像素格式」当成硬错误，导致自己写的文件打不开

- **现象**：容器矩阵扫到裸 h264 基本流时，读回阶段失败：`No pix_fmt descriptor for unknown`。
- **根因**：`src/stream.rs` 的 `from_stream` 对视频流无条件
  `PixelFormat::from(codecpar.format).descriptor()?` —— 裸流（`.h264`）的
  `AVCodecParameters.format` 是 `AV_PIX_FMT_NONE`，`descriptor()` 返回 `Err`，
  于是**打开自己刚写的文件**就报错；而同一函数 3 行之前已把 NONE 当"未知但合法"占位。
- **修复**：未知格式时 `pix_fmt_desc = descriptor().ok()`，`bits_per_pixel` 等派生量
  退化为 0，不再整体失败。回归测试：矩阵中的 `h264` / `h265` 裸流行（写入→打开→解码全链路）。
- **状态**：✅ 已修复并验证（2026-09-14）。

### 5.9 【已修复】音频解码没有像视频那样的「输出格式统一」

- **现象**：`decode::<f32>` 对 AAC/AC-3/MP3 可用，对 MP2 失败：
  `format:[Sample:s16p], expected 2, got 4`。调用方必须先读 `codecpar().format`
  才能决定元素类型。
- **修复**：
  1. 新增 `DecoderBuilder::with_sample_fmt(SampleFormat)`（与 `with_pix_fmt` 对称，
     仅音频有效、跨类型构建时报错、`NONE` 拒绝）：解码帧在进滤镜图之前经 swresample
     统一到目标格式；**默认仍为编解码器原生格式**（保住无损解码与位精确性，零开销）。
     统一后任何音频文件都能 `decode::<f32>()`。
  2. 连带修复重采样器对 `AV_CHANNEL_ORDER_UNSPEC` 布局帧的拒绝：无声道掩码的 WAV
     解码帧布局是 UNSPEC，`swr_alloc_set_opts2` 会把上下文布局归一化为默认值，
     `swr_convert` 随即以 `AVERROR_INPUT/OUTPUT_CHANGED` 拒绝每一帧。
     `resample.rs` 现在把 UNSPEC 布局按 FFmpeg 惯例解释为声道数的默认布局
     （输入帧与输出布局两侧都归一化）。
  3. 回归测试：`decode::tests::test_decode_audio_with_sample_fmt_unifies_output`
     （assets/wav.wav 原生 S16 vs 统一 FLTP，样本总量一致）、
     `test_decode_builder_sample_fmt_validation`、
     `resample::tests::test_convert_frame_with_unspec_channel_layout`、
     容器矩阵对全部音频容器跑「原生 + 统一」两遍并比对样本数/峰值。
- **状态**：✅ 已修复并验证（2026-09-14）。

### 5.10 【新发现，待定】编码器默认假设容器要 global header，裸流输出丢失参数集

- **现象**：用默认 `EncoderBuilder` 写裸 h264（`.h264`），文件里**没有 SPS/PPS**
  （NAL 序列只有 SEI+IDR），ffprobe/ffmpeg 均无法解码；ffmpeg CLI 自己写的裸流
  每个关键帧都带 SPS/PPS，可正常解码。
- **根因**：`EncoderBuilder::default()` 的 `ofmt_flag = AVFMT_GLOBALHEADER`
  （`encode.rs`），构建时无条件给编码器加 `AV_CODEC_FLAG_GLOBAL_HEADER` ——
  参数集进 extradata。mp4/mkv 等容器会写 extradata 所以没事；**裸流 muxer 不写
  extradata**，参数集就丢了。`with_oformat_flags` 的文档声称"由 muxer 派生"，
  实际 `Muxer` 并未派生（文档与现实不符）。
- **候选方案**（需要定夺）：
  1. `Muxer` 提供容器 flags 的查询，`add_encoder` 接受 builder 并在构建前注入
     `AVFMT_*` flags（符合文档承诺，需加一个 API）；
  2. 默认改为不设置 GLOBAL_HEADER（容器文件每个关键帧多带一份参数集，略大）；
  3. 维持现状 + 文档写明，裸流输出方显式 `with_oformat_flags(NO_TIMESTAMPS)`
     （容器矩阵的 `raw_spec` 行即此写法）。
- **状态**：⏸️ 待定。

