//! Minimal FLV writer for exported sounds and video streams.

pub const AUDIO: u8 = 8;
pub const VIDEO: u8 = 9;
pub const SCRIPT: u8 = 18;

pub struct Writer {
    out: Vec<u8>,
}

impl Writer {
    pub fn new(audio: bool, video: bool) -> Writer {
        let mut out = b"FLV\x01".to_vec();
        out.push(((audio as u8) << 2) | video as u8);
        out.extend_from_slice(&9u32.to_be_bytes());
        out.extend_from_slice(&0u32.to_be_bytes());
        Writer { out }
    }

    pub fn tag(&mut self, kind: u8, timestamp: u32, data: &[u8]) {
        let o = &mut self.out;
        o.push(kind);
        o.extend_from_slice(&(data.len() as u32).to_be_bytes()[1..]);
        o.extend_from_slice(&timestamp.to_be_bytes()[1..]);
        o.push((timestamp >> 24) as u8);
        o.extend_from_slice(&[0, 0, 0]);
        o.extend_from_slice(data);
        o.extend_from_slice(&(11 + data.len() as u32).to_be_bytes());
    }

    /// An `onMetaData` script tag with numeric properties.
    pub fn metadata(&mut self, props: &[(&str, f64)]) {
        let mut d = vec![2];
        amf_string(&mut d, "onMetaData");
        d.push(8);
        d.extend_from_slice(&(props.len() as u32).to_be_bytes());
        for (k, v) in props {
            amf_string(&mut d, k);
            d.push(0);
            d.extend_from_slice(&v.to_be_bytes());
        }
        d.extend_from_slice(&[0, 0, 9]);
        self.tag(SCRIPT, 0, &d);
    }

    pub fn finish(self) -> Vec<u8> {
        self.out
    }
}

fn amf_string(d: &mut Vec<u8>, s: &str) {
    d.extend_from_slice(&(s.len() as u16).to_be_bytes());
    d.extend_from_slice(s.as_bytes());
}
