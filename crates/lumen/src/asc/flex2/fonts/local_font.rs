//! Port of swfutils `flash.fonts.LocalFont` and the `ObjectInputStream.readObject()` of a `localFonts.ser` snapshot
//! (a serialized `HashMap<String, LocalFont>` from `flex2.tools.FontSnapshot`).

use std::collections::HashMap;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalFont {
    pub postscript_name: Option<String>,
    pub path: Option<String>,
    pub fs_type: i32,
    pub copyright: Option<String>,
    pub trademark: Option<String>,
}

#[derive(Clone, Debug)]
enum Obj {
    Null,
    Str(String),
    Class(usize),
    Instance { prims: Vec<(String, i64)>, objs: Vec<(String, Obj)>, annotation: Vec<Obj> },
    Block,
}

#[derive(Clone, Debug)]
struct ClassDesc {
    flags: u8,
    fields: Vec<(u8, String)>,
    sup: Option<usize>,
}

struct Reader<'a> {
    d: &'a [u8],
    p: usize,
    handles: Vec<Obj>,
    classes: Vec<ClassDesc>,
}

type R<T> = Result<T, ()>;

impl Reader<'_> {
    fn u8(&mut self) -> R<u8> {
        let b = *self.d.get(self.p).ok_or(())?;
        self.p += 1;
        Ok(b)
    }
    fn u16(&mut self) -> R<u16> {
        Ok((self.u8()? as u16) << 8 | self.u8()? as u16)
    }
    fn i32(&mut self) -> R<i32> {
        Ok((self.u16()? as i32) << 16 | self.u16()? as i32)
    }
    fn i64(&mut self) -> R<i64> {
        Ok((self.i32()? as i64) << 32 | (self.i32()? as u32 as i64))
    }
    fn utf(&mut self, len: usize) -> R<String> {
        let b = self.d.get(self.p..self.p + len).ok_or(())?;
        self.p += len;
        let mut units = Vec::new();
        let mut i = 0;
        while i < b.len() {
            let c = b[i] as u16;
            if c < 0x80 {
                units.push(c);
                i += 1;
            } else if c & 0xE0 == 0xC0 {
                units.push((c & 0x1F) << 6 | (*b.get(i + 1).ok_or(())? as u16 & 0x3F));
                i += 2;
            } else {
                units.push((c & 0x0F) << 12 | (*b.get(i + 1).ok_or(())? as u16 & 0x3F) << 6 | (*b.get(i + 2).ok_or(())? as u16 & 0x3F));
                i += 3;
            }
        }
        Ok(String::from_utf16_lossy(&units))
    }
    fn short_utf(&mut self) -> R<String> {
        let n = self.u16()? as usize;
        self.utf(n)
    }

    fn class_desc(&mut self) -> R<Option<usize>> {
        match self.u8()? {
            0x70 => Ok(None),
            0x71 => match self.handle()? {
                Obj::Class(c) => Ok(Some(c)),
                _ => Err(()),
            },
            0x72 => {
                self.short_utf()?;
                self.i64()?;
                let ci = self.classes.len();
                self.classes.push(ClassDesc { flags: 0, fields: Vec::new(), sup: None });
                self.handles.push(Obj::Class(ci));
                let flags = self.u8()?;
                let n = self.u16()?;
                let mut fields = Vec::new();
                for _ in 0..n {
                    let t = self.u8()?;
                    let f = self.short_utf()?;
                    if t == b'L' || t == b'[' {
                        self.object()?;
                    }
                    fields.push((t, f));
                }
                if self.u8()? != 0x78 {
                    return Err(());
                }
                let sup = self.class_desc()?;
                let c = &mut self.classes[ci];
                c.flags = flags;
                c.fields = fields;
                c.sup = sup;
                Ok(Some(ci))
            }
            _ => Err(()),
        }
    }

    fn handle(&mut self) -> R<Obj> {
        let h = self.i32()? - 0x7E0000;
        self.handles.get(h as usize).cloned().ok_or(())
    }

    fn object(&mut self) -> R<Obj> {
        match self.u8()? {
            0x70 => Ok(Obj::Null),
            0x71 => self.handle(),
            0x74 => {
                let s = self.short_utf()?;
                self.handles.push(Obj::Str(s.clone()));
                Ok(Obj::Str(s))
            }
            0x7C => {
                let n = self.i64()? as usize;
                let s = self.utf(n)?;
                self.handles.push(Obj::Str(s.clone()));
                Ok(Obj::Str(s))
            }
            0x77 => {
                let n = self.u8()? as usize;
                self.p += n;
                Ok(Obj::Block)
            }
            0x73 => {
                let class = self.class_desc()?.ok_or(())?;
                let h = self.handles.len();
                self.handles.push(Obj::Null);
                let mut chain = Vec::new();
                let mut c = Some(class);
                while let Some(ci) = c {
                    chain.push(ci);
                    c = self.classes[ci].sup;
                }
                let (mut prims, mut objs, mut annotation) = (Vec::new(), Vec::new(), Vec::new());
                for &ci in chain.iter().rev() {
                    let cd = self.classes[ci].clone();
                    for (t, f) in &cd.fields {
                        let v = match t {
                            b'B' | b'Z' => self.u8()? as i64,
                            b'C' | b'S' => self.u16()? as i16 as i64,
                            b'I' | b'F' => self.i32()? as i64,
                            b'J' | b'D' => self.i64()?,
                            _ => {
                                let o = self.object()?;
                                objs.push((f.clone(), o));
                                continue;
                            }
                        };
                        prims.push((f.clone(), v));
                    }
                    if cd.flags & 0x01 != 0 {
                        loop {
                            if self.d.get(self.p) == Some(&0x78) {
                                self.p += 1;
                                break;
                            }
                            if self.d.get(self.p) == Some(&0x77) {
                                self.p += 1;
                                let n = self.u8()? as usize;
                                annotation.push(Obj::Block);
                                self.p += n;
                                continue;
                            }
                            annotation.push(self.object()?);
                        }
                    }
                }
                let o = Obj::Instance { prims, objs, annotation };
                self.handles[h] = o.clone();
                Ok(o)
            }
            _ => Err(()),
        }
    }
}

fn string_field(objs: &[(String, Obj)], name: &str) -> Option<String> {
    objs.iter().find(|(f, _)| f == name).and_then(|(_, o)| if let Obj::Str(s) = o { Some(s.clone()) } else { None })
}

/// `(Map<String, LocalFont>) new ObjectInputStream(...).readObject()`; `Err` where Java throws (the caller ignores it).
pub fn read_local_fonts(data: &[u8]) -> Result<HashMap<String, LocalFont>, ()> {
    let mut r = Reader { d: data, p: 0, handles: Vec::new(), classes: Vec::new() };
    if r.u16()? != 0xACED || r.u16()? != 5 {
        return Err(());
    }
    let Obj::Instance { annotation, .. } = r.object()? else { return Err(()) };
    let items: Vec<&Obj> = annotation.iter().filter(|o| !matches!(o, Obj::Block)).collect();
    let mut map = HashMap::new();
    for kv in items.chunks(2) {
        let [Obj::Str(k), v] = kv else { return Err(()) };
        let Obj::Instance { prims, objs, .. } = v else { return Err(()) };
        let lf = LocalFont {
            postscript_name: string_field(objs, "postscriptName"),
            path: string_field(objs, "path"),
            fs_type: prims.iter().find(|(f, _)| f == "fsType").map_or(0, |(_, v)| *v as i32),
            copyright: string_field(objs, "copyright"),
            trademark: string_field(objs, "trademark"),
        };
        map.insert(k.clone(), lf);
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    /// A `flex2.tools.FontSnapshot` file of the case fonts against Java's own reading of it.
    #[test]
    fn reads_font_snapshot() {
        let Ok(data) = std::fs::read(r"S:\Desktop\Projects\Rust-FFDec\target\fonts_ws\integ\localFonts.ser") else { return };
        let want = std::fs::read_to_string(r"S:\Desktop\Projects\Rust-FFDec\target\fonts_ws\integ\localFonts.txt").unwrap();
        let m = super::read_local_fonts(&data).unwrap();
        let mut keys: Vec<&String> = m.keys().collect();
        keys.sort();
        let n = |s: &Option<String>| s.clone().unwrap_or_else(|| "null".into());
        let got: String = keys.iter().map(|k| { let f = &m[*k]; format!("{}\t{}\t{}\t{}\t{}\t{}\n", k, n(&f.postscript_name), n(&f.path), f.fs_type, n(&f.copyright), n(&f.trademark)) }).collect();
        assert_eq!(got.replace("\r\n", "\n"), want.replace("\r\n", "\n"));
        assert!(super::read_local_fonts(b"\r\n").is_err());
    }
}
