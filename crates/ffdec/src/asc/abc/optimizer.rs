//! Port of `macromedia.abc.Optimizer`, which mxmlc runs over every abc it emits.

use std::panic::{catch_unwind, AssertUnwindSafe};

use super::bytecode_buffer::BytecodeBuffer;
use super::decoder::Decoder;
use super::decoder_exception::DecoderException;
use super::encoder::Encoder;

pub fn main(args: &[String]) {
    let mut file_names: Vec<String> = Vec::new();
    let mut output_file = "merged.abc".to_string();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "-o" {
            i += 1;
            output_file = args[i].clone();
        } else {
            file_names.push(args[i].clone());
        }
        i += 1;
    }

    // an unreadable file leaves a null buffer behind in Java, which fails the whole merge
    let byte_codes: Option<Vec<BytecodeBuffer>> = file_names.iter().map(|f| BytecodeBuffer::from_file(f).ok()).collect();
    let abc = byte_codes.and_then(optimize_buffers);

    match abc {
        Some(abc) => {
            if let Err(e) = std::fs::write(&output_file, abc) {
                eprintln!("{}", e);
            }
        }
        None => eprintln!("Error occurred when merging abc files."),
    }
}

/// `optimize(ByteList)`; `None` where Java would hit a `NullPointerException`.
pub fn optimize(bytes: &[u8]) -> Option<Vec<u8>> {
    optimize_buffers(vec![BytecodeBuffer::from_bytes(bytes.to_vec())])
}

/// Java's `catch (Throwable ex) { ex.printStackTrace(); ... }`: errors and panics both count.
fn guard<T>(f: impl FnOnce() -> Result<T, DecoderException>) -> Option<T> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(v)) => Some(v),
        Ok(Err(e)) => {
            eprintln!("macromedia.abc.DecoderException: {}", e);
            None
        }
        Err(_) => None,
    }
}

pub fn optimize_buffers(byte_codes: Vec<BytecodeBuffer>) -> Option<Vec<u8>> {
    let abc_size = byte_codes.len();
    let mut decoders: Vec<Decoder> = Vec::with_capacity(abc_size);
    let mut major_version = 0;
    let mut minor_version = 0;

    for b in byte_codes {
        let d = guard(|| Decoder::new(b))?;
        if d.major_version > major_version {
            major_version = d.major_version;
        }
        if d.minor_version > minor_version {
            minor_version = d.minor_version;
        }
        decoders.push(d);
    }
    let pools = decoders.iter().map(|d| d.constant_pool.clone()).collect();

    let mut encoder = Encoder::new(major_version, minor_version);
    encoder.enable_peep_hole();
    guard(|| {
        encoder.add_constant_pools(pools);
        encoder.disable_debugging();
        encoder.remove_metadata();
        encoder.configure(&decoders);
        Ok(())
    })?;

    for (j, decoder) in decoders.iter().enumerate() {
        encoder.use_constant_pool(j as i32);
        let method_info = &decoder.method_info;
        guard(|| {
            for k in 0..method_info.size() {
                method_info.decode(k, &mut encoder)?;
            }
            Ok(())
        })?;
    }

    for (j, decoder) in decoders.iter().enumerate() {
        encoder.use_constant_pool(j as i32);
        let metadata_info = &decoder.metadata_info;
        guard(|| {
            for k in 0..metadata_info.size() {
                metadata_info.decode(k, &mut encoder)?;
            }
            Ok(())
        })?;
    }

    for (j, decoder) in decoders.iter().enumerate() {
        encoder.use_constant_pool(j as i32);
        let class_info = &decoder.class_info;
        guard(|| {
            for k in 0..class_info.size() {
                class_info.decode_instance(k, &mut encoder)?;
            }
            Ok(())
        })?;
    }

    for (j, decoder) in decoders.iter().enumerate() {
        encoder.use_constant_pool(j as i32);
        let class_info = &decoder.class_info;
        guard(|| {
            for k in 0..class_info.size() {
                class_info.decode_class(k, 0, &mut encoder)?;
            }
            Ok(())
        })?;
    }

    for (j, decoder) in decoders.iter().enumerate() {
        encoder.use_constant_pool(j as i32);
        let script_info = &decoder.script_info;
        guard(|| {
            for k in 0..script_info.size() {
                script_info.decode(k, &mut encoder)?;
            }
            Ok(())
        })?;
    }

    for (j, decoder) in decoders.iter().enumerate() {
        encoder.use_constant_pool(j as i32);
        let method_bodies = &decoder.method_bodies;
        guard(|| {
            for k in 0..method_bodies.size() {
                method_bodies.decode_passes(k, 2, &mut encoder)?;
            }
            Ok(())
        })?;
    }

    Some(encoder.to_abc())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    const HELLO: &str = r"C:\Users\Aallyn\AppData\Local\Temp\claude\S--Desktop-Projects-Rust-FFDec\918e2469-beb7-4861-9c86-13b9dac07e23\scratchpad\mx\Hello.swf";
    const CORPUS: &str =
        r"C:\Users\Aallyn\AppData\Local\Temp\claude\S--Desktop-Projects-TroveUI\c97486dc-2200-441b-a026-1591ba7f8d6a\scratchpad\all\swfs";

    fn abcs(swf: &Path) -> Vec<Vec<u8>> {
        let data = std::fs::read(swf).unwrap();
        let swf = crate::Swf::parse(&data).unwrap();
        let mut out = Vec::new();
        swf.walk(|_, tag| {
            if let Some((abc, _)) = crate::abc::tag_abc(tag) {
                out.push(abc.to_vec());
            }
        });
        out
    }

    fn swfs(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut entries: Vec<PathBuf> = rd.map(|e| e.unwrap().path()).collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                swfs(&p, out);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("swf")) {
                out.push(p);
            }
        }
    }

    #[test]
    fn hello_mxmlc_output_is_a_fixed_point() {
        if !Path::new(HELLO).exists() {
            eprintln!("skipped: {} not found", HELLO);
            return;
        }
        let abcs = abcs(Path::new(HELLO));
        assert!(!abcs.is_empty());
        for abc in abcs {
            let once = optimize(&abc).expect("optimize");
            assert_eq!(once, abc, "mxmlc already optimized this abc");
            assert_eq!(optimize(&once).expect("optimize"), once);
        }
    }

    #[test]
    #[ignore]
    fn corpus_is_idempotent() {
        let mut files = Vec::new();
        swfs(Path::new(CORPUS), &mut files);
        let (mut same, mut total) = (0, 0);
        for f in files {
            for abc in abcs(&f) {
                total += 1;
                let once = optimize(&abc).expect("optimize");
                let twice = optimize(&once).expect("optimize");
                assert_eq!(twice, once, "{}", f.display());
                if once == abc {
                    same += 1;
                }
            }
        }
        eprintln!("{same}/{total} abcs unchanged by optimize, all idempotent");
    }

    /// Writes `N.abc` and the Rust `N.rust.abc` for every abc of every SWF under `ASC_CORPUS` into `ASC_DUMP`,
    /// for comparison against the Java Optimizer.
    #[test]
    #[ignore]
    fn dump_for_java_comparison() {
        let (Ok(src), Ok(dst)) = (std::env::var("ASC_CORPUS"), std::env::var("ASC_DUMP")) else {
            eprintln!("skipped: set ASC_CORPUS and ASC_DUMP");
            return;
        };
        std::fs::create_dir_all(&dst).unwrap();
        let mut files = Vec::new();
        swfs(Path::new(&src), &mut files);
        if Path::new(&src).is_file() {
            files.push(PathBuf::from(&src));
        }
        let mut n = 0;
        for f in files {
            let Ok(list) = std::panic::catch_unwind(|| abcs(&f)) else { continue };
            for abc in list {
                let base = Path::new(&dst).join(format!("{n:05}"));
                std::fs::write(base.with_extension("abc"), &abc).unwrap();
                if let Some(out) = optimize(&abc) {
                    std::fs::write(base.with_extension("rust.abc"), out).unwrap();
                }
                n += 1;
            }
        }
        eprintln!("dumped {n} abcs");
    }

    /// Merges every run of three dumped `N.abc` files in `ASC_DUMP` into `mN.rust.abc`, as the Java driver does.
    #[test]
    #[ignore]
    fn merge_for_java_comparison() {
        let Ok(dst) = std::env::var("ASC_DUMP") else {
            eprintln!("skipped: set ASC_DUMP");
            return;
        };
        let mut names: Vec<PathBuf> = std::fs::read_dir(&dst)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.file_name().unwrap().to_str().unwrap().len() == 9 && p.extension().is_some_and(|x| x == "abc"))
            .collect();
        names.sort();
        for chunk in names.chunks(3) {
            let bufs = chunk.iter().map(|p| BytecodeBuffer::from_bytes(std::fs::read(p).unwrap())).collect();
            let stem = chunk[0].file_stem().unwrap().to_str().unwrap().to_string();
            if let Some(out) = optimize_buffers(bufs) {
                std::fs::write(Path::new(&dst).join(format!("m{stem}.rust.abc")), out).unwrap();
            }
        }
    }
}
