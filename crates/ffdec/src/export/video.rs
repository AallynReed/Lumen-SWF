//! DefineVideoStream + VideoFrame tags to FLV.

use std::collections::BTreeMap;

use crate::error::Result;
use crate::io::Reader;
use crate::tags::{code::VIDEO_FRAME, Tag};
use crate::Swf;

use super::flv;

const SORENSON_H263: u8 = 2;
const VP6: u8 = 4;
const VP6_ALPHA: u8 = 5;

pub fn export(swf: &Swf, stream: &Tag) -> Result<Vec<u8>> {
    let mut r = Reader::new(&stream.body);
    let id = r.u16()?;
    let num_frames = r.u16()?;
    let width = r.u16()?;
    let height = r.u16()?;
    r.u8()?;
    let codec = r.u8()?;

    let mut frames: BTreeMap<u16, &[u8]> = BTreeMap::new();
    swf.walk(|_, t| {
        if t.code == VIDEO_FRAME && t.body.len() >= 4 && u16::from_le_bytes([t.body[0], t.body[1]]) == id {
            frames.insert(u16::from_le_bytes([t.body[2], t.body[3]]), &t.body[4..]);
        }
    });

    let rate = if swf.frame_rate > 0.0 { swf.frame_rate as f64 } else { 1.0 };
    let mut w = flv::Writer::new(false, true);
    w.metadata(&[
        ("duration", num_frames as f64 / rate),
        ("width", width as f64),
        ("height", height as f64),
        ("framerate", rate),
        ("videocodecid", codec as f64),
    ]);
    let (mut h_adj, mut v_adj) = (0u8, 0u8);
    for (&num, &data) in &frames {
        let mut frame_type = 1u8;
        let mut payload = Vec::with_capacity(data.len() + 2);
        if codec == VP6 || codec == VP6_ALPHA {
            let mut s = Reader::new(data);
            if codec == VP6_ALPHA {
                let _ = s.bytes(3);
            }
            if let Some((intra, dims)) = vp6_header(&mut s) {
                frame_type = if intra { 1 } else { 2 };
                if let Some((dim_y, dim_x)) = dims {
                    let adj = |size: u16, dim: u8| -> u8 {
                        if dim == 0 {
                            return 0;
                        }
                        let dim = dim as u32;
                        ((dim * (size as u32).div_ceil(dim)) - size as u32) as u8
                    };
                    h_adj = adj(width, dim_x);
                    v_adj = adj(height, dim_y);
                }
            }
            payload.push((h_adj << 4) | (v_adj & 0x0f));
        } else if codec == SORENSON_H263 {
            if let Some(t) = h263_picture_type(data) {
                frame_type = match t {
                    0 => 1,
                    1 => 2,
                    _ => 3,
                };
            }
        }
        payload.extend_from_slice(data);
        let mut tag = vec![(frame_type << 4) | (codec & 0x0f)];
        tag.extend_from_slice(&payload);
        w.tag(flv::VIDEO, (num as f64 * 1000.0 / rate).floor() as u32, &tag);
    }
    Ok(w.finish())
}

/// Returns whether the frame is intra-coded and, for intra frames, the macroblock grid size.
fn vp6_header(s: &mut Reader) -> Option<(bool, Option<(u8, u8)>)> {
    let frame_mode = s.ub(1).ok()?;
    s.ub(6).ok()?;
    let marker = s.ub(1).ok()?;
    if frame_mode != 0 {
        return Some((false, None));
    }
    s.ub(5).ok()?;
    let version2 = s.ub(2).ok()?;
    s.ub(1).ok()?;
    if marker == 1 || version2 == 0 {
        s.u16().ok()?;
    }
    let dim_y = s.u8().ok()?;
    let dim_x = s.u8().ok()?;
    Some((true, Some((dim_y, dim_x))))
}

fn h263_picture_type(data: &[u8]) -> Option<u32> {
    let mut s = Reader::new(data);
    s.ub(17).ok()?;
    s.ub(5).ok()?;
    s.ub(8).ok()?;
    match s.ub(3).ok()? {
        0 => {
            s.ub(16).ok()?;
        }
        1 => {
            s.ub(32).ok()?;
        }
        _ => {}
    }
    s.ub(2).ok()
}
