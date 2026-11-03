//! Port of JDK 17 `java.awt.geom.AffineTransform`, with its `state` machine (the arithmetic differs by state).

pub const APPLY_IDENTITY: i32 = 0;
pub const APPLY_TRANSLATE: i32 = 1;
pub const APPLY_SCALE: i32 = 2;
pub const APPLY_SHEAR: i32 = 4;
const HI_SHIFT: i32 = 3;
const HI_TRANSLATE: i32 = APPLY_TRANSLATE << HI_SHIFT;
const HI_SCALE: i32 = APPLY_SCALE << HI_SHIFT;
const HI_SHEAR: i32 = APPLY_SHEAR << HI_SHIFT;

pub const TYPE_IDENTITY: i32 = 0;
pub const TYPE_TRANSLATION: i32 = 1;
pub const TYPE_GENERAL_ROTATION: i32 = 16;
pub const TYPE_QUADRANT_ROTATION: i32 = 8;
pub const TYPE_MASK_ROTATION: i32 = TYPE_QUADRANT_ROTATION | TYPE_GENERAL_ROTATION;
const TYPE_UNKNOWN: i32 = -1;

const ROT90_CONVERSION: [i32; 8] = [
    APPLY_SHEAR,
    APPLY_SHEAR | APPLY_TRANSLATE,
    APPLY_SHEAR,
    APPLY_SHEAR | APPLY_TRANSLATE,
    APPLY_SCALE,
    APPLY_SCALE | APPLY_TRANSLATE,
    APPLY_SHEAR | APPLY_SCALE,
    APPLY_SHEAR | APPLY_SCALE | APPLY_TRANSLATE,
];

/// `NoninvertibleTransformException`.
#[derive(Debug)]
pub struct NoninvertibleTransformException;

#[derive(Clone, Copy, Debug)]
pub struct AffineTransform {
    pub m00: f64,
    pub m10: f64,
    pub m01: f64,
    pub m11: f64,
    pub m02: f64,
    pub m12: f64,
    pub state: i32,
    type_: i32,
}

impl Default for AffineTransform {
    fn default() -> Self {
        AffineTransform::new()
    }
}

impl AffineTransform {
    pub fn new() -> AffineTransform {
        AffineTransform { m00: 1.0, m10: 0.0, m01: 0.0, m11: 1.0, m02: 0.0, m12: 0.0, state: APPLY_IDENTITY, type_: TYPE_IDENTITY }
    }

    fn with_state(m00: f64, m10: f64, m01: f64, m11: f64, m02: f64, m12: f64, state: i32) -> AffineTransform {
        AffineTransform { m00, m10, m01, m11, m02, m12, state, type_: TYPE_UNKNOWN }
    }

    /// `new AffineTransform(m00, m10, m01, m11, m02, m12)`.
    pub fn from_values(m00: f64, m10: f64, m01: f64, m11: f64, m02: f64, m12: f64) -> AffineTransform {
        let mut t = AffineTransform { m00, m10, m01, m11, m02, m12, state: 0, type_: TYPE_UNKNOWN };
        t.update_state();
        t
    }

    pub fn get_translate_instance(tx: f64, ty: f64) -> AffineTransform {
        let mut t = AffineTransform::new();
        t.set_to_translation(tx, ty);
        t
    }

    pub fn get_scale_instance(sx: f64, sy: f64) -> AffineTransform {
        let mut t = AffineTransform::new();
        t.set_to_scale(sx, sy);
        t
    }

    pub fn get_rotate_instance(theta: f64) -> AffineTransform {
        let mut t = AffineTransform::new();
        t.set_to_rotation(theta);
        t
    }

    pub fn get_rotate_instance_at(theta: f64, anchorx: f64, anchory: f64) -> AffineTransform {
        let mut t = AffineTransform::new();
        t.set_to_rotation_at(theta, anchorx, anchory);
        t
    }

    pub fn get_scale_x(&self) -> f64 {
        self.m00
    }
    pub fn get_scale_y(&self) -> f64 {
        self.m11
    }
    pub fn get_shear_x(&self) -> f64 {
        self.m01
    }
    pub fn get_shear_y(&self) -> f64 {
        self.m10
    }
    pub fn get_translate_x(&self) -> f64 {
        self.m02
    }
    pub fn get_translate_y(&self) -> f64 {
        self.m12
    }

    pub fn is_identity(&self) -> bool {
        self.state == APPLY_IDENTITY || self.get_type() == TYPE_IDENTITY
    }

    pub fn get_type(&self) -> i32 {
        if self.type_ == TYPE_UNKNOWN {
            self.calculate_type()
        } else {
            self.type_
        }
    }

    fn calculate_type(&self) -> i32 {
        let mut t = *self;
        t.update_state();
        let mut ret = TYPE_IDENTITY;
        match t.state {
            7 | 6 => {
                if t.state == 7 {
                    ret = TYPE_TRANSLATION;
                }
                let (m0, m2, m3, m1) = (t.m00, t.m01, t.m10, t.m11);
                if m0 * m2 + m3 * m1 != 0.0 {
                    return 32;
                }
                let sgn0 = m0 >= 0.0;
                let sgn1 = m1 >= 0.0;
                ret |= if sgn0 == sgn1 {
                    if m0 != m1 || m2 != -m3 {
                        20
                    } else if m0 * m1 - m2 * m3 != 1.0 {
                        18
                    } else {
                        16
                    }
                } else if m0 != -m1 || m2 != m3 {
                    84
                } else if m0 * m1 - m2 * m3 != 1.0 {
                    82
                } else {
                    80
                };
            }
            5 | 4 => {
                if t.state == 5 {
                    ret = TYPE_TRANSLATION;
                }
                let (m0, m1) = (t.m01, t.m10);
                ret |= if (m0 >= 0.0) != (m1 >= 0.0) {
                    if m0 != -m1 {
                        12
                    } else if m0 != 1.0 && m0 != -1.0 {
                        10
                    } else {
                        8
                    }
                } else if m0 == m1 {
                    74
                } else {
                    76
                };
            }
            3 | 2 => {
                if t.state == 3 {
                    ret = TYPE_TRANSLATION;
                }
                let (m0, m1) = (t.m00, t.m11);
                let sgn0 = m0 >= 0.0;
                ret |= if sgn0 == (m1 >= 0.0) {
                    if sgn0 {
                        if m0 == m1 {
                            2
                        } else {
                            4
                        }
                    } else if m0 != m1 {
                        12
                    } else if m0 != -1.0 {
                        10
                    } else {
                        8
                    }
                } else if m0 == -m1 {
                    if m0 == 1.0 || m0 == -1.0 {
                        64
                    } else {
                        66
                    }
                } else {
                    68
                };
            }
            1 => ret = TYPE_TRANSLATION,
            _ => {}
        }
        ret
    }

    pub fn update_state(&mut self) {
        if self.m01 == 0.0 && self.m10 == 0.0 {
            if self.m00 == 1.0 && self.m11 == 1.0 {
                if self.m02 == 0.0 && self.m12 == 0.0 {
                    self.state = APPLY_IDENTITY;
                    self.type_ = TYPE_IDENTITY;
                } else {
                    self.state = APPLY_TRANSLATE;
                    self.type_ = TYPE_TRANSLATION;
                }
            } else {
                self.state = if self.m02 == 0.0 && self.m12 == 0.0 { APPLY_SCALE } else { APPLY_SCALE | APPLY_TRANSLATE };
                self.type_ = TYPE_UNKNOWN;
            }
        } else {
            self.state = if self.m00 == 0.0 && self.m11 == 0.0 {
                if self.m02 == 0.0 && self.m12 == 0.0 {
                    APPLY_SHEAR
                } else {
                    APPLY_SHEAR | APPLY_TRANSLATE
                }
            } else if self.m02 == 0.0 && self.m12 == 0.0 {
                APPLY_SHEAR | APPLY_SCALE
            } else {
                APPLY_SHEAR | APPLY_SCALE | APPLY_TRANSLATE
            };
            self.type_ = TYPE_UNKNOWN;
        }
    }

    pub fn translate(&mut self, tx: f64, ty: f64) {
        match self.state {
            7 => {
                self.m02 = tx * self.m00 + ty * self.m01 + self.m02;
                self.m12 = tx * self.m10 + ty * self.m11 + self.m12;
                if self.m02 == 0.0 && self.m12 == 0.0 {
                    self.state = APPLY_SHEAR | APPLY_SCALE;
                    if self.type_ != TYPE_UNKNOWN {
                        self.type_ -= TYPE_TRANSLATION;
                    }
                }
            }
            6 => {
                self.m02 = tx * self.m00 + ty * self.m01;
                self.m12 = tx * self.m10 + ty * self.m11;
                if self.m02 != 0.0 || self.m12 != 0.0 {
                    self.state = 7;
                    self.type_ |= TYPE_TRANSLATION;
                }
            }
            5 => {
                self.m02 = ty * self.m01 + self.m02;
                self.m12 = tx * self.m10 + self.m12;
                if self.m02 == 0.0 && self.m12 == 0.0 {
                    self.state = APPLY_SHEAR;
                    if self.type_ != TYPE_UNKNOWN {
                        self.type_ -= TYPE_TRANSLATION;
                    }
                }
            }
            4 => {
                self.m02 = ty * self.m01;
                self.m12 = tx * self.m10;
                if self.m02 != 0.0 || self.m12 != 0.0 {
                    self.state = 5;
                    self.type_ |= TYPE_TRANSLATION;
                }
            }
            3 => {
                self.m02 = tx * self.m00 + self.m02;
                self.m12 = ty * self.m11 + self.m12;
                if self.m02 == 0.0 && self.m12 == 0.0 {
                    self.state = APPLY_SCALE;
                    if self.type_ != TYPE_UNKNOWN {
                        self.type_ -= TYPE_TRANSLATION;
                    }
                }
            }
            2 => {
                self.m02 = tx * self.m00;
                self.m12 = ty * self.m11;
                if self.m02 != 0.0 || self.m12 != 0.0 {
                    self.state = 3;
                    self.type_ |= TYPE_TRANSLATION;
                }
            }
            1 => {
                self.m02 = tx + self.m02;
                self.m12 = ty + self.m12;
                if self.m02 == 0.0 && self.m12 == 0.0 {
                    self.state = APPLY_IDENTITY;
                    self.type_ = TYPE_IDENTITY;
                }
            }
            _ => {
                self.m02 = tx;
                self.m12 = ty;
                if tx != 0.0 || ty != 0.0 {
                    self.state = APPLY_TRANSLATE;
                    self.type_ = TYPE_TRANSLATION;
                }
            }
        }
    }

    fn rotate90(&mut self) {
        let m0 = self.m00;
        self.m00 = self.m01;
        self.m01 = -m0;
        let m0 = self.m10;
        self.m10 = self.m11;
        self.m11 = -m0;
        let mut state = ROT90_CONVERSION[self.state as usize];
        if (state & (APPLY_SHEAR | APPLY_SCALE)) == APPLY_SCALE && self.m00 == 1.0 && self.m11 == 1.0 {
            state -= APPLY_SCALE;
        }
        self.state = state;
        self.type_ = TYPE_UNKNOWN;
    }

    fn rotate180(&mut self) {
        self.m00 = -self.m00;
        self.m11 = -self.m11;
        let state = self.state;
        if state & APPLY_SHEAR != 0 {
            self.m01 = -self.m01;
            self.m10 = -self.m10;
        } else if self.m00 == 1.0 && self.m11 == 1.0 {
            self.state = state & !APPLY_SCALE;
        } else {
            self.state = state | APPLY_SCALE;
        }
        self.type_ = TYPE_UNKNOWN;
    }

    fn rotate270(&mut self) {
        let m0 = self.m00;
        self.m00 = -self.m01;
        self.m01 = m0;
        let m0 = self.m10;
        self.m10 = -self.m11;
        self.m11 = m0;
        let mut state = ROT90_CONVERSION[self.state as usize];
        if (state & (APPLY_SHEAR | APPLY_SCALE)) == APPLY_SCALE && self.m00 == 1.0 && self.m11 == 1.0 {
            state -= APPLY_SCALE;
        }
        self.state = state;
        self.type_ = TYPE_UNKNOWN;
    }

    pub fn rotate(&mut self, theta: f64) {
        let sin = theta.sin();
        if sin == 1.0 {
            self.rotate90();
        } else if sin == -1.0 {
            self.rotate270();
        } else {
            let cos = theta.cos();
            if cos == -1.0 {
                self.rotate180();
            } else if cos != 1.0 {
                let (m0, m1) = (self.m00, self.m01);
                self.m00 = cos * m0 + sin * m1;
                self.m01 = -sin * m0 + cos * m1;
                let (m0, m1) = (self.m10, self.m11);
                self.m10 = cos * m0 + sin * m1;
                self.m11 = -sin * m0 + cos * m1;
                self.update_state();
            }
        }
    }

    pub fn rotate_at(&mut self, theta: f64, anchorx: f64, anchory: f64) {
        self.translate(anchorx, anchory);
        self.rotate(theta);
        self.translate(-anchorx, -anchory);
    }

    pub fn scale(&mut self, sx: f64, sy: f64) {
        let mut state = self.state;
        match state {
            7 | 6 | 5 | 4 => {
                if state & APPLY_SCALE != 0 {
                    self.m00 *= sx;
                    self.m11 *= sy;
                }
                self.m01 *= sy;
                self.m10 *= sx;
                if self.m01 == 0.0 && self.m10 == 0.0 {
                    state &= APPLY_TRANSLATE;
                    if self.m00 == 1.0 && self.m11 == 1.0 {
                        self.type_ = if state == APPLY_IDENTITY { TYPE_IDENTITY } else { TYPE_TRANSLATION };
                    } else {
                        state |= APPLY_SCALE;
                        self.type_ = TYPE_UNKNOWN;
                    }
                    self.state = state;
                }
            }
            3 | 2 => {
                self.m00 *= sx;
                self.m11 *= sy;
                if self.m00 == 1.0 && self.m11 == 1.0 {
                    state &= APPLY_TRANSLATE;
                    self.state = state;
                    self.type_ = if state == APPLY_IDENTITY { TYPE_IDENTITY } else { TYPE_TRANSLATION };
                } else {
                    self.type_ = TYPE_UNKNOWN;
                }
            }
            _ => {
                self.m00 = sx;
                self.m11 = sy;
                if sx != 1.0 || sy != 1.0 {
                    self.state = state | APPLY_SCALE;
                    self.type_ = TYPE_UNKNOWN;
                }
            }
        }
    }

    pub fn shear(&mut self, shx: f64, shy: f64) {
        let state = self.state;
        match state {
            7 | 6 => {
                let (m0, m1) = (self.m00, self.m01);
                self.m00 = m0 + m1 * shy;
                self.m01 = m0 * shx + m1;
                let (m0, m1) = (self.m10, self.m11);
                self.m10 = m0 + m1 * shy;
                self.m11 = m0 * shx + m1;
                self.update_state();
            }
            5 | 4 => {
                self.m00 = self.m01 * shy;
                self.m11 = self.m10 * shx;
                if self.m00 != 0.0 || self.m11 != 0.0 {
                    self.state = state | APPLY_SCALE;
                }
                self.type_ = TYPE_UNKNOWN;
            }
            3 | 2 => {
                self.m01 = self.m00 * shx;
                self.m10 = self.m11 * shy;
                if self.m01 != 0.0 || self.m10 != 0.0 {
                    self.state = state | APPLY_SHEAR;
                }
                self.type_ = TYPE_UNKNOWN;
            }
            _ => {
                self.m01 = shx;
                self.m10 = shy;
                if self.m01 != 0.0 || self.m10 != 0.0 {
                    self.state = state | APPLY_SCALE | APPLY_SHEAR;
                    self.type_ = TYPE_UNKNOWN;
                }
            }
        }
    }

    pub fn set_to_translation(&mut self, tx: f64, ty: f64) {
        self.m00 = 1.0;
        self.m10 = 0.0;
        self.m01 = 0.0;
        self.m11 = 1.0;
        self.m02 = tx;
        self.m12 = ty;
        if tx != 0.0 || ty != 0.0 {
            self.state = APPLY_TRANSLATE;
            self.type_ = TYPE_TRANSLATION;
        } else {
            self.state = APPLY_IDENTITY;
            self.type_ = TYPE_IDENTITY;
        }
    }

    pub fn set_to_scale(&mut self, sx: f64, sy: f64) {
        self.m00 = sx;
        self.m10 = 0.0;
        self.m01 = 0.0;
        self.m11 = sy;
        self.m02 = 0.0;
        self.m12 = 0.0;
        if sx != 1.0 || sy != 1.0 {
            self.state = APPLY_SCALE;
            self.type_ = TYPE_UNKNOWN;
        } else {
            self.state = APPLY_IDENTITY;
            self.type_ = TYPE_IDENTITY;
        }
    }

    pub fn set_to_rotation(&mut self, theta: f64) {
        let mut sin = theta.sin();
        let mut cos = theta.cos();
        if sin == 1.0 || sin == -1.0 {
            cos = 0.0;
            self.state = APPLY_SHEAR;
            self.type_ = TYPE_QUADRANT_ROTATION;
        } else if cos == -1.0 || cos == 1.0 {
            sin = 0.0;
            if cos == 1.0 {
                self.state = APPLY_IDENTITY;
                self.type_ = TYPE_IDENTITY;
            } else {
                self.state = APPLY_SCALE;
                self.type_ = TYPE_QUADRANT_ROTATION;
            }
        } else {
            self.state = APPLY_SHEAR | APPLY_SCALE;
            self.type_ = TYPE_GENERAL_ROTATION;
        }
        self.m00 = cos;
        self.m10 = sin;
        self.m01 = -sin;
        self.m11 = cos;
        self.m02 = 0.0;
        self.m12 = 0.0;
    }

    pub fn set_to_rotation_at(&mut self, theta: f64, anchorx: f64, anchory: f64) {
        self.set_to_rotation(theta);
        let sin = self.m10;
        let one_minus_cos = 1.0 - self.m00;
        self.m02 = anchorx * one_minus_cos + anchory * sin;
        self.m12 = anchory * one_minus_cos - anchorx * sin;
        if self.m02 != 0.0 || self.m12 != 0.0 {
            self.state |= APPLY_TRANSLATE;
            self.type_ |= TYPE_TRANSLATION;
        }
    }

    pub fn concatenate(&mut self, tx: &AffineTransform) {
        let mystate = self.state;
        let txstate = tx.state;
        match (txstate << HI_SHIFT) | mystate {
            s if s >> HI_SHIFT == 0 => return,
            s if s == HI_SHEAR | HI_SCALE | HI_TRANSLATE | APPLY_IDENTITY => {
                self.m01 = tx.m01;
                self.m10 = tx.m10;
                self.m00 = tx.m00;
                self.m11 = tx.m11;
                self.m02 = tx.m02;
                self.m12 = tx.m12;
                self.state = txstate;
                self.type_ = tx.type_;
                return;
            }
            s if s == HI_SCALE | HI_TRANSLATE | APPLY_IDENTITY => {
                self.m00 = tx.m00;
                self.m11 = tx.m11;
                self.m02 = tx.m02;
                self.m12 = tx.m12;
                self.state = txstate;
                self.type_ = tx.type_;
                return;
            }
            s if s == HI_TRANSLATE | APPLY_IDENTITY => {
                self.m02 = tx.m02;
                self.m12 = tx.m12;
                self.state = txstate;
                self.type_ = tx.type_;
                return;
            }
            s if s == HI_SHEAR | HI_SCALE | APPLY_IDENTITY => {
                self.m01 = tx.m01;
                self.m10 = tx.m10;
                self.m00 = tx.m00;
                self.m11 = tx.m11;
                self.state = txstate;
                self.type_ = tx.type_;
                return;
            }
            s if s == HI_SCALE | APPLY_IDENTITY => {
                self.m00 = tx.m00;
                self.m11 = tx.m11;
                self.state = txstate;
                self.type_ = tx.type_;
                return;
            }
            s if s == HI_SHEAR | HI_TRANSLATE | APPLY_IDENTITY => {
                self.m02 = tx.m02;
                self.m12 = tx.m12;
                self.m01 = tx.m01;
                self.m10 = tx.m10;
                self.m00 = 0.0;
                self.m11 = 0.0;
                self.state = txstate;
                self.type_ = tx.type_;
                return;
            }
            s if s == HI_SHEAR | APPLY_IDENTITY => {
                self.m01 = tx.m01;
                self.m10 = tx.m10;
                self.m00 = 0.0;
                self.m11 = 0.0;
                self.state = txstate;
                self.type_ = tx.type_;
                return;
            }
            s if s >> HI_SHIFT == APPLY_TRANSLATE => {
                self.translate(tx.m02, tx.m12);
                return;
            }
            s if s >> HI_SHIFT == APPLY_SCALE => {
                self.scale(tx.m00, tx.m11);
                return;
            }
            s if s == HI_SHEAR | 7 || s == HI_SHEAR | 6 => {
                let (t01, t10) = (tx.m01, tx.m10);
                let m0 = self.m00;
                self.m00 = self.m01 * t10;
                self.m01 = m0 * t01;
                let m0 = self.m10;
                self.m10 = self.m11 * t10;
                self.m11 = m0 * t01;
                self.type_ = TYPE_UNKNOWN;
                return;
            }
            s if s == HI_SHEAR | 5 || s == HI_SHEAR | 4 => {
                self.m00 = self.m01 * tx.m10;
                self.m01 = 0.0;
                self.m11 = self.m10 * tx.m01;
                self.m10 = 0.0;
                self.state = mystate ^ (APPLY_SHEAR | APPLY_SCALE);
                self.type_ = TYPE_UNKNOWN;
                return;
            }
            s if s == HI_SHEAR | 3 || s == HI_SHEAR | 2 => {
                self.m01 = self.m00 * tx.m01;
                self.m00 = 0.0;
                self.m10 = self.m11 * tx.m10;
                self.m11 = 0.0;
                self.state = mystate ^ (APPLY_SHEAR | APPLY_SCALE);
                self.type_ = TYPE_UNKNOWN;
                return;
            }
            s if s == HI_SHEAR | 1 => {
                self.m00 = 0.0;
                self.m01 = tx.m01;
                self.m10 = tx.m10;
                self.m11 = 0.0;
                self.state = APPLY_TRANSLATE | APPLY_SHEAR;
                self.type_ = TYPE_UNKNOWN;
                return;
            }
            _ => {}
        }
        let (t00, t01, t02, t10, t11, t12) = (tx.m00, tx.m01, tx.m02, tx.m10, tx.m11, tx.m12);
        match mystate {
            6 | 7 => {
                if mystate == 6 {
                    self.state = mystate | txstate;
                }
                let (m0, m1) = (self.m00, self.m01);
                self.m00 = t00 * m0 + t10 * m1;
                self.m01 = t01 * m0 + t11 * m1;
                self.m02 += t02 * m0 + t12 * m1;
                let (m0, m1) = (self.m10, self.m11);
                self.m10 = t00 * m0 + t10 * m1;
                self.m11 = t01 * m0 + t11 * m1;
                self.m12 += t02 * m0 + t12 * m1;
                self.type_ = TYPE_UNKNOWN;
                return;
            }
            5 | 4 => {
                let m0 = self.m01;
                self.m00 = t10 * m0;
                self.m01 = t11 * m0;
                self.m02 += t12 * m0;
                let m0 = self.m10;
                self.m10 = t00 * m0;
                self.m11 = t01 * m0;
                self.m12 += t02 * m0;
            }
            3 | 2 => {
                let m0 = self.m00;
                self.m00 = t00 * m0;
                self.m01 = t01 * m0;
                self.m02 += t02 * m0;
                let m0 = self.m11;
                self.m10 = t10 * m0;
                self.m11 = t11 * m0;
                self.m12 += t12 * m0;
            }
            1 => {
                self.m00 = t00;
                self.m01 = t01;
                self.m02 += t02;
                self.m10 = t10;
                self.m11 = t11;
                self.m12 += t12;
                self.state = txstate | APPLY_TRANSLATE;
                self.type_ = TYPE_UNKNOWN;
                return;
            }
            _ => unreachable!(),
        }
        self.update_state();
    }

    pub fn pre_concatenate(&mut self, tx: &AffineTransform) {
        let mut mystate = self.state;
        let txstate = tx.state;
        let s = (txstate << HI_SHIFT) | mystate;
        match s {
            0..=7 => return,
            8 | 10 | 12 | 14 => {
                self.m02 = tx.m02;
                self.m12 = tx.m12;
                self.state = mystate | APPLY_TRANSLATE;
                self.type_ |= TYPE_TRANSLATION;
                return;
            }
            9 | 11 | 13 | 15 => {
                self.m02 += tx.m02;
                self.m12 += tx.m12;
                return;
            }
            16..=23 => {
                if s <= 17 {
                    self.state = mystate | APPLY_SCALE;
                }
                let (t00, t11) = (tx.m00, tx.m11);
                if mystate & APPLY_SHEAR != 0 {
                    self.m01 *= t00;
                    self.m10 *= t11;
                    if mystate & APPLY_SCALE != 0 {
                        self.m00 *= t00;
                        self.m11 *= t11;
                    }
                } else {
                    self.m00 *= t00;
                    self.m11 *= t11;
                }
                if mystate & APPLY_TRANSLATE != 0 {
                    self.m02 *= t00;
                    self.m12 *= t11;
                }
                self.type_ = TYPE_UNKNOWN;
                return;
            }
            32..=39 => {
                if s == 36 || s == 37 {
                    mystate |= APPLY_SCALE;
                }
                if s <= 37 {
                    self.state = mystate ^ APPLY_SHEAR;
                }
                let (t01, t10) = (tx.m01, tx.m10);
                let m0 = self.m00;
                self.m00 = self.m10 * t01;
                self.m10 = m0 * t10;
                let m0 = self.m01;
                self.m01 = self.m11 * t01;
                self.m11 = m0 * t10;
                let m0 = self.m02;
                self.m02 = self.m12 * t01;
                self.m12 = m0 * t10;
                self.type_ = TYPE_UNKNOWN;
                return;
            }
            _ => {}
        }
        let (t00, t01, mut t02, t10, t11, mut t12) = (tx.m00, tx.m01, tx.m02, tx.m10, tx.m11, tx.m12);
        if mystate & APPLY_TRANSLATE != 0 {
            let (m0, m1) = (self.m02, self.m12);
            t02 += m0 * t00 + m1 * t01;
            t12 += m0 * t10 + m1 * t11;
        }
        self.m02 = t02;
        self.m12 = t12;
        match mystate & !APPLY_TRANSLATE {
            6 => {
                let (m0, m1) = (self.m00, self.m10);
                self.m00 = m0 * t00 + m1 * t01;
                self.m10 = m0 * t10 + m1 * t11;
                let (m0, m1) = (self.m01, self.m11);
                self.m01 = m0 * t00 + m1 * t01;
                self.m11 = m0 * t10 + m1 * t11;
            }
            4 => {
                let m0 = self.m10;
                self.m00 = m0 * t01;
                self.m10 = m0 * t11;
                let m0 = self.m01;
                self.m01 = m0 * t00;
                self.m11 = m0 * t10;
            }
            2 => {
                let m0 = self.m00;
                self.m00 = m0 * t00;
                self.m10 = m0 * t10;
                let m0 = self.m11;
                self.m01 = m0 * t01;
                self.m11 = m0 * t11;
            }
            _ => {
                self.m00 = t00;
                self.m10 = t10;
                self.m01 = t01;
                self.m11 = t11;
                self.state = mystate | txstate;
                self.type_ = TYPE_UNKNOWN;
                return;
            }
        }
        self.update_state();
    }

    pub fn create_inverse(&self) -> Result<AffineTransform, NoninvertibleTransformException> {
        let (m00, m01, m02, m10, m11, m12) = (self.m00, self.m01, self.m02, self.m10, self.m11, self.m12);
        match self.state {
            7 => {
                let det = m00 * m11 - m01 * m10;
                if det.abs() <= f64::from_bits(1) {
                    return Err(NoninvertibleTransformException);
                }
                Ok(AffineTransform::with_state(m11 / det, -m10 / det, -m01 / det, m00 / det, (m01 * m12 - m11 * m02) / det, (m10 * m02 - m00 * m12) / det, 7))
            }
            6 => {
                let det = m00 * m11 - m01 * m10;
                if det.abs() <= f64::from_bits(1) {
                    return Err(NoninvertibleTransformException);
                }
                Ok(AffineTransform::with_state(m11 / det, -m10 / det, -m01 / det, m00 / det, 0.0, 0.0, 6))
            }
            5 => {
                if m01 == 0.0 || m10 == 0.0 {
                    return Err(NoninvertibleTransformException);
                }
                Ok(AffineTransform::with_state(0.0, 1.0 / m01, 1.0 / m10, 0.0, -m12 / m10, -m02 / m01, 5))
            }
            4 => {
                if m01 == 0.0 || m10 == 0.0 {
                    return Err(NoninvertibleTransformException);
                }
                Ok(AffineTransform::with_state(0.0, 1.0 / m01, 1.0 / m10, 0.0, 0.0, 0.0, 4))
            }
            3 => {
                if m00 == 0.0 || m11 == 0.0 {
                    return Err(NoninvertibleTransformException);
                }
                Ok(AffineTransform::with_state(1.0 / m00, 0.0, 0.0, 1.0 / m11, -m02 / m00, -m12 / m11, 3))
            }
            2 => {
                if m00 == 0.0 || m11 == 0.0 {
                    return Err(NoninvertibleTransformException);
                }
                Ok(AffineTransform::with_state(1.0 / m00, 0.0, 0.0, 1.0 / m11, 0.0, 0.0, 2))
            }
            1 => Ok(AffineTransform::with_state(1.0, 0.0, 0.0, 1.0, -m02, -m12, 1)),
            _ => Ok(AffineTransform::new()),
        }
    }

    /// `transform(ptSrc, ptDst)` for one point, in doubles.
    pub fn transform_point(&self, x: f64, y: f64) -> (f64, f64) {
        let (m00, m01, m02, m10, m11, m12) = (self.m00, self.m01, self.m02, self.m10, self.m11, self.m12);
        match self.state {
            7 => (x * m00 + y * m01 + m02, x * m10 + y * m11 + m12),
            6 => (x * m00 + y * m01, x * m10 + y * m11),
            5 => (y * m01 + m02, x * m10 + m12),
            4 => (y * m01, x * m10),
            3 => (x * m00 + m02, y * m11 + m12),
            2 => (x * m00, y * m11),
            1 => (x + m02, y + m12),
            _ => (x, y),
        }
    }

    /// `transform(double[] src, 0, double[] dst, 0, n)` in place.
    pub fn transform_coords(&self, pts: &mut [f64]) {
        for p in pts.chunks_mut(2) {
            let (x, y) = self.transform_point(p[0], p[1]);
            p[0] = x;
            p[1] = y;
        }
    }

    /// `deltaTransform(ptSrc, ptDst)`.
    pub fn delta_transform(&self, x: f64, y: f64) -> (f64, f64) {
        match self.state {
            7 | 6 => (x * self.m00 + y * self.m01, x * self.m10 + y * self.m11),
            5 | 4 => (y * self.m01, x * self.m10),
            3 | 2 => (x * self.m00, y * self.m11),
            _ => (x, y),
        }
    }
}
