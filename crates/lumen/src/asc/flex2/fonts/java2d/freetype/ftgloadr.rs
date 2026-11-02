//! Port of FreeType 2.14.2 `src/base/ftgloadr.c` (as built into JDK 17.0.19). The `current` outline is the tail of the base arrays starting at the base counts.

use crate::asc::flex2::fonts::java2d::freetype::fterrors::*;
use crate::asc::flex2::fonts::java2d::freetype::fttypes::*;

#[derive(Clone, Copy, Default, Debug)]
pub struct FtSubGlyph {
    pub index: i32,
    pub flags: u16,
    pub arg1: i32,
    pub arg2: i32,
    pub transform: FtMatrix,
}

#[derive(Clone, Default)]
pub struct FtGlyphLoader {
    pub max_points: u32,
    pub max_contours: u32,
    pub max_subglyphs: u32,
    pub use_extra: bool,
    pub points: Vec<FtVector>,
    pub tags: Vec<u8>,
    pub contours: Vec<u16>,
    pub extra_points: Vec<FtVector>,
    pub extra_points2: Vec<FtVector>,
    pub subglyphs: Vec<FtSubGlyph>,
    pub base_n_points: u16,
    pub base_n_contours: u16,
    pub base_flags: i32,
    pub base_num_subglyphs: u32,
    pub cur_n_points: u16,
    pub cur_n_contours: u16,
    pub cur_num_subglyphs: u32,
}

impl FtGlyphLoader {
    pub fn rewind(&mut self) {
        self.base_n_points = 0;
        self.base_n_contours = 0;
        self.base_flags = 0;
        self.base_num_subglyphs = 0;
        self.cur_n_points = 0;
        self.cur_n_contours = 0;
        self.cur_num_subglyphs = 0;
    }

    pub fn reset(&mut self) {
        self.points = Vec::new();
        self.tags = Vec::new();
        self.contours = Vec::new();
        self.extra_points = Vec::new();
        self.extra_points2 = Vec::new();
        self.subglyphs = Vec::new();
        self.max_points = 0;
        self.max_contours = 0;
        self.max_subglyphs = 0;
        self.rewind();
    }

    pub fn create_extra(&mut self) {
        if self.max_points == 0 || !self.extra_points.is_empty() {
            return;
        }
        self.extra_points = vec![FtVector::default(); self.max_points as usize];
        self.extra_points2 = vec![FtVector::default(); self.max_points as usize];
        self.use_extra = true;
    }

    pub fn check_points(&mut self, n_points: u32, n_contours: u32) -> FtError {
        self.create_extra();
        let mut new_max = self.base_n_points as u32 + self.cur_n_points as u32 + n_points;
        let old_max = self.max_points;
        if new_max > old_max {
            if new_max > FT_OUTLINE_POINTS_MAX {
                self.reset();
                return FT_ERR_ARRAY_TOO_LARGE;
            }
            let min_new_max = old_max + (old_max >> 1);
            if new_max < min_new_max {
                new_max = min_new_max;
            }
            new_max = (new_max + 7) & !7;
            if new_max > FT_OUTLINE_POINTS_MAX {
                new_max = FT_OUTLINE_POINTS_MAX;
            }
            self.points.resize(new_max as usize, FtVector::default());
            self.tags.resize(new_max as usize, 0);
            if self.use_extra {
                self.extra_points.resize(new_max as usize, FtVector::default());
                self.extra_points2.resize(new_max as usize, FtVector::default());
            }
            self.max_points = new_max;
        }
        self.create_extra();
        let old_max = self.max_contours;
        let mut new_max = self.base_n_contours as u32 + self.cur_n_contours as u32 + n_contours;
        if new_max > old_max {
            if new_max > FT_OUTLINE_CONTOURS_MAX {
                self.reset();
                return FT_ERR_ARRAY_TOO_LARGE;
            }
            let min_new_max = old_max + (old_max >> 1);
            if new_max < min_new_max {
                new_max = min_new_max;
            }
            new_max = (new_max + 3) & !3;
            if new_max > FT_OUTLINE_CONTOURS_MAX {
                new_max = FT_OUTLINE_CONTOURS_MAX;
            }
            self.contours.resize(new_max as usize, 0);
            self.max_contours = new_max;
        }
        FT_ERR_OK
    }

    pub fn check_subglyphs(&mut self, n_subs: u32) -> FtError {
        let mut new_max = self.base_num_subglyphs + self.cur_num_subglyphs + n_subs;
        if new_max > self.max_subglyphs {
            new_max = (new_max + 1) & !1;
            self.subglyphs.resize(new_max as usize, FtSubGlyph::default());
            self.max_subglyphs = new_max;
        }
        FT_ERR_OK
    }

    pub fn prepare(&mut self) {
        self.cur_n_points = 0;
        self.cur_n_contours = 0;
        self.cur_num_subglyphs = 0;
    }

    pub fn add(&mut self) {
        let bp = self.base_n_points;
        let bc = self.base_n_contours as usize;
        for n in 0..self.cur_n_contours as usize {
            self.contours[bc + n] = self.contours[bc + n].wrapping_add(bp);
        }
        self.base_n_points = self.base_n_points.wrapping_add(self.cur_n_points);
        self.base_n_contours = self.base_n_contours.wrapping_add(self.cur_n_contours);
        self.base_num_subglyphs += self.cur_num_subglyphs;
        self.prepare();
    }
}
