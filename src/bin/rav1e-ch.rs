// Copyright (c) 2017-2019, The rav1e contributors. All rights reserved
//
// This source code is subject to the terms of the BSD 2 Clause License and
// the Alliance for Open Media Patent License 1.0. If the BSD 2 Clause License
// was not distributed with this source code in the LICENSE file, you can
// obtain it at www.aomedia.org/license/software. If the Alliance for Open
// Media Patent License 1.0 was not distributed with this source code in the
// PATENTS file, you can obtain it at www.aomedia.org/license/patent.

#![deny(bare_trait_objects)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_ptr_alignment)]
#![allow(clippy::cognitive_complexity)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::verbose_bit_mask)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::many_single_char_names)]

#[macro_use]
extern crate log;

mod common;
mod decoder;
mod error;
#[cfg(feature = "serialize")]
mod kv;
mod muxer;
mod stats;

use crate::common::*;
use crate::error::*;
use crate::stats::*;
use rav1e::prelude::channel::FrameSender;
use rav1e::prelude::*;

use crate::decoder::Decoder;
use crate::decoder::VideoDetails;
use crate::muxer::*;
use std::fs::File;
use std::io::{Read, Seek, Write};
use std::sync::Arc;

struct Source<D: Decoder + Send> {
  limit: usize,
  count: usize,
  input: D,
  #[cfg(all(unix, feature = "signal-hook"))]
  exit_requested: Arc<std::sync::atomic::AtomicBool>,
}

impl<D: Decoder + Send> Source<D> {
  fn read_frame<T: Pixel>(
    &mut self, sf: &FrameSender<T>, video_info: &VideoDetails,
  ) -> bool {
    if self.limit != 0 && self.count == self.limit {
      return false;
    }

    #[cfg(all(unix, feature = "signal-hook"))]
    {
      if self.exit_requested.load(std::sync::atomic::Ordering::SeqCst) {
        return false;
      }
    }

    match self.input.read_frame(&video_info) {
      Ok(frame) => {
        match video_info.bit_depth {
          8 | 10 | 12 => {}
          _ => panic!("unknown input bit depth!"),
        }
        self.count += 1;
        let _ = sf.send(frame);
        true
      }
      _ => false,
    }
  }
}

fn do_encode<T: Pixel, D: Decoder + Send>(
  cfg: Config, verbose: Verbose, mut progress: ProgressInfo,
  output: &mut dyn Muxer, source: &mut Source<D>,
  pass1file_name: Option<&String>, pass2file_name: Option<&String>,
  mut y4m_enc: Option<y4m::Encoder<'_, Box<dyn Write + Send>>>,
) -> Result<(), CliError> {
  let pass2file = pass2file_name.map(|f| {
    File::open(f).unwrap_or_else(|_| {
      panic!("Unable to open \"{}\" for reading two-pass data.", f)
    })
  });
  let pass1file = pass1file_name.map(|f| {
    File::create(f).unwrap_or_else(|_| {
      panic!("Unable to open \"{}\" for writing two-pass data.", f)
    })
  });

  let ((sf, rp), (pass2, pass1)) =
    if pass1file.is_none() && pass2file.is_none() {
      (
        cfg
          .new_channel::<T>()
          .map_err(|e| e.context("Invalid encoder settings"))?,
        (None, None),
      )
    } else {
      let (video_channels, (pass2_s, pass1_r)) = cfg
        .new_multipass_channel::<T>()
        .map_err(|e| e.context("Invalid encoder settings"))?;

      (
        video_channels,
        (pass2file.map(|p2| (p2, pass2_s)), pass1file.map(|p1| (p1, pass1_r))),
      )
    };

  let y4m_details = source.input.get_video_details();

  crossbeam::scope(|s| {
    if let Some((mut p1, pass1_r)) = pass1 {
      s.spawn(move |_| {
        let p = pass1_r.iter();
        // Receive a Summary data as first and last buffer in the channel.
        p.for_each(|d| {
          let buf = match d {
            PassData::Summary(d) => {
              p1.seek(std::io::SeekFrom::Start(0)).expect("Unable to seek in two-pass data file.");
              d
            }
            PassData::Frame(d) => {
              d
            }
          };

          let len = (buf.len() as u32).to_be_bytes();
          p1.write_all(&len).unwrap();
          p1.write_all(&buf).unwrap();
        });
      });
    }

    if let Some((mut p2, pass2_s)) = pass2 {
      s.spawn(move |_| {
        let mut len = [0u8; 4];
        let mut buf = [0u8; 64]; // TODO: have an API for this.
        let mut summary = true;
        while p2.read_exact(&mut len).is_ok() {
          let len = u32::from_be_bytes(len) as usize;
          p2.read_exact(&mut buf[..len]).unwrap(); // TODO: errors
          let data = buf[..len].to_vec().into_boxed_slice();
          if summary {
            if pass2_s.send(PassData::Summary(data)).is_err() {
              break;
            }
            summary = false;
          } else {
            if pass2_s.send(PassData::Frame(data)).is_err() {
              break;
            }
          }
        }
      });
    }

    s.spawn(|_| {
      while source.read_frame(&sf, &y4m_details) {
        // eprintln!("Reading frame");
      }
      drop(sf);
    });
    s.spawn(|_| {
      for pkt in rp.iter() {
        output.write_frame(
          pkt.input_frameno as u64,
          pkt.data.as_ref(),
          pkt.frame_type,
        );
        output.flush().unwrap();
        if let (Some(ref mut y4m_enc_uw), Some(ref rec)) =
          (y4m_enc.as_mut(), &pkt.rec)
        {
          write_y4m_frame(y4m_enc_uw, rec, y4m_details);
        }
        let frame: FrameSummary = pkt.into();
        progress.add_frame(frame.clone());
        match verbose {
          Verbose::Verbose => info!("{} - {}", frame, progress),
          Verbose::Normal => eprint!("\r{}                    ", progress),
          Verbose::Quiet => {}
        }
      }
      if verbose == Verbose::Normal {
        // Clear out the temporary progress indicator
        eprint!("\r");
      }
      progress.print_summary(verbose == Verbose::Verbose);
    });
  })
  .unwrap();

  Ok(())
}

fn main() {
  #[cfg(feature = "tracing")]
  use rust_hawktracer::*;
  better_panic::install();
  init_logger();

  #[cfg(feature = "tracing")]
  let instance = HawktracerInstance::new();
  #[cfg(feature = "tracing")]
  let _listener = instance.create_listener(HawktracerListenerType::ToFile {
    file_path: "trace.bin".into(),
    buffer_size: 4096,
  });

  match run() {
    Ok(()) => {}
    Err(e) => error::print_error(&e),
  }
}

fn init_logger() {
  use std::str::FromStr;
  fn level_colored(l: log::Level) -> console::StyledObject<&'static str> {
    use console::style;
    use log::Level;
    match l {
      Level::Trace => style("??").dim(),
      Level::Debug => style("? ").dim(),
      Level::Info => style("> ").green(),
      Level::Warn => style("! ").yellow(),
      Level::Error => style("!!").red(),
    }
  }

  // this can be changed to flatten
  let level = std::env::var("RAV1E_LOG")
    .ok()
    .map(|l| log::LevelFilter::from_str(&l).ok())
    .unwrap_or(Some(log::LevelFilter::Info))
    .unwrap();

  fern::Dispatch::new()
    .format(move |out, message, record| {
      out.finish(format_args!(
        "{level} {message}",
        level = level_colored(record.level()),
        message = message,
      ));
    })
    // set the default log level. to filter out verbose log messages from dependencies, set
    // this to Warn and overwrite the log level for your crate.
    .level(log::LevelFilter::Warn)
    // change log levels for individual modules. Note: This looks for the record's target
    // field which defaults to the module path but can be overwritten with the `target`
    // parameter:
    // `info!(target="special_target", "This log message is about special_target");`
    .level_for("rav1e", level)
    .level_for("rav1e_ch", level)
    // output to stdout
    .chain(std::io::stderr())
    .apply()
    .unwrap();
}

cfg_if::cfg_if! {
  if #[cfg(target_os = "windows")] {
    fn print_rusage() {
      eprintln!("Windows benchmarking is not supported currently.");
    }
  } else {
    fn print_rusage() {
      let (utime, stime, maxrss) = unsafe {
        let mut usage = std::mem::zeroed();
        let _ = libc::getrusage(libc::RUSAGE_SELF, &mut usage);
        (usage.ru_utime, usage.ru_stime, usage.ru_maxrss)
      };
      eprintln!(
        "user time: {} s",
        utime.tv_sec as f64 + utime.tv_usec as f64 / 1_000_000f64
      );
      eprintln!(
        "system time: {} s",
        stime.tv_sec as f64 + stime.tv_usec as f64 / 1_000_000f64
      );
      eprintln!("maximum rss: {} KB", maxrss);
    }
  }
}

fn run() -> Result<(), error::CliError> {
  let mut cli = parse_cli()?;
  // Maximum frame size by specification + maximum y4m header
  let limit = y4m::Limits {
    // Use saturating operations to gracefully handle 32-bit architectures
    bytes: 64usize
      .saturating_mul(64)
      .saturating_mul(4096)
      .saturating_mul(2304)
      .saturating_add(1024),
  };
  let mut y4m_dec = y4m::Decoder::new_with_limits(&mut cli.io.input, limit)
    .expect("cannot decode the input");
  let video_info = y4m_dec.get_video_details();
  let y4m_enc = match cli.io.rec.as_mut() {
    Some(rec) => Some(
      y4m::encode(
        video_info.width,
        video_info.height,
        y4m::Ratio::new(
          video_info.time_base.den as usize,
          video_info.time_base.num as usize,
        ),
      )
      .with_colorspace(y4m_dec.get_colorspace())
      .write_header(rec)
      .unwrap(),
    ),
    None => None,
  };

  cli.enc.width = video_info.width;
  cli.enc.height = video_info.height;
  cli.enc.bit_depth = video_info.bit_depth;
  cli.enc.chroma_sampling = video_info.chroma_sampling;
  cli.enc.chroma_sample_position = video_info.chroma_sample_position;

  // If no pixel range is specified via CLI, assume limited,
  // as it is the default for the Y4M format.
  if !cli.color_range_specified {
    cli.enc.pixel_range = PixelRange::Limited;
  }

  if !cli.override_time_base {
    cli.enc.time_base = video_info.time_base;
  }

  let cfg = Config { enc: cli.enc, threads: cli.threads };

  #[cfg(feature = "serialize")]
  {
    if let Some(save_config) = cli.save_config {
      let mut out = File::create(save_config)
        .map_err(|e| e.context("Cannot create configuration file"))?;
      let s = toml::to_string(&cfg.enc).unwrap();
      out
        .write_all(s.as_bytes())
        .map_err(|e| e.context("Cannot write the configuration file"))?
    }
  }

  cli.io.output.write_header(
    video_info.width,
    video_info.height,
    cli.enc.time_base.den as usize,
    cli.enc.time_base.num as usize,
  );

  info!(
    "Using y4m decoder: {}x{}p @ {}/{} fps, {}, {}-bit",
    video_info.width,
    video_info.height,
    video_info.time_base.den,
    video_info.time_base.num,
    video_info.chroma_sampling,
    video_info.bit_depth
  );
  info!("Encoding settings: {}", cfg.enc);

  let progress = ProgressInfo::new(
    Rational { num: video_info.time_base.den, den: video_info.time_base.num },
    if cli.limit == 0 { None } else { Some(cli.limit) },
    cfg.enc.show_psnr,
  );

  for _ in 0..cli.skip {
    y4m_dec.read_frame().expect("Skipped more frames than in the input");
  }

  #[cfg(all(unix, feature = "signal-hook"))]
  let exit_requested = {
    use std::sync::atomic::*;
    let e = Arc::new(AtomicBool::from(false));

    fn setup_signal(sig: i32, e: Arc<AtomicBool>) {
      unsafe {
        signal_hook::register(sig, move || {
          if e.load(Ordering::SeqCst) {
            std::process::exit(128 + sig);
          }
          e.store(true, Ordering::SeqCst);
          info!("Exit requested, flushing.");
        })
        .expect("Cannot register the signal hooks");
      }
    }

    setup_signal(signal_hook::SIGTERM, e.clone());
    setup_signal(signal_hook::SIGQUIT, e.clone());
    setup_signal(signal_hook::SIGINT, e.clone());

    e
  };

  #[cfg(all(unix, feature = "signal-hook"))]
  let mut source =
    Source { limit: cli.limit, input: y4m_dec, count: 0, exit_requested };
  #[cfg(not(all(unix, feature = "signal-hook")))]
  let mut source = Source { limit: cli.limit, input: y4m_dec, count: 0 };

  if video_info.bit_depth == 8 {
    do_encode::<u8, y4m::Decoder<'_, _>>(
      cfg,
      cli.verbose,
      progress,
      &mut *cli.io.output,
      &mut source,
      cli.pass1file_name.as_ref(),
      cli.pass2file_name.as_ref(),
      y4m_enc,
    )?
  } else {
    do_encode::<u16, y4m::Decoder<'_, _>>(
      cfg,
      cli.verbose,
      progress,
      &mut *cli.io.output,
      &mut source,
      cli.pass1file_name.as_ref(),
      cli.pass2file_name.as_ref(),
      y4m_enc,
    )?
  }
  if cli.benchmark {
    print_rusage();
  }

  Ok(())
}
