#![allow(dead_code)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

#[cfg_attr(feature = "cargo-clippy", allow(const_static_lifetime))]
#[cfg_attr(feature = "cargo-clippy", allow(unreadable_literal))]

pub mod aom {
    include!(concat!(env!("OUT_DIR"), "/aom.rs"));
}

pub use aom::*;

#[cfg(test)]
mod tests {
    use super::aom::*;
    use std::ffi::CStr;
    use std::mem;
    #[test]
    fn version() {
        println!("{}", unsafe {
            CStr::from_ptr(aom_codec_version_str()).to_string_lossy()
        });
        println!("{}", unsafe {
            CStr::from_ptr(aom_codec_build_config()).to_string_lossy()
        });
    }
    #[test]
    fn encode() {
        let w = 360;
        let h = 360;
        let align = 32;
        let kf_interval = 10;
        let mut raw = unsafe { mem::uninitialized() };
        let mut ctx = unsafe { mem::uninitialized() };

        let ret = unsafe { aom_img_alloc(&mut raw, aom_img_fmt::AOM_IMG_FMT_I420, w, h, align) };
        if ret.is_null() {
            panic!("Image allocation failed");
        }
        mem::forget(ret); // raw and ret are the same
        print!("{:#?}", raw);

        let mut cfg = unsafe { mem::uninitialized() };
        let mut ret = unsafe { aom_codec_enc_config_default(aom_codec_av1_cx(), &mut cfg, 0) };

        if ret != aom_codec_err_t::AOM_CODEC_OK {
            panic!("Default Configuration failed");
        }

        cfg.g_w = w;
        cfg.g_h = h;
        cfg.g_timebase.num = 1;
        cfg.g_timebase.den = 30;
        cfg.rc_target_bitrate = 100 * 1014;

        ret = unsafe {
            aom_codec_enc_init_ver(
                &mut ctx,
                aom_codec_av1_cx(),
                &mut cfg,
                0,
                AOM_ENCODER_ABI_VERSION as i32,
            )
        };

        if ret != aom_codec_err_t::AOM_CODEC_OK {
            panic!("Codec Init failed");
        }

        let mut out = 0;
        for i in 0..100 {
            let mut flags = 0;
            if i % kf_interval == 0 {
                flags |= AOM_EFLAG_FORCE_KF;
            }
            unsafe {
                let ret =
                    aom_codec_encode(&mut ctx, &mut raw, i, 1, flags as aom_enc_frame_flags_t);
                if ret != aom_codec_err_t::AOM_CODEC_OK {
                    panic!("Encode failed {:?}", ret);
                }

                let mut iter = mem::zeroed();
                loop {
                    let pkt = aom_codec_get_cx_data(&mut ctx, &mut iter);

                    if pkt.is_null() {
                        break;
                    } else {
                        println!("{:#?}", (*pkt).kind);
                        out = 1;
                    }
                }
            }
        }

        if out != 1 {
            panic!("No packet produced");
        }
    }
}
