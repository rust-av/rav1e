// Copyright (c) 2018-2021, The rav1e contributors. All rights reserved
//
// This source code is subject to the terms of the BSD 2 Clause License and
// the Alliance for Open Media Patent License 1.0. If the BSD 2 Clause License
// was not distributed with this source code in the LICENSE file, you can
// obtain it at www.aomedia.org/license/software. If the Alliance for Open
// Media Patent License 1.0 was not distributed with this source code in the
// PATENTS file, you can obtain it at www.aomedia.org/license/patent.

use crate::api::channel::*;
// use crate::api::color::*;
use crate::api::config::*;
// use crate::api::context::RcData;
use crate::api::internal::ContextInner;
use crate::api::util::*;
use crate::api::InterConfig;

use crossbeam::channel::*;
use crossbeam::thread;

// use crate::encoder::*;
use crate::frame::*;
use crate::util::Pixel;

use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::sync::Arc;

struct SubGop<T: Pixel> {
  frames: Vec<Arc<Frame<T>>>,
  end_gop: bool,
}

/*
impl<T: Pixel> SubGop<T> {
  fn build_fi(&self) -> Vec<FrameData<T>> {
    todo!()
  }
}
*/

// Extra
struct SceneChange {
  frames: u64,
  min_key_frame_interval: u64,
  max_key_frame_interval: u64,
}

const PYRAMID_SIZE: usize = 7;

impl SceneChange {
  fn new(min_key_frame_interval: u64, max_key_frame_interval: u64) -> Self {
    Self { frames: 0, min_key_frame_interval, max_key_frame_interval }
  }

  // Tell where to split the lookahead
  // 7 is currently hardcoded, it should be a parameter
  fn split<T: Pixel>(
    &mut self, lookahead: &[Arc<Frame<T>>],
  ) -> Option<(usize, bool)> {
    self.frames += 1;

    let new_gop = if self.frames < self.min_key_frame_interval {
      false
    } else if self.frames >= self.max_key_frame_interval {
      self.frames = 0;
      true
    } else {
      false
    };

    let len = lookahead.len();

    if len > PYRAMID_SIZE {
      Some((PYRAMID_SIZE, new_gop))
    } else if new_gop {
      Some((len - 1, true))
    } else {
      None
    }
  }
}

struct Worker<T: Pixel> {
  idx: usize,
  back: Sender<usize>,
  send: Sender<Packet<T>>,
  inner: ContextInner<T>,
}

impl<T: Pixel> Worker<T> {
  fn process(&mut self, s: SubGop<T>) {
    for f in s.frames {
      while !self.inner.needs_more_fi_lookahead() {
        let r = self.inner.receive_packet();
        match r {
          Ok(p) => {
            self.send.send(p).unwrap();
          }
          Err(EncoderStatus::Encoded) => {}
          _ => todo!("Error management {:?}", r),
        }
      }
      let _ = self.inner.send_frame(Some(f), None);
      //      let _ = self.send.send();
    }
  }

  fn flush(&mut self) {
    self.inner.limit = Some(self.inner.frame_count);
    let _ = self.inner.send_frame(None, None);

    loop {
      match self.inner.receive_packet() {
        Ok(p) => self.send.send(p).unwrap(),
        Err(EncoderStatus::LimitReached) => break,
        Err(EncoderStatus::Encoded) => {}
        _ => todo!("Error management"),
      }
    }
  }
}

impl<T: Pixel> Drop for Worker<T> {
  fn drop(&mut self) {
    let _ = self.back.send(self.idx);
  }
}

struct WorkerPool<T: Pixel> {
  send_workers: Sender<usize>,
  recv_workers: Receiver<usize>,
  send_reassemble: Sender<(usize, Receiver<Packet<T>>)>,
  count: usize,
  _pixel: PhantomData<T>,
  cfg: Config,
}

impl<T: Pixel> WorkerPool<T> {
  fn new(
    workers: usize, mut cfg: Config,
  ) -> (Self, Receiver<(usize, Receiver<Packet<T>>)>) {
    let (send_workers, recv_workers) = bounded(workers);
    let (send_reassemble, recv_reassemble) = unbounded();

    // TODO: unpack send_frame in process
    cfg.enc.speed_settings.no_scene_detection = true;

    for w in 0..workers {
      let _ = send_workers.send(w);
    }

    (
      WorkerPool {
        send_workers,
        recv_workers,
        send_reassemble,
        count: 0,
        _pixel: PhantomData,
        cfg,
      },
      recv_reassemble,
    )
  }

  fn get_worker(&mut self, s: &thread::Scope) -> Option<Sender<SubGop<T>>> {
    self.recv_workers.recv().ok().map(|idx| {
      let (sb_send, sb_recv) = unbounded();
      let (send, recv) = unbounded();

      let _ = self.send_reassemble.send((self.count, recv));

      let inner = self.cfg.new_inner().unwrap();

      let mut w = Worker { idx, back: self.send_workers.clone(), send, inner };

      s.spawn(move |_| {
        for sb in sb_recv.iter() {
          w.process(sb);
        }

        w.flush();
      });

      self.count += 1;

      sb_send
    })
  }
}

fn reassemble<P: Pixel>(
  recv_reassemble: Receiver<(usize, Receiver<Packet<P>>)>, s: &thread::Scope,
  send_packet: Sender<Packet<P>>,
) {
  s.spawn(move |_| {
    let mut pending = BTreeMap::new();
    let mut last_idx = 0;
    let mut packet_index = 0;
    for (idx, recv) in recv_reassemble.iter() {
      pending.insert(idx, recv);
      while let Some(recv) = pending.remove(&last_idx) {
        for mut p in recv {
          // patch up the packet_index
          p.input_frameno = packet_index;
          let _ = send_packet.send(p);
          packet_index += 1;
        }
        last_idx += 1;
      }
    }

    while !pending.is_empty() {
      if let Some(recv) = pending.remove(&last_idx) {
        for mut p in recv {
          // patch up the packet_index
          p.input_frameno = packet_index;
          let _ = send_packet.send(p);
          packet_index += 1;
        }
      }
      last_idx += 1;
    }
  });
}

impl Config {
  // Group the incoming frames in Gops, emit a SubGop at time.
  fn scenechange<T: Pixel>(
    &self, s: &thread::Scope, r: Receiver<FrameInput<T>>,
  ) -> Receiver<SubGop<T>> {
    let inter_cfg = InterConfig::new(&self.enc);
    let lookahead_distance = inter_cfg.keyframe_lookahead_distance() as usize;
    let (send, recv) = bounded(lookahead_distance * 2);

    let mut sc = SceneChange::new(
      self.enc.min_key_frame_interval,
      self.enc.max_key_frame_interval,
    );

    s.spawn(move |_| {
      let mut lookahead = Vec::new();
      for f in r.iter() {
        let (frame, _params) = f;

        lookahead.push(frame.unwrap());

        // we need at least lookahead_distance frames to reason
        if lookahead.len() < lookahead_distance {
          continue;
        }

        if let Some((split_pos, end_gop)) = sc.split(&lookahead) {
          let rem = lookahead.split_off(split_pos);
          let _ = send.send(SubGop { frames: lookahead, end_gop });

          lookahead = rem;
        }
      }

      while let Some((split_pos, end_gop)) = sc.split(&lookahead) {
        let rem = lookahead.split_off(split_pos);
        let _ = send.send(SubGop { frames: lookahead, end_gop });

        lookahead = rem;
      }

      if !lookahead.is_empty() {
        let _ = send.send(SubGop { frames: lookahead, end_gop: true });
      }
    });

    recv
  }

  /// Encode the subgops, dispatch each Gop to an available worker
  fn encode<T: Pixel>(
    &self, s: &thread::Scope, workers: usize, r: Receiver<SubGop<T>>,
    send_packet: Sender<Packet<T>>,
  ) {
    let (mut pool, recv) = WorkerPool::new(workers, self.clone());

    let mut sg_send = pool.get_worker(s).unwrap();

    s.spawn(move |s| {
      for sb in r.iter() {
        let end_gop = sb.end_gop;
        let _ = sg_send.send(sb);

        if end_gop {
          sg_send = pool.get_worker(s).unwrap();
        }
      }
    });

    reassemble(recv, s, send_packet)
  }

  /// Create a single pass by_gop encoder channel
  ///
  /// Drop the `FrameSender<T>` endpoint to flush the encoder.
  ///
  ///
  pub fn new_by_gop_channel<T: Pixel>(
    &self, slots: usize,
  ) -> Result<VideoDataChannel<T>, InvalidConfig> {
    let rc = &self.rate_control;

    if rc.emit_pass_data || rc.summary.is_some() {
      return Err(InvalidConfig::RateControlConfigurationMismatch);
    }

    self.validate()?;

    // TODO: make it user-settable
    let input_len = self.enc.rdo_lookahead_frames as usize * 4;
    let frame_limit = std::i32::MAX as u64;

    let (send_frame, receive_frame) = bounded(input_len);
    let (send_packet, receive_packet) = unbounded();

    let cfg = self.clone();

    let pool = self.new_thread_pool();

    // We use crossbeam instead of rayon on purpose.
    // We could add a work-stealing logic later.
    let run = move || {
      let _ = thread::scope(|s| {
        let sg_recv = cfg.scenechange(s, receive_frame);
        cfg.encode(s, slots, sg_recv, send_packet);
      });
    };

    if let Some(pool) = pool {
      pool.spawn(run);
    } else {
      rayon::spawn(run);
    }

    let channel = (
      FrameSender::new(frame_limit, send_frame, Arc::new(self.enc)),
      PacketReceiver { receiver: receive_packet, config: Arc::new(self.enc) },
    );

    Ok(channel)
  }
}
