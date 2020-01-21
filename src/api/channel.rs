// Copyright (c) 2018-2019, The rav1e contributors. All rights reserved
//
// This source code is subject to the terms of the BSD 2 Clause License and
// the Alliance for Open Media Patent License 1.0. If the BSD 2 Clause License
// was not distributed with this source code in the LICENSE file, you can
// obtain it at www.aomedia.org/license/software. If the Alliance for Open
// Media Patent License 1.0 was not distributed with this source code in the
// PATENTS file, you can obtain it at www.aomedia.org/license/patent.
#![allow(missing_docs)]

use crate::api::color::*;
use crate::api::config::*;
use crate::api::util::*;

use bitstream_io::*;
use crossbeam::channel::*;

use crate::encoder::*;
use crate::frame::*;
use crate::util::Pixel;

use std::io;
use std::sync::Arc;

/// Endpoint to send frames
pub struct FrameSender<T: Pixel>(
  Sender<(Option<Arc<Frame<T>>>, Option<FrameParameters>)>,
);

impl<T: Pixel> FrameSender<T> {
  pub fn try_send<F: IntoFrame<T>>(
    &self, frame: F,
  ) -> Result<(), TrySendError<(Option<Arc<Frame<T>>>, Option<FrameParameters>)>>
  {
    self.0.try_send(frame.into())
  }

  pub fn send<F: IntoFrame<T>>(
    &self, frame: F,
  ) -> Result<(), SendError<(Option<Arc<Frame<T>>>, Option<FrameParameters>)>>
  {
    self.0.send(frame.into())
  }
  pub fn len(&self) -> usize {
    self.0.len()
  }
  // TODO: proxy more methods
}

/// Endpoint to receive packets
pub struct PacketReceiver<T: Pixel>(Receiver<Packet<T>>);

impl<T: Pixel> PacketReceiver<T> {
  pub fn try_recv(&self) -> Result<Packet<T>, TryRecvError> {
    self.0.try_recv()
  }
  pub fn recv(&self) -> Result<Packet<T>, RecvError> {
    self.0.recv()
  }
  pub fn len(&self) -> usize {
    self.0.len()
  }
  // TODO: proxy more methods
}

// TODO: use a separate struct?
impl Config {
  // TODO: add validation
  #[inline]
  pub fn new_frame<T: Pixel>(&self) -> Frame<T> {
    Frame::new(self.enc.width, self.enc.height, self.enc.chroma_sampling)
  }

  /// Produces a sequence header matching the current encoding context.
  ///
  /// Its format is compatible with the AV1 Matroska and ISOBMFF specification.
  /// Note that the returned header does not include any config OBUs which are
  /// required for some uses. See [the specification].
  ///
  /// [the specification]:
  /// https://aomediacodec.github.io/av1-isobmff/#av1codecconfigurationbox-section
  #[inline]
  pub fn container_sequence_header(&self) -> Vec<u8> {
    fn sequence_header_inner(seq: &Sequence) -> io::Result<Vec<u8>> {
      let mut buf = Vec::new();

      {
        let mut bw = BitWriter::endian(&mut buf, BigEndian);
        bw.write_bit(true)?; // marker
        bw.write(7, 1)?; // version
        bw.write(3, seq.profile)?;
        bw.write(5, 31)?; // level
        bw.write_bit(false)?; // tier
        bw.write_bit(seq.bit_depth > 8)?; // high_bitdepth
        bw.write_bit(seq.bit_depth == 12)?; // twelve_bit
        bw.write_bit(seq.bit_depth == 1)?; // monochrome
        bw.write_bit(seq.chroma_sampling != ChromaSampling::Cs444)?; // chroma_subsampling_x
        bw.write_bit(seq.chroma_sampling == ChromaSampling::Cs420)?; // chroma_subsampling_y
        bw.write(2, 0)?; // sample_position
        bw.write(3, 0)?; // reserved
        bw.write_bit(false)?; // initial_presentation_delay_present

        bw.write(4, 0)?; // reserved
      }

      Ok(buf)
    }

    let seq = Sequence::new(&self.enc);

    sequence_header_inner(&seq).unwrap()
  }
}

/// Create a single pass encoder channel
///
/// The `PacketReceiver<T>` endpoint may block if not enough Frames are
/// sent through the `FrameSender<T>` endpoint.
///
/// Drop the `FrameSender<T>` endpoint to flush the encoder.
pub fn new_channel<T: Pixel>(
  cfg: &Config,
) -> Result<(FrameSender<T>, PacketReceiver<T>), InvalidConfig> {
  let (send_frame, receive_frame) = unbounded();
  let (send_packet, receive_packet) = unbounded();

  let mut inner = cfg.new_inner()?;
  let pool =
    rayon::ThreadPoolBuilder::new().num_threads(cfg.threads).build().unwrap();

  pool.spawn(move || {
    for f in receive_frame.iter() {
      if !inner.needs_more_fi_lookahead() {
        // needs_more_fi_lookahead() should guard for missing output_frameno
        // already.
        // this call should return either Ok or Err(Encoded)
        if let Some(p) = inner.receive_packet().ok() {
          send_packet.send(p).unwrap();
        }
      }

      let (frame, params) = f;
      let _ = inner.send_frame(frame, params); // TODO make sure it cannot fail.
    }

    inner.limit = Some(inner.frame_count);
    let _ = inner.send_frame(None, None);

    loop {
      let r = inner.receive_packet();
      eprintln!("got {:?}", r);
      match r {
        Ok(p) => {
          send_packet.send(p).unwrap();
        }
        Err(EncoderStatus::LimitReached) => break,
        Err(EncoderStatus::Encoded) => {}
        _ => unreachable!(),
      }
    }

    // drop(send_packet); // This happens implicitly
  });

  Ok((FrameSender(send_frame), PacketReceiver(receive_packet)))
}

/// Endpoint to send previous-pass statistics data
pub struct PassDataSender(Sender<Box<[u8]>>);
/// Endpoint to receive previous-pass statistics data
pub struct PassDataReceiver(Receiver<Box<[u8]>>);

/// Create a multipass encoder channel
///
/// To setup a first-pass encode drop the `PassDataSender` before sending the
/// first Frame.
///
/// The `PacketReceiver<T>` may block if not enough pass statistics data
/// are sent through the `PassDataSender` endpoint
///
/// Drop the `Sender<F>` endpoint to flush the encoder.
/// The last buffer in the Receiver<Box<[u8]>>
pub fn new_multipass_channel<T: Pixel>(
  cfg: &Config,
) -> ((FrameSender<T>, PacketReceiver<T>), (PassDataSender, PassDataReceiver))
{
  unimplemented!()
}
