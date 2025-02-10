use libc::{c_int, c_uint};
use bitflags::bitflags;
use rsmpeg::ffi;

bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub struct AvPacketFlags: c_uint {
        const KEY     = ffi::AV_PKT_FLAG_KEY;
        const CORRUPT = ffi::AV_PKT_FLAG_CORRUPT;
        const DISCARD = ffi::AV_PKT_FLAG_DISCARD;
        const TRUSTED = ffi::AV_PKT_FLAG_TRUSTED;
        const DISPOSABLE = ffi::AV_PKT_FLAG_DISPOSABLE;
    }
}

bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub struct AvCodecFlags: c_uint {
        const UNALIGNED       = ffi::AV_CODEC_FLAG_UNALIGNED;
        const QSCALE          = ffi::AV_CODEC_FLAG_QSCALE;
        const _4MV            = ffi::AV_CODEC_FLAG_4MV;
        const OUTPUT_CORRUPT  = ffi::AV_CODEC_FLAG_OUTPUT_CORRUPT;
        const QPEL            = ffi::AV_CODEC_FLAG_QPEL;
        const PASS1           = ffi::AV_CODEC_FLAG_PASS1;
        const PASS2           = ffi::AV_CODEC_FLAG_PASS2;
        const GRAY            = ffi::AV_CODEC_FLAG_GRAY;
        const PSNR            = ffi::AV_CODEC_FLAG_PSNR;
        // #[cfg(not(feature = "ffmpeg_6_0"))]
        // const TRUNCATED       = ffi::AV_CODEC_FLAG_TRUNCATED;
        const INTERLACED_DCT  = ffi::AV_CODEC_FLAG_INTERLACED_DCT;
        const LOW_DELAY       = ffi::AV_CODEC_FLAG_LOW_DELAY;
        const GLOBAL_HEADER   = ffi::AV_CODEC_FLAG_GLOBAL_HEADER;
        const BITEXACT        = ffi::AV_CODEC_FLAG_BITEXACT;
        const AC_PRED         = ffi::AV_CODEC_FLAG_AC_PRED;
        const LOOP_FILTER     = ffi::AV_CODEC_FLAG_LOOP_FILTER;
        const INTERLACED_ME   = ffi::AV_CODEC_FLAG_INTERLACED_ME;
        const CLOSED_GOP      = ffi::AV_CODEC_FLAG_CLOSED_GOP;
    }
}

bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub struct AvFormatFlags: c_uint {
        const NO_FILE       = ffi::AVFMT_NOFILE;
        const NEED_NUMBER   = ffi::AVFMT_NEEDNUMBER;
        const SHOW_IDS      = ffi::AVFMT_SHOW_IDS;
        // #[cfg(not(feature = "ffmpeg_4_0"))]
        // const RAW_PICTURE   = ffi::AVFMT_RAWPICTURE;
        const GLOBAL_HEADER = ffi::AVFMT_GLOBALHEADER;
        const NO_TIMESTAMPS = ffi::AVFMT_NOTIMESTAMPS;
        const GENERIC_INDEX = ffi::AVFMT_GENERIC_INDEX;
        const TS_DISCONT    = ffi::AVFMT_TS_DISCONT;
        const VARIABLE_FPS  = ffi::AVFMT_VARIABLE_FPS;
        const NO_DIMENSIONS = ffi::AVFMT_NODIMENSIONS;
        const NO_STREAMS    = ffi::AVFMT_NOSTREAMS;
        const NO_BINSEARCH  = ffi::AVFMT_NOBINSEARCH;
        const NO_GENSEARCH  = ffi::AVFMT_NOGENSEARCH;
        const NO_BYTE_SEEK  = ffi::AVFMT_NO_BYTE_SEEK;
        const ALLOW_FLUSH   = ffi::AVFMT_ALLOW_FLUSH;
        const TS_NONSTRICT  = ffi::AVFMT_TS_NONSTRICT;
        const TS_NEGATIVE   = ffi::AVFMT_TS_NEGATIVE;
        const SEEK_TO_PTS   = ffi::AVFMT_SEEK_TO_PTS;
    }
}

bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub struct AvScalerFlags: c_uint {
        const FAST_BILINEAR        = ffi::SWS_FAST_BILINEAR;
        const BILINEAR             = ffi::SWS_BILINEAR;
        const BICUBIC              = ffi::SWS_BICUBIC;
        const X                    = ffi::SWS_X;
        const POINT                = ffi::SWS_POINT;
        const AREA                 = ffi::SWS_AREA;
        const BICUBLIN             = ffi::SWS_BICUBLIN;
        const GAUSS                = ffi::SWS_GAUSS;
        const SINC                 = ffi::SWS_SINC;
        const LANCZOS              = ffi::SWS_LANCZOS;
        const SPLINE               = ffi::SWS_SPLINE;
        const SRC_V_CHR_DROP_MASK  = ffi::SWS_SRC_V_CHR_DROP_MASK;
        const SRC_V_CHR_DROP_SHIFT = ffi::SWS_SRC_V_CHR_DROP_SHIFT;
        const PARAM_DEFAULT        = ffi::SWS_PARAM_DEFAULT;
        const PRINT_INFO           = ffi::SWS_PRINT_INFO;
        const FULL_CHR_H_INT       = ffi::SWS_FULL_CHR_H_INT;
        const FULL_CHR_H_INP       = ffi::SWS_FULL_CHR_H_INP;
        const DIRECT_BGR           = ffi::SWS_DIRECT_BGR ;
        const ACCURATE_RND         = ffi::SWS_ACCURATE_RND;
        const BITEXACT             = ffi::SWS_BITEXACT;
        const ERROR_DIFFUSION      = ffi::SWS_ERROR_DIFFUSION;
    }
}

bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub struct AvDispositionFlags: c_uint {
        const DEFAULT          = ffi::AV_DISPOSITION_DEFAULT;
        const DUB              = ffi::AV_DISPOSITION_DUB;
        const ORIGINAL         = ffi::AV_DISPOSITION_ORIGINAL;
        const COMMENT          = ffi::AV_DISPOSITION_COMMENT;
        const LYRICS           = ffi::AV_DISPOSITION_LYRICS;
        const KARAOKE          = ffi::AV_DISPOSITION_KARAOKE;
        const FORCED           = ffi::AV_DISPOSITION_FORCED;
        const HEARING_IMPAIRED = ffi::AV_DISPOSITION_HEARING_IMPAIRED;
        const VISUAL_IMPAIRED  = ffi::AV_DISPOSITION_VISUAL_IMPAIRED;
        const CLEAN_EFFECTS    = ffi::AV_DISPOSITION_CLEAN_EFFECTS;
        const ATTACHED_PIC     = ffi::AV_DISPOSITION_ATTACHED_PIC;
        const CAPTIONS         = ffi::AV_DISPOSITION_CAPTIONS;
        const DESCRIPTIONS     = ffi::AV_DISPOSITION_DESCRIPTIONS;
        const METADATA         = ffi::AV_DISPOSITION_METADATA;
        // #[cfg(feature = "ffmpeg_7_1")]
        const MULTILAYER       = ffi::AV_DISPOSITION_MULTILAYER;
    }
}