// Copyright (c) 2018-2020, The rav1e contributors. All rights reserved
//
// This source code is subject to the terms of the BSD 2 Clause License and
// the Alliance for Open Media Patent License 1.0. If the BSD 2 Clause License
// was not distributed with this source code in the LICENSE file, you can
// obtain it at www.aomedia.org/license/software. If the Alliance for Open
// Media Patent License 1.0 was not distributed with this source code in the
// PATENTS file, you can obtain it at www.aomedia.org/license/patent.

// use thiserror::*;

use crate::api::config::*;
use crate::api::context::Context;
use crate::util::Pixel;

/// TODO: make it richer
#[derive(Debug)]
pub enum RcError {
  InvalidVersion,
  ConfigurationMismatch,
}

const RC_SUMMARY_PACKET_SIZE: usize = 64;
const RC_FRAME_PACKET_SIZE: usize = 16;

impl Config {
  /// Setup the configuration rate-control settings using a first pass summary
  ///
  /// Error out if the summary and the configuration do not match
  /// or the summary version is not compatible with the encoder
  ///
  pub fn rc_setup_twopass(&mut self, summary: &[u8]) -> Result<(), RcError> {
    Ok(())
  }

  /// Setup the configuration rate-control to emit first pass data
  ///
  /// Return a tuple with the Summary packet and Frame packet size in bytes
  /// or an error if the Configuration is incorrect
  pub fn rc_setup_first_pass(&mut self) -> Result<(usize, usize), RcError> {
    Ok((RC_SUMMARY_PACKET_SIZE, RC_FRAME_PACKET_SIZE))
  }
}
/// Rate Control Summary Packet
pub struct Summary(Box<[u8]>);
/// Rate Control Frame-specific Packet
pub struct Frame(Box<[u8]>);

// TODO implement AsRef and friends to access the byte data properly.

/// Rate Control Data
pub enum RcData {
  /// A Rate Control Summary Packet
  Summary(Summary),
  /// A Rate Control Frame-specific Packet
  Frame(Frame),
}

impl<T: Pixel> Context<T> {
  /// Return the first pass data
  ///
  /// Call it before ... and after ...
  pub fn rc_first_pass_data(&mut self) -> Result<RcData, ()> {
    Err(())
  }

  /// Feed the first pass data to the encoder
  ///
  /// Call it before receive_packet
  pub fn rc_second_pass_data(&mut self, data: RcData) -> Result<(), ()> {
    Ok(())
  }
}

/// Merge a set of RcData::Summary
pub fn rc_merge_summaries(summaries: &[Summary]) -> Result<RcData, ()> {
  Err(())
}
