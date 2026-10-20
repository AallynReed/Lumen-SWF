//! Event sounds (DefineSound) and stream sounds (SoundStreamHead + SoundStreamBlock).

use crate::error::{Error, Result};
use crate::io::{Reader, Writer};
use crate::tags::{code::*, Tag, Timeline};

use super::flv;

pub const RATES: [u32; 4] = [5512, 11025, 22050, 44100];

pub mod format {
    pub const PCM_NATIVE: u8 = 0;
    pub const ADPCM: u8 = 1;
    pub const MP3: u8 = 2;
    pub const PCM_LE: u8 = 3;
    pub const NELLY_16K: u8 = 4;
    pub const NELLY_8K: u8 = 5;
    pub const NELLY: u8 = 6;
    pub const SPEEX: u8 = 11;
}

/// Everything needed to export one sound, with its data split into the original chunks.
pub struct Sound<'a> {
    pub format: u8,
    pub rate_index: u8,
    pub sixteen_bit: bool,
    pub stereo: bool,
    pub chunks: Vec<&'a [u8]>,
    /// MP3 samples to skip at the start (stream latency seek).
    pub latency: u32,
    pub stream: bool,
    pub frame_rate: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Mp3,
    Wav,
    Flv,
}

impl Kind {
    pub fn ext(self) -> &'static str {
        match self {
            Kind::Mp3 => "mp3",
            Kind::Wav => "wav",
            Kind::Flv => "flv",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            Kind::Mp3 => "audio/mpeg",
            Kind::Wav => "audio/wav",
            Kind::Flv => "video/x-flv",
        }
    }
}

impl<'a> Sound<'a> {
    pub fn from_define_sound(tag: &'a Tag) -> Result<Sound<'a>> {
        let mut r = Reader::new(&tag.body);
        r.u16()?;
        let format = r.ub(4)? as u8;
        let rate_index = r.ub(2)? as u8;
        let sixteen_bit = r.bit()?;
        let stereo = r.bit()?;
        r.u32()?;
        let mut data = r.rest();
        if format == format::MP3 {
            data = data.get(2..).unwrap_or(&[]);
        }
        Ok(Sound { format, rate_index, sixteen_bit, stereo, chunks: vec![data], latency: 0, stream: false, frame_rate: 0.0 })
    }

    /// The head at `index` in `timeline` and its blocks, up to the next stream head.
    pub fn from_stream(timeline: &'a Timeline, index: usize, frame_rate: f32) -> Result<Sound<'a>> {
        let head = &timeline.tags[index];
        let mut r = Reader::new(&head.body);
        r.u8()?;
        let format = r.ub(4)? as u8;
        let rate_index = r.ub(2)? as u8;
        let sixteen_bit = r.bit()?;
        let stereo = r.bit()?;
        r.u16()?;
        let latency = if format == format::MP3 && r.remaining() >= 2 { r.i16()?.max(0) as u32 } else { 0 };
        let chunks = timeline.tags[index + 1..]
            .iter()
            .take_while(|t| t.code != SOUND_STREAM_HEAD && t.code != SOUND_STREAM_HEAD2)
            .filter(|t| t.code == SOUND_STREAM_BLOCK)
            .map(|t| if format == format::MP3 { t.body.get(4..).unwrap_or(&[]) } else { &t.body[..] })
            .collect();
        Ok(Sound { format, rate_index, sixteen_bit, stereo, chunks, latency, stream: true, frame_rate })
    }

    pub fn rate(&self) -> u32 {
        match self.format {
            format::NELLY_16K => 16000,
            format::NELLY_8K => 8000,
            format::NELLY => 22050,
            _ => RATES[self.rate_index as usize & 3],
        }
    }

    /// FFDec's default "MP3/WAV/FLV" choice.
    pub fn native_kind(&self) -> Kind {
        match self.format {
            format::MP3 if self.stream && self.latency > 0 => Kind::Wav,
            format::MP3 => Kind::Mp3,
            format::PCM_NATIVE | format::PCM_LE => Kind::Wav,
            format::ADPCM | format::NELLY_16K | format::NELLY_8K | format::NELLY if self.stream => Kind::Wav,
            _ => Kind::Flv,
        }
    }

    /// A kind the browser can play: MP3 or WAV.
    pub fn playable_kind(&self) -> Kind {
        if self.format == format::MP3 && self.latency == 0 { Kind::Mp3 } else { Kind::Wav }
    }

    pub fn export(&self, kind: Kind) -> Result<Vec<u8>> {
        match kind {
            Kind::Mp3 => Ok(self.chunks.concat()),
            Kind::Wav => {
                let (pcm, stereo) = self.decode()?;
                Ok(wav(self.rate(), stereo, &pcm))
            }
            Kind::Flv => Ok(self.flv()),
        }
    }

    /// Decodes to interleaved signed 16-bit PCM. Returns the samples and whether they are stereo.
    pub fn decode(&self) -> Result<(Vec<i16>, bool)> {
        let mut out = Vec::new();
        let mut stereo = self.stereo;
        match self.format {
            format::PCM_NATIVE | format::PCM_LE => {
                for c in &self.chunks {
                    if self.sixteen_bit {
                        out.extend(c.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])));
                    } else {
                        out.extend(c.iter().map(|&b| ((b as i16) - 128) << 8));
                    }
                }
            }
            format::ADPCM => {
                for c in &self.chunks {
                    adpcm(c, self.stereo, &mut out);
                }
            }
            format::NELLY_16K | format::NELLY_8K | format::NELLY => {
                stereo = false;
                for c in &self.chunks {
                    out.extend(super::nellymoser::decode(c));
                }
            }
            format::MP3 => {
                let (pcm, channels) = mp3(&self.chunks.concat())?;
                stereo = channels == 2;
                let skip = self.latency as usize * channels;
                out = pcm.get(skip..).map(<[i16]>::to_vec).unwrap_or_default();
            }
            f => return Err(Error::Other(format!("cannot decode sound format {f} ({})", crate::info::sound_format(f as u32)))),
        }
        Ok((out, stereo))
    }

    fn flv(&self) -> Vec<u8> {
        let mut w = flv::Writer::new(true, false);
        let head = (self.format << 4) | (self.rate_index << 2) | ((self.sixteen_bit as u8) << 1) | self.stereo as u8;
        let ms = if self.frame_rate > 0.0 { (1000.0 / self.frame_rate) as u32 } else { 0 };
        for (i, c) in self.chunks.iter().enumerate() {
            let mut data = vec![head];
            data.extend_from_slice(c);
            let ts = if self.stream { ms * i as u32 } else { 0 };
            w.tag(flv::AUDIO, ts, &data);
        }
        w.finish()
    }
}

pub fn wav(rate: u32, stereo: bool, pcm: &[i16]) -> Vec<u8> {
    let channels: u16 = if stereo { 2 } else { 1 };
    let mut w = Writer::new();
    w.bytes(b"RIFF");
    w.u32(36 + pcm.len() as u32 * 2);
    w.bytes(b"WAVEfmt ");
    w.u32(16);
    w.u16(1);
    w.u16(channels);
    w.u32(rate);
    w.u32(rate * channels as u32 * 2);
    w.u16(channels * 2);
    w.u16(16);
    w.bytes(b"data");
    w.u32(pcm.len() as u32 * 2);
    for s in pcm {
        w.i16(*s);
    }
    w.data
}

const INDEX_ADJUST: [&[i32]; 4] = [
    &[-1, 2],
    &[-1, -1, 2, 4],
    &[-1, -1, -1, -1, 2, 4, 6, 8],
    &[-1, -1, -1, -1, -1, -1, -1, -1, 1, 2, 4, 6, 8, 10, 13, 16],
];

const STEP_SIZES: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66, 73, 80, 88, 97, 107,
    118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449, 494, 544, 598, 658, 724, 796, 876, 963,
    1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272, 2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894,
    6484, 7132, 7845, 8630, 9493, 10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794,
    32767,
];

struct AdpcmState {
    sample: i32,
    index: i32,
}

impl AdpcmState {
    fn step(&mut self, code: u32, bits: u32) -> i16 {
        let step = STEP_SIZES[self.index as usize];
        let mut diff = step >> (bits - 1);
        for k in 0..bits - 1 {
            if code & (1 << k) != 0 {
                diff += step >> (bits - 2 - k);
            }
        }
        if code & (1 << (bits - 1)) != 0 {
            diff = -diff;
        }
        self.sample = (self.sample + diff).clamp(-32768, 32767);
        let table = INDEX_ADJUST[bits as usize - 2];
        self.index = (self.index + table[(code as usize) & (table.len() - 1)]).clamp(0, 88);
        self.sample as i16
    }
}

/// Port of FFDec's `AdpcmDecoder`, including its choice not to emit each packet's initial sample.
fn adpcm(data: &[u8], stereo: bool, out: &mut Vec<i16>) {
    let mut r = Reader::new(data);
    let Ok(code_size) = r.ub(2) else { return };
    let bits = code_size + 2;
    let channels = if stereo { 2 } else { 1 };
    let avail = |r: &Reader| r.remaining_bits();
    loop {
        let mut states = Vec::with_capacity(channels);
        for _ in 0..channels {
            let (Ok(sample), Ok(index)) = (r.sb(16), r.ub(6)) else { return };
            states.push(AdpcmState { sample, index: index as i32 });
        }
        for _ in 1..=4095 {
            if avail(&r) < (bits as usize) * channels {
                break;
            }
            for s in states.iter_mut() {
                let code = r.ub(bits).unwrap();
                out.push(s.step(code, bits));
            }
        }
        if r.is_empty() {
            return;
        }
    }
}

fn mp3(data: &[u8]) -> Result<(Vec<i16>, usize)> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let err = |e: symphonia::core::errors::Error| Error::Other(format!("MP3: {e}"));
    let mss = MediaSourceStream::new(Box::new(std::io::Cursor::new(data.to_vec())), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("mp3");
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(err)?;
    let mut format = probed.format;
    let track = format.default_track().ok_or_else(|| Error::Other("MP3: no audio track".into()))?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default()).map_err(err)?;
    let mut out = Vec::new();
    let mut channels = track.codec_params.channels.map(|c| c.count()).unwrap_or(1);
    while let Ok(packet) = format.next_packet() {
        if packet.track_id() != track_id {
            continue;
        }
        let Ok(buf) = decoder.decode(&packet) else { continue };
        let spec = *buf.spec();
        channels = spec.channels.count();
        let mut sb = SampleBuffer::<i16>::new(buf.capacity() as u64, spec);
        sb.copy_interleaved_ref(buf);
        out.extend_from_slice(sb.samples());
    }
    Ok((out, channels))
}

#[cfg(test)]
mod tests {
    #[test]
    fn wav_header() {
        let w = super::wav(22050, true, &[1, -1, 2, -2]);
        assert_eq!(&w[..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(w[4..8].try_into().unwrap()), 36 + 8);
        assert_eq!(&w[36..40], b"data");
        assert_eq!(w.len(), 44 + 8);
    }

    #[test]
    fn adpcm_four_bit_matches_ffdec_tables() {
        let mut s = super::AdpcmState { sample: 0, index: 0 };
        assert_eq!(s.step(0b0111, 4), 7 / 8 + 7 / 4 + 7 / 2 + 7);
        assert_eq!(s.index, 8);
        let mut s = super::AdpcmState { sample: 0, index: 0 };
        assert_eq!(s.step(0b1000, 4), -(7 >> 3));
    }
}
