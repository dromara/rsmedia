use rsmpeg::ffi;

#[repr(u32)]
#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum AvPacketFlags {
    KEY = ffi::AV_PKT_FLAG_KEY,
    CORRUPT = ffi::AV_PKT_FLAG_CORRUPT,
    DISCARD = ffi::AV_PKT_FLAG_DISCARD,
    TRUSTED = ffi::AV_PKT_FLAG_TRUSTED,
    DISPOSABLE = ffi::AV_PKT_FLAG_DISPOSABLE,
}

#[repr(u32)]
#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum AvCodecFlags {
    UNALIGNED = ffi::AV_CODEC_FLAG_UNALIGNED,
    QSCALE = ffi::AV_CODEC_FLAG_QSCALE,
    _4MV = ffi::AV_CODEC_FLAG_4MV,
    OUTPUT_CORRUPT = ffi::AV_CODEC_FLAG_OUTPUT_CORRUPT,
    QPEL = ffi::AV_CODEC_FLAG_QPEL,
    PASS1 = ffi::AV_CODEC_FLAG_PASS1,
    PASS2 = ffi::AV_CODEC_FLAG_PASS2,
    GRAY = ffi::AV_CODEC_FLAG_GRAY,
    PSNR = ffi::AV_CODEC_FLAG_PSNR,
    // #[cfg(not(feature = "ffmpeg_6_0"))]
    // TRUNCATED       = ffi::AV_CODEC_FLAG_TRUNCATED,
    INTERLACED_DCT = ffi::AV_CODEC_FLAG_INTERLACED_DCT,
    LOW_DELAY = ffi::AV_CODEC_FLAG_LOW_DELAY,
    GLOBAL_HEADER = ffi::AV_CODEC_FLAG_GLOBAL_HEADER,
    BITEXACT = ffi::AV_CODEC_FLAG_BITEXACT,
    AC_PRED = ffi::AV_CODEC_FLAG_AC_PRED,
    LOOP_FILTER = ffi::AV_CODEC_FLAG_LOOP_FILTER,
    INTERLACED_ME = ffi::AV_CODEC_FLAG_INTERLACED_ME,
    CLOSED_GOP = ffi::AV_CODEC_FLAG_CLOSED_GOP,
}

#[repr(u32)]
#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum AvFormatFlags {
    NO_FILE = ffi::AVFMT_NOFILE,
    NEED_NUMBER = ffi::AVFMT_NEEDNUMBER,
    SHOW_IDS = ffi::AVFMT_SHOW_IDS,
    // #[cfg(not(feature = "ffmpeg_4_0"))]
    // RAW_PICTURE   = ffi::AVFMT_RAWPICTURE,
    GLOBAL_HEADER = ffi::AVFMT_GLOBALHEADER,
    NO_TIMESTAMPS = ffi::AVFMT_NOTIMESTAMPS,
    GENERIC_INDEX = ffi::AVFMT_GENERIC_INDEX,
    TS_DISCONT = ffi::AVFMT_TS_DISCONT,
    VARIABLE_FPS = ffi::AVFMT_VARIABLE_FPS,
    NO_DIMENSIONS = ffi::AVFMT_NODIMENSIONS,
    NO_STREAMS = ffi::AVFMT_NOSTREAMS,
    NO_BINSEARCH = ffi::AVFMT_NOBINSEARCH,
    NO_GENSEARCH = ffi::AVFMT_NOGENSEARCH,
    NO_BYTE_SEEK = ffi::AVFMT_NO_BYTE_SEEK,
    ALLOW_FLUSH = ffi::AVFMT_ALLOW_FLUSH,
    TS_NONSTRICT = ffi::AVFMT_TS_NONSTRICT,
    TS_NEGATIVE = ffi::AVFMT_TS_NEGATIVE,
    SEEK_TO_PTS = ffi::AVFMT_SEEK_TO_PTS,
}

#[repr(u32)]
#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum AvFormatContextFlags {
    GENPTS = ffi::AVFMT_FLAG_GENPTS,
    IGNIDX = ffi::AVFMT_FLAG_IGNIDX,
    NONBLOCK = ffi::AVFMT_FLAG_NONBLOCK,
    IGNDTS = ffi::AVFMT_FLAG_IGNDTS,
    NOFILLIN = ffi::AVFMT_FLAG_NOFILLIN,
    NOPARSE = ffi::AVFMT_FLAG_NOPARSE,
    NOBUFFER = ffi::AVFMT_FLAG_NOBUFFER,
    CUSTOM_IO = ffi::AVFMT_FLAG_CUSTOM_IO,
    DISCARD_CORRUPT = ffi::AVFMT_FLAG_DISCARD_CORRUPT,
    FLUSH_PACKETS = ffi::AVFMT_FLAG_FLUSH_PACKETS,
    BITEXACT = ffi::AVFMT_FLAG_BITEXACT,
    SORT_DTS = ffi::AVFMT_FLAG_SORT_DTS,
    FAST_SEEK = ffi::AVFMT_FLAG_FAST_SEEK,
    SHORTEST = ffi::AVFMT_FLAG_SHORTEST,
    AUTO_BSF = ffi::AVFMT_FLAG_AUTO_BSF,
}

#[repr(u32)]
#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum AvScalerFlags {
    FAST_BILINEAR = ffi::SWS_FAST_BILINEAR,
    BILINEAR = ffi::SWS_BILINEAR,
    BICUBIC = ffi::SWS_BICUBIC,
    X = ffi::SWS_X,
    POINT = ffi::SWS_POINT,
    AREA = ffi::SWS_AREA,
    BICUBLIN = ffi::SWS_BICUBLIN,
    GAUSS = ffi::SWS_GAUSS,
    SINC = ffi::SWS_SINC,
    LANCZOS = ffi::SWS_LANCZOS,
    SPLINE = ffi::SWS_SPLINE,
    SRC_V_CHR_DROP_MASK = ffi::SWS_SRC_V_CHR_DROP_MASK,
    // alias POINT=16
    // SRC_V_CHR_DROP_SHIFT = ffi::SWS_SRC_V_CHR_DROP_SHIFT,
    PARAM_DEFAULT = ffi::SWS_PARAM_DEFAULT,
    PRINT_INFO = ffi::SWS_PRINT_INFO,
    FULL_CHR_H_INT = ffi::SWS_FULL_CHR_H_INT,
    FULL_CHR_H_INP = ffi::SWS_FULL_CHR_H_INP,
    DIRECT_BGR = ffi::SWS_DIRECT_BGR,
    ACCURATE_RND = ffi::SWS_ACCURATE_RND,
    BITEXACT = ffi::SWS_BITEXACT,
    ERROR_DIFFUSION = ffi::SWS_ERROR_DIFFUSION,
}

#[repr(u32)]
#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum AvDispositionFlags {
    DEFAULT = ffi::AV_DISPOSITION_DEFAULT,
    DUB = ffi::AV_DISPOSITION_DUB,
    ORIGINAL = ffi::AV_DISPOSITION_ORIGINAL,
    COMMENT = ffi::AV_DISPOSITION_COMMENT,
    LYRICS = ffi::AV_DISPOSITION_LYRICS,
    KARAOKE = ffi::AV_DISPOSITION_KARAOKE,
    FORCED = ffi::AV_DISPOSITION_FORCED,
    HEARING_IMPAIRED = ffi::AV_DISPOSITION_HEARING_IMPAIRED,
    VISUAL_IMPAIRED = ffi::AV_DISPOSITION_VISUAL_IMPAIRED,
    CLEAN_EFFECTS = ffi::AV_DISPOSITION_CLEAN_EFFECTS,
    ATTACHED_PIC = ffi::AV_DISPOSITION_ATTACHED_PIC,
    CAPTIONS = ffi::AV_DISPOSITION_CAPTIONS,
    DESCRIPTIONS = ffi::AV_DISPOSITION_DESCRIPTIONS,
    METADATA = ffi::AV_DISPOSITION_METADATA,
    #[cfg(feature = "ffmpeg7")]
    MULTILAYER = ffi::AV_DISPOSITION_MULTILAYER,
}

#[repr(i32)]
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum MediaType {
    UNKNOWN = ffi::AVMEDIA_TYPE_UNKNOWN,
    VIDEO = ffi::AVMEDIA_TYPE_VIDEO,
    AUDIO = ffi::AVMEDIA_TYPE_AUDIO,
    DATA = ffi::AVMEDIA_TYPE_DATA,
    SUBTITLE = ffi::AVMEDIA_TYPE_SUBTITLE,
    ATTACHMENT = ffi::AVMEDIA_TYPE_ATTACHMENT,
}

impl From<ffi::AVMediaType> for MediaType {
    fn from(item: ffi::AVMediaType) -> Self {
        match item {
            ffi::AVMEDIA_TYPE_UNKNOWN => MediaType::UNKNOWN,
            ffi::AVMEDIA_TYPE_VIDEO => MediaType::VIDEO,
            ffi::AVMEDIA_TYPE_AUDIO => MediaType::AUDIO,
            ffi::AVMEDIA_TYPE_DATA => MediaType::DATA,
            ffi::AVMEDIA_TYPE_SUBTITLE => MediaType::SUBTITLE,
            ffi::AVMEDIA_TYPE_ATTACHMENT => MediaType::ATTACHMENT,
            _ => panic!("Invalid media type"),
        }
    }
}

#[repr(i32)]
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum SampleFormat {
    /// < none
    NONE = ffi::AV_SAMPLE_FMT_NONE,
    /// < unsigned 8 bits
    U8 = ffi::AV_SAMPLE_FMT_U8,
    /// < signed 16 bits
    S16 = ffi::AV_SAMPLE_FMT_S16,
    /// < signed 32 bits
    S32 = ffi::AV_SAMPLE_FMT_S32,
    /// < float
    FLT = ffi::AV_SAMPLE_FMT_FLT,
    /// < double
    DBL = ffi::AV_SAMPLE_FMT_DBL,
    /// < unsigned 8 bits, planar
    U8P = ffi::AV_SAMPLE_FMT_U8P,
    /// < signed 16 bits, planar
    S16P = ffi::AV_SAMPLE_FMT_S16P,
    /// < signed 32 bits, planar
    S32P = ffi::AV_SAMPLE_FMT_S32P,
    /// < float, planar
    FLTP = ffi::AV_SAMPLE_FMT_FLTP,
    /// < double, planar
    DBLP = ffi::AV_SAMPLE_FMT_DBLP,
    /// < signed 64 bits
    S64 = ffi::AV_SAMPLE_FMT_S64,
    /// < signed 64 bits, planar
    S64P = ffi::AV_SAMPLE_FMT_S64P,
}

impl From<ffi::AVSampleFormat> for SampleFormat {
    fn from(item: ffi::AVSampleFormat) -> Self {
        match item {
            ffi::AV_SAMPLE_FMT_NONE => SampleFormat::NONE,
            ffi::AV_SAMPLE_FMT_U8 => SampleFormat::U8,
            ffi::AV_SAMPLE_FMT_S16 => SampleFormat::S16,
            ffi::AV_SAMPLE_FMT_S32 => SampleFormat::S32,
            ffi::AV_SAMPLE_FMT_FLT => SampleFormat::FLT,
            ffi::AV_SAMPLE_FMT_DBL => SampleFormat::DBL,
            ffi::AV_SAMPLE_FMT_U8P => SampleFormat::U8P,
            ffi::AV_SAMPLE_FMT_S16P => SampleFormat::S16P,
            ffi::AV_SAMPLE_FMT_S32P => SampleFormat::S32P,
            ffi::AV_SAMPLE_FMT_FLTP => SampleFormat::FLTP,
            ffi::AV_SAMPLE_FMT_DBLP => SampleFormat::DBLP,
            ffi::AV_SAMPLE_FMT_S64 => SampleFormat::S64,
            ffi::AV_SAMPLE_FMT_S64P => SampleFormat::S64P,
            _ => panic!("Invalid sample format"),
        }
    }
}
