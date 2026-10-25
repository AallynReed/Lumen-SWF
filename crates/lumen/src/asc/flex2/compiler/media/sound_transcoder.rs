//! Port of `flex2.compiler.media.SoundTranscoder`: an MP3, from its first frame sync, as a `DefineSound`.

use super::abstract_transcoder::{self, new_results, resolve_source, AbstractTranscoder, DefineTagClass};
use crate::asc::flex2::compiler::source::VirtualFile;
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::flex2::compiler::transcoder::{self, Args, PathResolver, ResultsRef, Transcoder, TranscoderException, TranscodingResults};
use crate::asc::flex2::flash::swf::tags::{codes, Body, DefineSound, Tag, TagRef};

const MP3_FREQUENCIES: [[i32; 4]; 4] = [[11025, 0, 22050, 44100], [12000, 0, 24000, 48000], [8000, 0, 16000, 32000], [0, 0, 0, 0]];

const MP3_BITRATES: [[i32; 5]; 16] = [
    [0, 0, 0, 0, 0],
    [32, 32, 32, 32, 8],
    [64, 48, 40, 48, 16],
    [96, 56, 48, 56, 24],
    [128, 64, 56, 64, 32],
    [160, 80, 64, 80, 40],
    [192, 96, 80, 96, 48],
    [224, 112, 96, 112, 56],
    [256, 128, 112, 128, 64],
    [288, 160, 128, 144, 80],
    [320, 192, 160, 160, 96],
    [352, 224, 192, 176, 112],
    [384, 256, 224, 192, 128],
    [416, 320, 256, 224, 144],
    [448, 384, 320, 256, 160],
    [-1, -1, -1, -1, -1],
];

const MP3_BITRATE_INDICES: [[i32; 4]; 4] = [[-1, 4, 4, 3], [-1, -1, -1, -1], [-1, 4, 4, 3], [-1, 2, 1, 0]];

pub struct SoundTranscoder {
    base: AbstractTranscoder,
}

impl Default for SoundTranscoder {
    fn default() -> Self {
        Self::new()
    }
}

impl SoundTranscoder {
    pub fn new() -> SoundTranscoder {
        SoundTranscoder { base: AbstractTranscoder::new(&[super::mime::MP3], Some(DefineTagClass::DefineSound), true, "flex2.compiler.media.SoundTranscoder") }
    }

    fn do_transcode(define_tag: Option<DefineTagClass>, context: &PathResolver, args: &mut Args, class_name: &str, generate_source: bool) -> Result<ResultsRef, TranscoderException> {
        let results = new_results(TranscodingResults::with_source(resolve_source(context, args)?));
        let new_name = args.get(transcoder::NEWNAME).cloned();
        let source = results.borrow().asset_source.clone().unwrap();
        results.borrow_mut().define_tag = Some(mp3(&source, new_name)?);
        if generate_source {
            let base_class = abstract_transcoder::associated_class(define_tag, results.borrow().define_tag.as_ref());
            abstract_transcoder::generate_source(&results, base_class, class_name, args, &[])?;
        }
        Ok(results)
    }
}

fn mp3(source: &VirtualFile, symbol_name: Option<String>) -> Result<TagRef, TranscoderException> {
    let bytes = source.to_byte_array().map_err(|e| TranscoderException::exception_while_transcoding(&e.to_string()))?;
    let sound = read_fully(&bytes);
    if sound.len() < 5 {
        return Err(TranscoderException::new("the sound file was not recognized as a MP3."));
    }
    let d = |i: usize| sound.get(i).copied().unwrap_or(0) as i8 as i32;
    let version = (d(3) >> 3) & 0x3;
    let layer = (d(3) >> 1) & 0x3;
    let sampling_rate = (d(4) >> 2) & 0x3;
    let channel_mode = (d(5) >> 6) & 0x3;
    let frequency = MP3_FREQUENCIES[sampling_rate as usize][version as usize];
    let rate = match frequency {
        11025 => 1,
        22050 => 2,
        44100 => 3,
        _ => return Err(TranscoderException::new(format!("unsupported sampling rate ({frequency}Hz)"))),
    };
    let sample_count = count_frames(&sound) as i64 * if layer == 3 { 384 } else { 1152 };
    if sample_count < 0 {
        return Err(TranscoderException::new("could not determine the sample frame count"));
    }
    let ds = DefineSound { format: 2, rate, size: 1, type_: if channel_mode == 3 { 0 } else { 1 }, sample_count, data: sound };
    let t = Tag::new_ref(codes::DEFINE_SOUND, Body::DefineSound(ds));
    t.borrow_mut().name = symbol_name;
    Ok(t)
}

/// `readFully`: two zero bytes (the frames to skip), then everything from the first 11-bit frame sync.
fn read_fully(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len() + 2);
    out.push(0);
    out.push(0);
    let mut state = 0;
    for &b in input {
        match state {
            0 => {
                if b == 255 {
                    state = 1;
                }
            }
            1 => {
                if (b >> 5) & 0x7 == 7 {
                    out.push(255);
                    out.push(b);
                    state = 2;
                } else {
                    state = 0;
                }
            }
            _ => out.push(b),
        }
    }
    out
}

/// `countFrames`, with its LAME-padding fallback.
fn count_frames(data: &[u8]) -> i32 {
    let mut count = 0;
    let mut start: usize = 2;
    let mut skipped = false;
    while start + 2 < data.len() {
        let mut b1 = data[start] as i32;
        let mut b2 = data[start + 1] as i32;
        let mut b3 = data[start + 2] as i32;
        if b1 != 255 || (b2 >> 5) & 0x7 != 7 {
            if !skipped && start > 0 {
                b3 = b2;
                b2 = b1;
                b1 = data[start - 1] as i32;
                if b1 != 255 || (b2 >> 5) & 0x7 != 7 {
                    start += 1;
                    continue;
                } else {
                    start -= 1;
                }
            } else {
                start += 1;
                continue;
            }
        }
        let version = (b2 >> 3) & 0x3;
        let layer = (b2 >> 1) & 0x3;
        let bits = (b3 >> 4) & 0xf;
        let bitrate_index = MP3_BITRATE_INDICES[version as usize][layer as usize];
        let bitrate = if bitrate_index != -1 { MP3_BITRATES[bits as usize][bitrate_index as usize] * 1000 } else { -1 };
        if bitrate == -1 {
            skipped = true;
            start += 1;
            continue;
        }
        let sampling_rate = (b3 >> 2) & 0x3;
        let frequency = MP3_FREQUENCIES[sampling_rate as usize][version as usize];
        if frequency == 0 {
            skipped = true;
            start += 1;
            continue;
        }
        let padding = (b3 >> 1) & 0x1;
        let frame_length = if layer == 3 { (12 * bitrate / frequency + padding) * 4 } else { 144 * bitrate / frequency + padding };
        if frame_length == 0 {
            break;
        }
        start += frame_length as usize;
        skipped = false;
        count += 1;
    }
    count
}

impl Transcoder for SoundTranscoder {
    fn is_supported(&self, mime_type: &str) -> bool {
        self.base.is_supported(mime_type)
    }

    fn transcode(&mut self, context: &PathResolver, _symbol_table: Option<&SymbolTable>, args: &mut Args, class_name: &str, generate_source: bool) -> Result<ResultsRef, TranscoderException> {
        let dt = self.base.define_tag;
        self.base.transcode(args, &|_| false, &mut |args| Self::do_transcode(dt, context, args, class_name, generate_source))
    }

    fn get_associated_class(&self, tag: Option<&TagRef>) -> Option<String> {
        self.base.get_associated_class(tag)
    }

    fn clear(&mut self) {
        self.base.clear();
    }
}
