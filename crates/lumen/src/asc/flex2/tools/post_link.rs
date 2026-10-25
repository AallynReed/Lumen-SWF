//! Port of `flex2.tools.PostLink`: merges each frame's `DoABC` tags into one, through `macromedia.abc`'s encoder with
//! debug info and unkept metadata dropped and the peephole optimizer on.

use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::asc::abc::bytecode_buffer::BytecodeBuffer;
use crate::asc::abc::decoder::Decoder;
use crate::asc::abc::decoder_exception::DecoderException;
use crate::asc::abc::encoder::Encoder;
use crate::asc::flex2::compiler::configuration::Configuration;
use crate::asc::flex2::linker::{DoAbc, Movie};

/// `catch (Throwable ex) { assert false; skipFrame = true; }`.
fn guard<T>(f: impl FnOnce() -> Result<T, DecoderException>) -> Option<T> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(v)) => Some(v),
        _ => None,
    }
}

/// `run(Movie)`.
pub fn run(movie: &mut Movie, configuration: &Configuration) {
    let debug = configuration.debug || configuration.verbose_stacktraces;
    let optimize = configuration.optimize;
    let mut as3metadata: Vec<String> = configuration.keep_as3_metadata.clone().unwrap_or_default();
    if !movie.metadata_to_keep.is_empty() {
        for m in &movie.metadata_to_keep {
            if !as3metadata.contains(m) {
                as3metadata.push(m.clone());
            }
        }
    }
    let enable_debugger = movie.enable_debugger.is_some();
    for (i, f) in movie.frames.iter_mut().enumerate() {
        if optimize {
            merge(&mut f.do_abcs, enable_debugger || debug, optimize, &format!("frame{}", i + 1), &as3metadata);
        } else {
            for j in 0..f.do_abcs.len() {
                let mut one = vec![f.do_abcs[j].clone()];
                let name = one[0].name.clone();
                merge(&mut one, enable_debugger || debug, optimize, &name, &as3metadata);
                f.do_abcs[j] = one.pop().unwrap();
            }
        }
    }
}

/// `merge(doABCs, debug, runPeephole, name)`.
pub fn merge(do_abcs: &mut Vec<DoAbc>, debug: bool, run_peephole: bool, name: &str, as3metadata: &[String]) {
    let abc_size = do_abcs.len();
    if abc_size == 0 {
        return;
    }
    let flag = if abc_size == 1 { do_abcs[0].flag } else { 1 };
    let mut decoders: Vec<Decoder> = Vec::with_capacity(abc_size);
    let mut major_version = 0;
    let mut minor_version = 0;
    for tag in do_abcs.iter() {
        let buf = BytecodeBuffer::from_bytes(tag.abc.clone());
        let Some(d) = guard(|| Decoder::new(buf)) else { return };
        major_version = d.major_version;
        minor_version = d.minor_version;
        decoders.push(d);
    }
    let pools = decoders.iter().map(|d| d.constant_pool.clone()).collect();
    let mut encoder = Encoder::new(major_version, minor_version);
    let configured = guard(|| {
        encoder.add_constant_pools(pools);
        if !debug {
            encoder.disable_debugging();
        }
        encoder.remove_metadata();
        for m in as3metadata {
            encoder.add_metadata_to_keep(m);
        }
        if run_peephole {
            encoder.enable_peep_hole();
        }
        encoder.configure(&decoders);
        Ok(())
    });
    if configured.is_none() {
        return;
    }
    macro_rules! pass {
        ($body:expr) => {
            for (j, decoder) in decoders.iter().enumerate() {
                encoder.use_constant_pool(j as i32);
                let ok = guard(|| {
                    #[allow(clippy::redundant_closure_call)]
                    ($body)(decoder, &mut encoder)
                });
                if ok.is_none() {
                    return;
                }
            }
        };
    }
    pass!(|d: &Decoder, e: &mut Encoder| -> Result<(), DecoderException> {
        for k in 0..d.method_info.size() {
            d.method_info.decode(k, e)?;
        }
        Ok(())
    });
    pass!(|d: &Decoder, e: &mut Encoder| -> Result<(), DecoderException> {
        for k in 0..d.metadata_info.size() {
            d.metadata_info.decode(k, e)?;
        }
        Ok(())
    });
    pass!(|d: &Decoder, e: &mut Encoder| -> Result<(), DecoderException> {
        for k in 0..d.class_info.size() {
            d.class_info.decode_instance(k, e)?;
        }
        Ok(())
    });
    pass!(|d: &Decoder, e: &mut Encoder| -> Result<(), DecoderException> {
        for k in 0..d.class_info.size() {
            d.class_info.decode_class(k, 0, e)?;
        }
        Ok(())
    });
    pass!(|d: &Decoder, e: &mut Encoder| -> Result<(), DecoderException> {
        for k in 0..d.script_info.size() {
            d.script_info.decode(k, e)?;
        }
        Ok(())
    });
    pass!(|d: &Decoder, e: &mut Encoder| -> Result<(), DecoderException> {
        for k in 0..d.method_bodies.size() {
            d.method_bodies.decode_passes(k, 2, e)?;
        }
        Ok(())
    });
    let abc = encoder.to_abc();
    do_abcs.clear();
    do_abcs.push(DoAbc { name: name.to_string(), flag, abc });
}
