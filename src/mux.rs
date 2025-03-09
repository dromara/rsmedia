use crate::io::{Reader, Writer};
use crate::stream::StreamInfo;
use crate::Packet;

use rsmpeg::avcodec::AVCodecParameters;

use anyhow::{Context, Result};
use std::collections::HashMap;

/// Builds a [`Muxer`].
pub struct MuxerBuilder<W: Writer> {
    writer: W,
    interleaved: bool,
    mapping: HashMap<usize, StreamInfo>,
}

impl<W: Writer> MuxerBuilder<W> {
    /// Create a new [`MuxerBuilder`].
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            interleaved: false,
            mapping: HashMap::new(),
        }
    }

    /// Add an output stream to the muxer based on an input stream from a reader. Any packets
    /// provided to [`Muxer::mux()`] from the given input stream will be muxed to the corresponding
    /// output stream.
    ///
    /// At least one stream must be added before any muxing can take place.
    ///
    /// # Arguments
    ///
    /// * `stream_info` - Stream information. Usually this information is retrieved by calling
    ///   [`Reader::stream_info()`].
    pub fn with_stream(mut self, stream_info: StreamInfo) -> Result<Self> {
        let (index, codec_parameters, reader_stream_time_base) = stream_info.into_parts();
        let writer_stream_index = unsafe {
            let mut av_stream = self.writer.output_mut().new_stream();
            av_stream.set_codecpar(AVCodecParameters::from_raw(codec_parameters));
            av_stream.set_time_base(reader_stream_time_base.into());
            av_stream.index
        };
        let stream_info = { StreamInfo::from_writer(&self.writer, writer_stream_index as usize)? };
        self.mapping.insert(index, stream_info);
        Ok(self)
    }

    /// Add output streams from reader to muxer. This will add all streams in the reader and
    /// duplicate them in the muxer. After calling this, it is safe to mux all packets from the
    /// provided reader.
    ///
    /// # Arguments
    ///
    /// * `reader` - Reader to add streams from.
    pub fn with_streams(mut self, reader: &Reader) -> Result<Self> {
        for stream in reader.input.streams() {
            let codec_type = stream.codecpar().codec_type();
            if !codec_type.is_video() && !codec_type.is_audio() && !codec_type.is_subtitle() {
                continue;
            }
            self = self.with_stream(reader.stream_info(stream.index as usize)?)?;
        }
        Ok(self)
    }

    /// This will cause the muxer to use interleaved write instead of normal write.
    pub fn with_interleaved(mut self) -> Self {
        self.interleaved = true;
        self
    }

    /// Build [`Muxer`].
    pub fn build(self) -> Muxer<W> {
        Muxer {
            writer: self.writer,
            mapping: self.mapping,
            interleaved: self.interleaved,
            have_written_header: false,
            have_written_trailer: false,
        }
    }
}

/// Represents a muxer. A muxer allows muxing media packets into a new container format. Muxing does
/// not require encoding and/or decoding.
///
/// # Examples
///
/// Mux to an MKV file:
///
/// ```rust,ignore
/// let reader = Reader::new(Path::new("from_file.mp4")).unwrap();
/// let writer = Writer::new(Path::new("to_file.mkv")).unwrap();
/// let muxer = MuxerBuilder::new(writer)
///     .with_streams(&reader)
///     .unwrap()
///     .build();
/// while let Ok(packet) = reader.read() {
///     muxer.mux(packet).unwrap();
/// }
/// muxer.finish().unwrap();
/// ```
///
/// Mux from file to MP4 and print length of first 100 buffer segments:
///
/// ```rust,ignore
/// let reader = Reader::new(Path::new("my_file.mp4")).unwrap();
/// let writer = BufferWriter::new("mp4").unwrap();
/// let mut muxer = MuxerBuilder::new(writer)
///     .with_streams(&reader)
///     .build()
///     .unwrap();
/// for _ in 0..100 {
///     println!("len: {}", muxer.mux().unwrap().len());
/// }
/// muxer.finish()?;
/// ```
pub struct Muxer<W: Writer> {
    pub(crate) writer: W,
    mapping: HashMap<usize, StreamInfo>,
    interleaved: bool,
    have_written_header: bool,
    have_written_trailer: bool,
}

impl<W: Writer> Muxer<W> {
    /// Mux a single packet. This will mux a single packet.
    ///
    /// # Arguments
    ///
    /// * `packet` - [`Packet`] to mux.
    pub fn mux(&mut self, packet: Packet) -> Result<W::Out> {
        if !self.have_written_header {
            self.have_written_header = true;
            self.writer.write_header()?;
        }

        let stream_desc = self
            .mapping
            .get(&(packet.stream_index()))
            .context("Packet stream index not found in muxer")?;

        let dst_stream = self
            .writer
            .output()
            .streams()
            .get(stream_desc.index)
            .context("Writer Stream not found in muxer")?;

        let mut pkt = packet;
        pkt.set_pos(-1);
        pkt.set_stream_index(dst_stream.index as usize);
        pkt.rescale_ts(stream_desc.time_base, dst_stream.time_base);

        let out = if self.interleaved {
            self.writer.write_interleaved(&mut pkt)?
        } else {
            self.writer.write_frame(&mut pkt)?
        };

        Ok(out)
    }

    /// Signal to the muxer that writing has finished. This will cause a trailer to be written if
    /// the container format has one.
    pub fn finish(&mut self) -> Result<Option<W::Out>> {
        if self.have_written_header && !self.have_written_trailer {
            self.have_written_trailer = true;
            self.writer.write_trailer().map(Some)
        } else {
            Ok(None)
        }
    }

    // Get parameter sets corresponding to each internal stream. The parameter set contains one SPS
    // (Sequence Parameter Set) and zero or more PPSs (Picture Parameter Sets).
    //
    // Note that this function only supports extracting parameter sets for streams with the H.264
    // codec and will return `Error::UnsupportedCodecParameterSets` for streams with another type
    // of codec.
    // pub fn parameter_sets_h264(&self) -> Vec<Result<(Sps<'_>, Pps<'_>)>> {
    //     self.writer
    //         .output()
    //         .streams()
    //         .iter().for_each(|stream| {
    //             if stream.codecpar().codec_id == ffi::AV_CODEC_ID_H264 {
    //                 extract_parameter_sets_h264(extradata(self.writer.output(), stream.index())?)
    //             } else {
    //                 Err(Error::msg("Unsupported codec parameter sets"))
    //             }
    //         })
    //         .collect::<Vec<_>>()
    // }
}

unsafe impl<W: Writer> Send for Muxer<W> {}
unsafe impl<W: Writer> Sync for Muxer<W> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::{self, BufferWriterBuilder, PacketizedBufWriterBuilder, StreamWriterBuilder};
    use crate::{Rational, Time};
    use std::collections::HashMap;
    use std::path::Path;

    #[test]
    #[ignore = "test_muxer, muxer convert mp4, mov, avi to mkv, requires a file to be present"]
    fn test_muxer() {
        let mut reader = Reader::new(Path::new("/tmp/bear.mp4")).unwrap();
        let writer = StreamWriterBuilder::new(Path::new("/tmp/bear.mov"))
            .with_format("mov")
            .build()
            .unwrap();

        let mut muxer = MuxerBuilder::new(writer)
            .with_streams(&reader)
            .unwrap()
            .build();
        while let Ok(packet) = reader.read_any() {
            muxer.mux(packet).unwrap();
        }
        muxer.finish().unwrap();
    }

    #[test]
    #[ignore = "test_rtp_muxer, muxer convert mp4, mov, avi to mkv, requires a file to be present"]
    fn test_rtp_muxer() {
        //! only have stream 0
        let mut reader = Reader::new(Path::new("/tmp/trim.mp4")).unwrap();

        let mut opts = HashMap::<String, String>::new();
        opts.insert("strict".to_string(), "experimental".to_string());
        opts.insert("rtsp_transport".to_string(), "tcp".to_string());
        opts.insert("an".to_string(), "".to_string());

        // RTP 只支持单流
        // WARNING! [rtp @ 0x15ae04e50] Only one stream supported in the RTP muxer
        let mut rtp_muxer = MuxerBuilder::new(
            PacketizedBufWriterBuilder::new("rtp")
                .with_options(&opts.into())
                .build()
                .unwrap(),
        )
        .with_streams(&reader)
        .unwrap()
        .build();

        let sdp = io::sdp(&rtp_muxer.writer.output).unwrap();
        println!("sdp: {}", sdp);
        let (seq, timestamp) = io::rtp_seq_and_timestamp(&rtp_muxer.writer.output);
        println!("seq: {}, timestamp: {}", seq, timestamp);
        let rtp_h264_mode = io::rtp_h264_mode_0(&rtp_muxer.writer.output);
        println!("rtp_h264_mode: {}", rtp_h264_mode);

        let duration = Time::from_nth_of_a_second(24);
        while let Ok(mut packet) = reader.read(0) {
            packet.set_pos(-1);
            packet.set_pts(duration);
            packet.set_dts(duration);
            packet.set_time_base(Rational::new(1, 24));
            let bufs = rtp_muxer.mux(packet).unwrap();
            println!("rtp_muxer len:{}", bufs.len())
        }
        rtp_muxer.finish().unwrap();
    }

    #[test]
    #[ignore = "test_buf_muxer, muxer convert mp4, mov, avi to mkv, requires a file to be present"]
    fn test_buf_muxer() {
        let mut reader = Reader::new(Path::new("/tmp/bear.mp4")).unwrap();
        let writer = BufferWriterBuilder::new("mp4").build().unwrap();

        let mut buf_muxer = MuxerBuilder::new(writer)
            .with_streams(&reader)
            .unwrap()
            .build();

        while let Ok(packet) = reader.read_any() {
            let buf = buf_muxer.mux(packet).unwrap();
            println!("buf_muxer len:{}", buf.len())
        }
        buf_muxer.finish().unwrap();
    }
}
