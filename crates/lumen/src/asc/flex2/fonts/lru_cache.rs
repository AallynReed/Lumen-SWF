//! Port of swfutils `flash.util.LRUCache`: a map whose explicitly put entries form a most-recently-used list and
//! are purged from its tail, `purge_size` at a time, once the map outgrows `max_size`. A `get` miss caches what
//! `fetch` returns, `None` included. (`IntMapLRUCache`, behind the glyph cache and `FontSet`, never purges in
//! the ways swfutils uses it, so those are plain maps.)

pub struct LRUCache<K, V> {
    /// The map, in no particular order.
    map: Vec<(K, V)>,
    /// Keys from most to least recently used.
    list: Vec<K>,
    max_size: usize,
    purge_size: usize,
}

impl<K: PartialEq + Clone, V: Clone> LRUCache<K, V> {
    pub fn new(_initial_size: i32, max_size: i32, purge_size: i32) -> LRUCache<K, V> {
        LRUCache { map: Vec::new(), list: Vec::new(), max_size: max_size.max(0) as usize, purge_size: purge_size.max(0) as usize }
    }

    /// `get(key)`, with `fetch` run on a miss and its result `put`.
    pub fn get_or_fetch(&mut self, key: &K, fetch: impl FnOnce() -> V) -> V {
        if let Some(v) = self.lookup(key) {
            return v;
        }
        let v = fetch();
        self.put(key.clone(), v.clone());
        v
    }

    /// The hit half of `get`: marks the entry most recently used.
    pub fn lookup(&mut self, key: &K) -> Option<V> {
        let i = self.map.iter().position(|(k, _)| k == key)?;
        let v = self.map[i].1.clone();
        self.set_most_recently_used(key);
        Some(v)
    }

    pub fn put(&mut self, key: K, value: V) {
        self.remove(&key);
        self.map.push((key.clone(), value));
        self.list.insert(0, key);
        if self.map.len() > self.max_size {
            self.purge_lru_elements();
        }
    }

    pub fn remove(&mut self, key: &K) {
        if let Some(i) = self.map.iter().position(|(k, _)| k == key) {
            self.map.remove(i);
            if let Some(j) = self.list.iter().position(|k| k == key) {
                self.list.remove(j);
            }
        }
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.list.clear();
    }

    pub fn size(&self) -> usize {
        self.map.len()
    }

    fn purge_lru_elements(&mut self) {
        for _ in 0..self.purge_size {
            let Some(key) = self.list.last().cloned() else { break };
            self.remove(&key);
        }
    }

    fn set_most_recently_used(&mut self, key: &K) {
        if let Some(j) = self.list.iter().position(|k| k == key) {
            let k = self.list.remove(j);
            self.list.insert(0, k);
        }
    }
}
