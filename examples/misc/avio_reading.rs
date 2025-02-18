//! Demo of custom IO using `AVIOContextCustom`.
use super::avio;
use anyhow::{Context, Result};
use rsmpeg::{
    avformat::{AVFormatContextInput, AVIOContextContainer, AVIOContextCustom},
    avutil::{AVMem, AVMmap},
    error::RsmpegError,
    ffi,
};
use std::ffi::CStr;
use std::sync::atomic::{self, AtomicI32};

pub fn avio_reading(file_path: &CStr) -> Result<()> {
    let (video_stream_index, mut input_format_context, mut decode_context) =
        avio::open_input_file(file_path)?;

    loop {
        let packet = match input_format_context.read_packet() {
            Ok(Some(x)) => x,
            // No more frames
            Ok(None) => break,
            Err(e) => panic!("Read frame error: {:?}", e),
        };

        if packet.stream_index as usize != video_stream_index {
            continue;
        }

        decode_context
            .send_packet(Some(&packet))
            .context("Send packet failed")?;

        let frame_index = AtomicI32::new(0);
        loop {
            let mut frame = match decode_context.receive_frame() {
                Ok(frame) => frame,
                Err(RsmpegError::DecoderDrainError) | Err(RsmpegError::DecoderFlushedError) => {
                    break
                }
                Err(e) => panic!("{}", e),
            };

            frame.set_pts(frame.best_effort_timestamp);

            // avio::pgm_save(&frame, &format!("{}/frame_{}.pgm", "/tmp", frame_index.fetch_add(1, atomic::Ordering::SeqCst)))?;
            avio::save_avframe_to_image(
                &frame,
                &format!(
                    "{}/frame_{}.jpeg",
                    "/tmp",
                    frame_index.fetch_add(1, atomic::Ordering::SeqCst)
                ),
            )?;
        }
    }

    Ok(())
}

fn avio_file_reading(filename: &CStr) -> Result<()> {
    let mmap = AVMmap::new(filename)?;
    let mut current = 0;

    let io_context = AVIOContextCustom::alloc_context(
        AVMem::new(4096),
        false,
        vec![],
        Some(Box::new(move |_, buf| {
            let right = mmap.len().min(current + buf.len());
            if right <= current {
                return ffi::AVERROR_EOF;
            }
            let read_len = right - current;
            buf[0..read_len].copy_from_slice(&mmap[current..right]);
            current = right;
            read_len as i32
        })),
        None,
        None,
    );

    let mut input_format_context =
        AVFormatContextInput::from_io_context(AVIOContextContainer::Custom(io_context))?;
    input_format_context.dump(0, filename)?;

    Ok(())
}

#[test]
fn test_avio_reading0() {
    avio_file_reading(cstr::cstr!("tests/assets/vids/bear.mp4")).unwrap();
}

#[test]
fn test_avio_reading1() {
    avio_file_reading(cstr::cstr!("tests/assets/vids/centaur.mpg")).unwrap();
}
