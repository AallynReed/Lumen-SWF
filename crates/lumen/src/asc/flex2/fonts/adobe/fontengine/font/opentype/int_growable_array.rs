//! Port of afe.jar `com.adobe.fontengine.font.opentype.IntGrowableArray`.

#[derive(Clone, Debug)]
pub struct IntGrowableArray {
    buffer: Vec<i32>,
    length: i32,
}

impl Default for IntGrowableArray {
    fn default() -> Self {
        Self::new()
    }
}

impl IntGrowableArray {
    pub fn new() -> IntGrowableArray {
        IntGrowableArray { buffer: vec![0; 50], length: 0 }
    }

    fn grow(&mut self, n: i32) {
        if n as usize >= self.buffer.len() {
            let mut n2 = self.buffer.len();
            while n as usize >= n2 {
                n2 += 50;
            }
            self.buffer.resize(n2, 0);
        }
    }

    pub fn set(&mut self, n: i32) {
        let at = self.length;
        self.set_at(n, at);
    }

    pub fn set_at(&mut self, n: i32, n2: i32) {
        self.grow(n2);
        self.buffer[n2 as usize] = n;
        if self.length <= n2 {
            self.length = n2 + 1;
        }
    }

    pub fn get(&self, n: i32) -> i32 {
        if n < self.length {
            return self.buffer[n as usize];
        }
        0
    }

    pub fn get_buffer(&self) -> &[i32] {
        &self.buffer
    }

    pub fn get_buffer_mut(&mut self) -> &mut [i32] {
        &mut self.buffer
    }

    pub fn get_length(&self) -> i32 {
        self.length
    }
}
