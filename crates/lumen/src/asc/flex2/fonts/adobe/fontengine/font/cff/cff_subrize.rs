//! Port of afe.jar `com.adobe.fontengine.font.cff.CFFSubrize`.

use std::cmp::Ordering;

use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cff_byte_array::CFFByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::char_strings::CharStrings;

const MAX_CALL_LIST_COUNT: usize = 3;
const NODE_GLOBAL: i32 = i16::MAX as i32;
const EDGE_TABLE_SMALLEST_SPARSE_SIZE: i32 = 128;
const EDGE_TABLE_SIZE_USE_SIMPLE_HASH: i32 = 16;
const SUBR_TABLE_SIZE_USE_SIMPLE_HASH: usize = 262144;
const LISTSIZE: usize = 4000;
const TX_MAX_CALL_STACK: i32 = 10;
const T2_SEPARATOR: u8 = 9;
const TX_CALLSUBR: u8 = 10;
const TX_RETURN: u8 = 11;
const TX_ENDCHAR: i32 = 14;
const T2_HINTMASK: i32 = 19;
const T2_CNTRMASK: i32 = 20;
const T2_CALLGSUBR: u8 = 29;

type NodeId = usize;
type EdgeId = usize;
type SubrId = usize;
type LinkId = usize;

struct Node {
    suffix: Option<NodeId>,
    edge_table: Option<Vec<Option<EdgeId>>>,
    misc: i32,
    edge_count: i32,
    edge_table_size: i32,
    paths: i16,
    id: i16,
    counted: bool,
    tested: bool,
    tail: bool,
}

#[derive(Default)]
struct Edge {
    label: i32,
    son: Option<NodeId>,
    length: i32,
}

#[derive(Default)]
struct Subr {
    node: NodeId,
    sups: Option<LinkId>,
    infs: Option<LinkId>,
    next: Option<SubrId>,
    cstr: i32,
    length: i16,
    count: i16,
    deltalen: i16,
    subrnum: i16,
    numsize: i16,
    maskcnt: i16,
    misc: i16,
    select: bool,
    reject: bool,
    member: bool,
}

struct Link {
    subr: SubrId,
    next: Option<LinkId>,
    offset: i32,
}

#[derive(Clone, Copy)]
struct Call {
    subr: SubrId,
    offset: i32,
}

#[derive(Default)]
struct SubrCSData {
    n_strings: i32,
    offset: Vec<i32>,
    data: Vec<u8>,
}

#[derive(Default)]
struct SubrFDInfo {
    subrs: SubrCSData,
    chars: SubrCSData,
    i_char: i32,
}

#[derive(Default)]
struct SubrFont {
    subr_font_cid: bool,
    fd_index: Vec<u8>,
    fd_count: i32,
    fd_info: Option<Vec<SubrFDInfo>>,
    subrs: SubrCSData,
    chars: SubrCSData,
}

/// Java's `Collections.sort` is stable; this merge sort is too, and never panics on a bad comparator.
fn stable_sort<T: Copy>(v: &mut Vec<T>, cmp: &dyn Fn(&T, &T) -> Ordering) {
    if v.len() < 2 {
        return;
    }
    let mid = v.len() / 2;
    let mut a = v[..mid].to_vec();
    let mut b = v[mid..].to_vec();
    stable_sort(&mut a, cmp);
    stable_sort(&mut b, cmp);
    let (mut i, mut j, mut k) = (0, 0, 0);
    while i < a.len() && j < b.len() {
        if cmp(&b[j], &a[i]) == Ordering::Less {
            v[k] = b[j];
            j += 1;
        } else {
            v[k] = a[i];
            i += 1;
        }
        k += 1;
    }
    while i < a.len() {
        v[k] = a[i];
        i += 1;
        k += 1;
    }
    while j < b.len() {
        v[k] = b[j];
        j += 1;
        k += 1;
    }
}

pub struct CFFSubrize {
    m_char_str: Vec<u8>,
    m_root: Option<NodeId>,
    m_base: Option<NodeId>,
    m_sinks: Vec<Option<NodeId>>,
    m_base_edge: EdgeId,
    m_subrs: Vec<SubrId>,
    m_tmp: Vec<SubrId>,
    m_reorder: Option<Vec<SubrId>>,
    m_calls: Vec<Call>,
    m_members: Vec<SubrId>,
    m_leaders: Vec<SubrId>,
    m_singleton: bool,
    m_off_size: i32,
    m_subr_stack_ovl: bool,
    m_gsubrs: Option<SubrCSData>,
    m_num_fonts: i32,
    m_fonts: Vec<SubrFont>,
    m_op_len_cache: [u8; 256],
    m_subr_hash: Vec<Option<SubrId>>,
    m_subr_prefix_map: Vec<u8>,
    m_subr_len_map: Vec<i16>,
    m_prefix_len: Vec<i16>,
    m_max_subr_len: i32,
    m_min_subr_len: i32,
    m_max_num_subrs: i32,
    m_subr_sort_mode: i32,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    subrs: Vec<Subr>,
    links: Vec<Link>,
}

impl Default for CFFSubrize {
    fn default() -> Self {
        CFFSubrize::new()
    }
}

impl CFFSubrize {
    pub fn new() -> CFFSubrize {
        CFFSubrize {
            m_char_str: Vec::new(),
            m_root: None,
            m_base: None,
            m_sinks: Vec::new(),
            m_base_edge: usize::MAX,
            m_subrs: Vec::new(),
            m_tmp: Vec::new(),
            m_reorder: None,
            m_calls: Vec::new(),
            m_members: Vec::new(),
            m_leaders: Vec::new(),
            m_singleton: false,
            m_off_size: 2,
            m_subr_stack_ovl: false,
            m_gsubrs: None,
            m_num_fonts: 0,
            m_fonts: Vec::new(),
            m_op_len_cache: [0; 256],
            m_subr_hash: Vec::new(),
            m_subr_prefix_map: vec![0; 8192],
            m_subr_len_map: Vec::new(),
            m_prefix_len: Vec::new(),
            m_max_subr_len: 0,
            m_min_subr_len: 0,
            m_max_num_subrs: 0,
            m_subr_sort_mode: 0,
            nodes: Vec::new(),
            edges: Vec::new(),
            subrs: Vec::new(),
            links: Vec::new(),
        }
    }

    fn new_node(&mut self, n: i32, n2: i32) -> NodeId {
        self.nodes.push(Node {
            suffix: None,
            edge_table: None,
            misc: n,
            edge_count: 0,
            edge_table_size: 0,
            paths: 0,
            id: n2 as i16,
            counted: false,
            tested: false,
            tail: false,
        });
        self.nodes.len() - 1
    }

    fn new_edge(&mut self) -> EdgeId {
        self.edges.push(Edge::default());
        self.edges.len() - 1
    }

    fn new_subr(&mut self) -> SubrId {
        self.subrs.push(Subr::default());
        self.subrs.len() - 1
    }

    fn new_link(&mut self, subr: SubrId, offset: i32, next: Option<LinkId>) -> LinkId {
        self.links.push(Link { subr, next, offset });
        self.links.len() - 1
    }

    fn cs(&self, i: i32) -> i8 {
        self.m_char_str[i as usize] as i8
    }

    fn cu(&self, i: i32) -> i32 {
        self.m_char_str[i as usize] as i32
    }

    fn son(&self, e: EdgeId) -> NodeId {
        self.edges[e].son.expect("java.lang.NullPointerException: edge.son")
    }

    pub fn subrize(&mut self, cs: CharStrings) -> CharStrings {
        self.subrize_fd(cs, None)
    }

    /// `subrize(CharStrings, byte[])`.
    pub fn subrize_fd(&mut self, cs: CharStrings, fd: Option<Vec<u8>>) -> CharStrings {
        let mut n = cs.get_count();
        let mut offs = vec![0i32; n as usize];
        let n2 = cs.offset_of(0);
        let n3 = cs.size();
        let n4 = n3 - n2;
        for i in 0..n {
            offs[i as usize] = cs.offset_of(i + 1) - n2;
        }
        let src = cs.data.base.get_bytes(n2, n4);
        let mut dst = vec![0u8; src.len() + 4 * n as usize];
        let mut n5 = 0usize;
        let mut n6 = 0usize;
        for i in 0..n {
            let n7 = offs[i as usize] as usize - n5;
            dst[n6..n6 + n7].copy_from_slice(&src[n5..n5 + n7]);
            n5 += n7;
            n6 += n7;
            dst[n6] = T2_SEPARATOR;
            dst[n6 + 1] = (i >> 16) as u8;
            dst[n6 + 2] = (i >> 8) as u8;
            dst[n6 + 3] = i as u8;
            n6 += 4;
            offs[i as usize] = n6 as i32;
        }
        n = self.subrize_raw(n, offs, dst, fd);
        let offs = self.get_main_offsets().to_vec();
        let data = self.get_main_data().to_vec();
        self.build_index(n, &offs, &data)
    }

    pub fn get_l_subrs(&self) -> Option<CharStrings> {
        let n = self.get_l_subr_count();
        if n == 0 {
            return None;
        }
        Some(self.build_index(n, self.get_l_subr_offsets(), self.get_l_subr_data()))
    }

    /// `getLSubrs(int)`.
    pub fn get_l_subrs_fd(&self, n: i32) -> Option<CharStrings> {
        let fi = self.m_fonts[0].fd_info.as_ref()?;
        if n as usize >= fi.len() {
            return None;
        }
        let sd = &fi[n as usize].subrs;
        if sd.n_strings == 0 {
            return None;
        }
        Some(self.build_index(sd.n_strings, &sd.offset, &sd.data))
    }

    pub fn get_g_subrs(&self) -> Option<CharStrings> {
        let n = self.get_g_subr_count();
        if n == 0 {
            return None;
        }
        let g = self.m_gsubrs.as_ref().unwrap();
        Some(self.build_index(n, &g.offset, &g.data))
    }

    pub fn subrize_raw(&mut self, n: i32, offs: Vec<i32>, data: Vec<u8>, fd: Option<Vec<u8>>) -> i32 {
        let mut f = SubrFont::default();
        f.chars.n_strings = n;
        f.chars.offset = offs;
        f.chars.data = data;
        if let Some(fd) = fd {
            f.subr_font_cid = true;
            let mut by: i8 = 0;
            for &b in &fd {
                if b as i8 > by {
                    by = b as i8;
                }
            }
            f.fd_index = fd;
            f.fd_count = by as i32 + 1;
            f.fd_info = Some((0..=by as i32).map(|_| SubrFDInfo::default()).collect());
        }
        self.cfw_subr_subrize(1, vec![f]);
        self.m_fonts[0].chars.n_strings
    }

    pub fn get_main_offsets(&self) -> &[i32] {
        &self.m_fonts[0].chars.offset
    }

    pub fn get_main_data(&self) -> &[u8] {
        &self.m_fonts[0].chars.data
    }

    pub fn get_g_subr_count(&self) -> i32 {
        self.m_gsubrs.as_ref().map_or(0, |g| g.n_strings)
    }

    pub fn get_l_subr_count(&self) -> i32 {
        self.m_fonts[0].subrs.n_strings
    }

    pub fn get_l_subr_offsets(&self) -> &[i32] {
        &self.m_fonts[0].subrs.offset
    }

    pub fn get_l_subr_data(&self) -> &[u8] {
        &self.m_fonts[0].subrs.data
    }

    fn build_index(&self, n: i32, offs: &[i32], data: &[u8]) -> CharStrings {
        let n2 = data.len() as i32 + 1;
        let mut b = CFFByteArray::get_cff_byte_array_builder_instance();
        b.add_card16(n);
        let n3 = if n2 > 0xffffff {
            4
        } else if n2 > 65535 {
            3
        } else if n2 > 255 {
            2
        } else {
            1
        };
        b.add_card8(n3);
        b.add_offset(n3, 1);
        for &o in offs {
            b.add_offset(n3, o + 1);
        }
        b.add_bytes(data, 0, n2 - 1);
        CharStrings::from_builder(b).expect("CFFSubrize.buildIndex")
    }

    fn t2oplen(b: &[u8; 2]) -> i32 {
        match b[0] {
            12 | 247..=254 => 2,
            28 => 3,
            9 => 4,
            255 => 5,
            19 | 20 => b[1] as i32,
            _ => 1,
        }
    }

    fn t2cstrcpy(&self, out: &mut [u8], mut n: i32, mut n2: i32, mut n3: i32) -> i32 {
        let n4 = n2 + n3;
        while n2 < n4 {
            let c = self.cu(n2);
            match c {
                19 | 20 => {
                    out[n as usize] = self.m_char_str[n2 as usize];
                    n += 1;
                    n2 += 1;
                    let mut n5 = self.cs(n2) as i32 - 2;
                    n2 += 1;
                    while n5 != 0 {
                        n5 -= 1;
                        out[n as usize] = self.m_char_str[n2 as usize];
                        n += 1;
                        n2 += 1;
                    }
                    n3 -= 1;
                    continue;
                }
                255 | 28 | 12 | 247..=254 => {
                    let k = match c {
                        255 => 5,
                        28 => 3,
                        _ => 2,
                    };
                    for _ in 0..k {
                        out[n as usize] = self.m_char_str[n2 as usize];
                        n += 1;
                        n2 += 1;
                    }
                    continue;
                }
                _ => {}
            }
            out[n as usize] = self.m_char_str[n2 as usize];
            n += 1;
            n2 += 1;
        }
        let _ = n3;
        n
    }

    fn new_edge_table(&mut self, node: NodeId, n: i32) {
        let nd = &mut self.nodes[node];
        nd.edge_table_size = n;
        nd.edge_count = 0;
        nd.edge_table = Some(vec![None; n as usize]);
    }

    fn init_edge(&mut self, e: EdgeId, n: i32, n2: i32, node: Option<NodeId>) {
        let ed = &mut self.edges[e];
        ed.label = n;
        ed.length = n2;
        ed.son = node;
    }

    fn labelcmp(&self, n: i32, mut n2: i32, mut n3: i32) -> i32 {
        let mut n4 = self.cs(n2) as i32 - self.cs(n3) as i32;
        n2 += 1;
        if n4 != 0 {
            return n4;
        }
        let n5 = self.op_len(n3);
        let mut n6 = if n < n5 { n } else { n5 };
        n3 += 1;
        loop {
            n6 -= 1;
            if n6 == 0 {
                break;
            }
            n4 = self.cs(n2) as i32 - self.cs(n3) as i32;
            n2 += 1;
            n3 += 1;
            if n4 == 0 {
                continue;
            }
            return n4;
        }
        n - n5
    }

    fn hash_label(&self, mut n: i32, mut n2: i32) -> i32 {
        let mut n3 = self.cu(n);
        n3 = n3.wrapping_add(n3 << 5);
        loop {
            n2 -= 1;
            if n2 <= 0 {
                break;
            }
            n += 1;
            let n4 = self.cu(n);
            n3 = n3.wrapping_add(n4);
            n3 = n3.wrapping_shl(5);
            n3 = n3.wrapping_add(n4);
        }
        n3
    }

    fn lookup_edge_table(&mut self, node: NodeId, n: i32, n2: i32) -> Option<EdgeId> {
        let n3 = self.nodes[node].edge_table_size;
        let n4 = n3 - 1;
        let mut n5 = if n3 <= EDGE_TABLE_SIZE_USE_SIMPLE_HASH { self.cu(n2) + n } else { self.hash_label(n2, n) };
        let mut n6: i32 = 0;
        for _ in 0..n3 {
            let slot = (n5 & n4) as usize;
            let e = match self.nodes[node].edge_table.as_ref().unwrap()[slot] {
                Some(e) => e,
                None => {
                    let e = self.new_edge();
                    self.nodes[node].edge_table.as_mut().unwrap()[slot] = Some(e);
                    e
                }
            };
            if self.edges[e].length == 0 {
                return Some(e);
            }
            if self.labelcmp(n, n2, self.edges[e].label) == 0 {
                return Some(e);
            }
            n6 += 1;
            n5 = n5.wrapping_add(n6);
        }
        None
    }

    fn add_edge_to_hash_table(&mut self, node: NodeId, node2: Option<NodeId>, n: i32, n2: i32, n3: i32) {
        let e;
        if self.nodes[node].edge_table.is_none() {
            self.new_edge_table(node, 1);
            e = self.new_edge();
            self.nodes[node].edge_table.as_mut().unwrap()[0] = Some(e);
        } else {
            let nd = &self.nodes[node];
            let bl = nd.edge_count >= nd.edge_table_size
                || nd.edge_table_size >= EDGE_TABLE_SMALLEST_SPARSE_SIZE && nd.edge_count >= nd.edge_table_size - (nd.edge_table_size >> 3);
            if bl {
                self.double_edge_table(node);
            }
            e = self.lookup_edge_table(node, n, n2).expect("java.lang.NullPointerException: full edge table");
        }
        self.init_edge(e, n2, n3, node2);
        self.nodes[node].edge_count += 1;
    }

    fn double_edge_table(&mut self, node: NodeId) {
        let old = self.nodes[node].edge_table.take().unwrap_or_default();
        let n = self.nodes[node].edge_table_size;
        let n2 = n * 2;
        {
            let nd = &mut self.nodes[node];
            nd.edge_table = Some(vec![None; n2 as usize]);
            nd.edge_table_size = n2;
            nd.edge_count = 0;
        }
        for i in 0..n as usize {
            let e = match old[i] {
                Some(e) => e,
                None => continue,
            };
            if self.edges[e].length == 0 {
                continue;
            }
            let (son, label, length) = (self.edges[e].son, self.edges[e].label, self.edges[e].length);
            let ol = self.op_len(label);
            self.add_edge_to_hash_table(node, son, ol, label, length);
        }
    }

    fn add_edge(&mut self, node: NodeId, node2: Option<NodeId>, n: i32, n2: i32, n3: i32) {
        self.add_edge_to_hash_table(node, node2, n, n2, n3);
    }

    fn find_edge(&mut self, node: NodeId, n: i32, n2: i32) -> Option<EdgeId> {
        if self.nodes[node].misc == -1 {
            return Some(self.m_base_edge);
        }
        let e = self.lookup_edge_table(node, n, n2);
        match e {
            None => None,
            Some(e) if self.edges[e].length != 0 => Some(e),
            Some(_) => None,
        }
    }

    fn find_edge_npe(&mut self, node: NodeId, n: i32, n2: i32) -> EdgeId {
        self.find_edge(node, n, n2).expect("java.lang.NullPointerException: findEdge")
    }

    fn copy_edge_table(&mut self, node: NodeId, node2: NodeId) {
        let size = self.nodes[node2].edge_table_size;
        self.new_edge_table(node, size);
        self.nodes[node].edge_count = self.nodes[node2].edge_count;
        for i in 0..size as usize {
            let e = match self.nodes[node2].edge_table.as_ref().unwrap()[i] {
                Some(e) => e,
                None => continue,
            };
            let (label, length, son) = (self.edges[e].label, self.edges[e].length, self.edges[e].son);
            let e2 = self.new_edge();
            self.init_edge(e2, label, length, son);
            self.nodes[node].edge_table.as_mut().unwrap()[i] = Some(e2);
        }
    }

    fn walk_edge_table(&mut self, node: NodeId, n: i32, n2: i32) {
        let size = self.nodes[node].edge_table_size;
        for i in 0..size as usize {
            let e = match self.nodes[node].edge_table.as_ref().unwrap()[i] {
                Some(e) => e,
                None => continue,
            };
            if self.edges[e].length == 0 {
                continue;
            }
            self.find_cand_subrs(e, n, n2);
        }
    }

    fn check_end_point(&mut self, node: NodeId, n: i32, n2: i32, n3: i32, _n4: i32) -> bool {
        if self.nodes[node].misc == -1 {
            return true;
        }
        if n < n2 {
            let ol = self.op_len(n);
            let e = self.find_edge_npe(node, ol, n);
            return self.labelcmp(n3, n2, self.edges[e].label + n2 - n) == 0;
        }
        self.find_edge(node, n3, n2).is_some()
    }

    fn extension(&mut self, node: NodeId, n: i32, n2: i32) -> Option<NodeId> {
        if n >= n2 {
            return Some(node);
        }
        let ol = self.op_len(n);
        let e = self.find_edge_npe(node, ol, n);
        self.edges[e].son
    }

    fn redirect(&mut self, node: NodeId, n: i32, n2: i32, node2: Option<NodeId>) {
        let ol = self.op_len(n);
        let e = self.find_edge_npe(node, ol, n);
        self.edges[e].son = node2;
        self.edges[e].length = n2 - n;
    }

    fn canonize(&mut self, node: Option<NodeId>, mut n: i32, n2: i32) -> (Option<NodeId>, i32) {
        let mut node = node;
        if n < n2 {
            let mut nd = node.expect("java.lang.NullPointerException: canonize");
            let mut n3 = self.op_len(n);
            if self.nodes[nd].misc == -1 {
                nd = self.m_root.unwrap();
                n += n3;
                if n >= n2 {
                    return (Some(nd), n);
                }
                n3 = self.op_len(n);
            }
            let mut e = self.find_edge_npe(nd, n3, n);
            while self.edges[e].length <= n2 - n {
                nd = self.son(e);
                n += self.edges[e].length;
                if n >= n2 {
                    continue;
                }
                let ol = self.op_len(n);
                e = self.find_edge_npe(nd, ol, n);
            }
            node = Some(nd);
        }
        (node, n)
    }

    fn split_edge(&mut self, node: NodeId, n: i32, n2: i32, n3: i32) -> NodeId {
        let ol = self.op_len(n);
        let e = self.find_edge_npe(node, ol, n);
        let son = self.son(e);
        let id = if self.nodes[son].id as i32 != n3 { NODE_GLOBAL } else { n3 };
        let misc = self.nodes[node].misc + n2 - n;
        let node2 = self.new_node(misc, id);
        let n4 = self.edges[e].label + n2 - n;
        let ol4 = self.op_len(n4);
        let len = self.edges[e].label + self.edges[e].length - n4;
        self.add_edge(node2, Some(son), ol4, n4, len);
        self.edges[e].length = n2 - n;
        self.edges[e].son = Some(node2);
        node2
    }

    fn separate_node(&mut self, node: NodeId, n: i32, n2: i32, n3: i32) -> (Option<NodeId>, i32) {
        let (node3o, n5) = self.canonize(Some(node), n, n2);
        if n5 < n2 {
            return (node3o, n5);
        }
        if self.nodes[node].misc == -1 {
            return (node3o, n5);
        }
        let node3 = node3o.unwrap();
        if self.nodes[node3].misc == self.nodes[node].misc + n2 - n {
            let mut node4 = Some(node3);
            while let Some(x) = node4 {
                if self.nodes[x].id as i32 != n3 {
                    self.nodes[x].id = i16::MAX;
                }
                node4 = self.nodes[x].suffix;
            }
            return (node3o, n5);
        }
        let id = if self.nodes[node3].id as i32 != n3 { NODE_GLOBAL } else { n3 };
        let misc = self.nodes[node].misc + n2 - n;
        let node5 = self.new_node(misc, id);
        self.copy_edge_table(node5, node3);
        self.nodes[node5].suffix = self.nodes[node3].suffix;
        self.nodes[node3].suffix = Some(node5);
        let mut node = Some(node);
        let mut n = n;
        loop {
            let nd = node.expect("java.lang.NullPointerException: separateNode");
            let ol = self.op_len(n);
            let e = self.find_edge_npe(nd, ol, n);
            let node3b = self.edges[e].son;
            self.init_edge(e, n, n2 - n, Some(node5));
            let mut n7 = n;
            loop {
                let n6 = self.op_len(n7);
                if n7 + n6 < n2 {
                    n7 += n6;
                } else {
                    break;
                }
            }
            let suf = self.nodes[nd].suffix;
            let (cn, ci) = self.canonize(suf, n, n7);
            node = cn;
            n = ci;
            let cur = node.expect("java.lang.NullPointerException: separateNode");
            let node6 = if n < n2 {
                let ol = self.op_len(n);
                let e = self.find_edge_npe(cur, ol, n);
                self.son(e)
            } else {
                cur
            };
            if self.nodes[node6].id as i32 != n3 {
                self.nodes[node6].id = i16::MAX;
            }
            let (node2, n4) = self.canonize(node, n, n2);
            if !(node3b == node2 && n5 == n4) {
                break;
            }
        }
        (Some(node5), n2)
    }

    fn add_font(&mut self, fi: usize, n: i32, bl: bool) {
        if self.m_fonts[fi].chars.n_strings == 0 {
            return;
        }
        let mut n3: i32 = 0;
        let mut n4: usize = 0;
        let n5 = self.m_fonts[fi].chars.offset[(self.m_fonts[fi].chars.n_strings - 1) as usize];
        self.m_char_str = self.m_fonts[fi].chars.data.clone();
        let cid = self.m_fonts[fi].subr_font_cid;
        let fd_index = if cid { self.m_fonts[fi].fd_index.clone() } else { Vec::new() };
        let mut n2: i32;
        if cid {
            n2 = (n + fd_index[n4] as i8 as i32) as i16 as i32;
            n4 += 1;
        } else {
            n2 = n;
        }
        if self.m_base.is_none() {
            let b = self.new_node(-1, n2);
            self.nodes[b].misc = -1;
            self.nodes[b].suffix = None;
            self.m_base = Some(b);
        }
        if self.m_root.is_none() {
            let r = self.new_node(0, n2);
            self.nodes[r].suffix = self.m_base;
            self.m_root = Some(r);
        }
        self.m_base_edge = self.new_edge();
        self.edges[self.m_base_edge].son = self.m_root;
        let mut node = self.m_root.unwrap();
        let mut n6 = n3;
        while n3 < n5 {
            let n7 = self.op_len(n3);
            let mut node3: Option<NodeId> = None;
            let mut node4: Option<NodeId> = None;
            let mut node5: Option<NodeId> = None;
            while !self.check_end_point(node, n6, n3, n7, n2) {
                if n6 < n3 {
                    let node6 = self.extension(node, n6, n3);
                    if node6 == node5 {
                        self.redirect(node, n6, n3, node3);
                        let suf = self.nodes[node].suffix;
                        let (a, b) = self.canonize(suf, n6, n3);
                        node = a.expect("java.lang.NullPointerException: addFont");
                        n6 = b;
                        continue;
                    }
                    node5 = node6;
                    node3 = Some(self.split_edge(node, n6, n3, n2));
                } else {
                    node3 = Some(node);
                }
                if n2 as usize >= self.m_sinks.len() {
                    self.m_sinks.resize(n2 as usize + 1, None);
                }
                let node2 = match self.m_sinks[n2 as usize] {
                    Some(s) => s,
                    None => {
                        let s = self.new_node(0, n2);
                        self.nodes[s].counted = true;
                        self.nodes[s].paths = 1;
                        self.m_sinks[n2 as usize] = Some(s);
                        s
                    }
                };
                self.add_edge(node3.unwrap(), Some(node2), n7, n3, n5 - n3);
                if let Some(n4n) = node4 {
                    self.nodes[n4n].suffix = node3;
                }
                node4 = node3;
                let suf = self.nodes[node].suffix;
                let (a, b) = self.canonize(suf, n6, n3);
                node = a.expect("java.lang.NullPointerException: addFont");
                n6 = b;
            }
            if let Some(n4n) = node4 {
                self.nodes[n4n].suffix = Some(node);
            }
            if bl {
                let mut node2 = node;
                let mut n8 = n6;
                let n9 = n3 + n7;
                while self.nodes[node2].misc != -1 {
                    let suf = self.nodes[node2].suffix;
                    let (a, b) = self.canonize(suf, n8, n9);
                    node2 = a.expect("java.lang.NullPointerException: addFont");
                    n8 = b;
                    if n8 < n9 || self.nodes[node2].id as i32 == n2 {
                        continue;
                    }
                    self.nodes[node2].id = i16::MAX;
                }
            }
            let (a, b) = self.separate_node(node, n6, n3 + n7, n2);
            node = a.expect("java.lang.NullPointerException: addFont");
            n6 = b;
            if cid && self.m_char_str[n3 as usize] == T2_SEPARATOR && n3 + n7 < n5 {
                n2 = n + fd_index[n4] as i8 as i32;
                n4 += 1;
            }
            n3 += n7;
        }
    }

    fn hash_subr(&self, n: i32, n2: i32) -> i32 {
        let mut n3 = n2;
        let n4 = if n2 >= 14 { n2 / 7 } else { 1 };
        let n5 = n2 - n4;
        let mut i = 0;
        while i < n5 {
            n3 = n3.wrapping_add(self.cu(n + i));
            n3 = n3.wrapping_add(n3.wrapping_shl(10));
            n3 ^= n3 >> 6;
            i += n4;
        }
        n3 = n3.wrapping_add(self.cu(n + n2 - 1));
        n3 = n3.wrapping_add(n3.wrapping_shl(10));
        n3 ^= n3 >> 6;
        n3 = n3.wrapping_add(n3.wrapping_shl(3));
        n3 ^= n3 >> 11;
        n3
    }

    fn lookup_subr_hash(&self, n: i32, n2: i32) -> i32 {
        let n5 = self.m_subr_hash.len();
        let n6 = n5 as i32 - 1;
        let mut n7: i32 = 0;
        let mut n8 = 0usize;
        let mut n3: i32 = if n5 <= SUBR_TABLE_SIZE_USE_SIMPLE_HASH {
            let n4 = self.cu(n);
            let n9 = self.cu(n + n2 / 2);
            let n10 = self.cu(n + n2 - 1);
            n4.wrapping_add(n9).wrapping_add(n10).wrapping_add(n2).wrapping_add(n2.wrapping_shl(5)).wrapping_add(n9.wrapping_shl(9)).wrapping_add(n10.wrapping_shl(14))
        } else {
            self.hash_subr(n, n2)
        };
        while n8 < n5 {
            let n4 = n3 & n6;
            let s = match self.m_subr_hash[n4 as usize] {
                None => return n4,
                Some(s) => s,
            };
            let sr = &self.subrs[s];
            if sr.length as i32 == n2 && (n == sr.cstr || self.bytecmp(n, sr.cstr, n2) == 0) {
                return n4;
            }
            n7 += 1;
            n3 = n3.wrapping_add(n7);
            n8 += 1;
        }
        -1
    }

    fn update_subr_quick_test_tables(&mut self) {
        self.m_max_subr_len = 0;
        self.m_min_subr_len = i32::MAX;
        for i in 0..self.m_subrs.len() {
            let s = self.m_subrs[i];
            if self.subrs[s].reject {
                continue;
            }
            let len = self.subrs[s].length as i32;
            let cstr = self.subrs[s].cstr;
            self.set_subr_prefix_map(cstr);
            if len > self.m_max_subr_len {
                self.m_max_subr_len = len;
            }
            if len >= self.m_min_subr_len {
                continue;
            }
            self.m_min_subr_len = len;
        }
        self.m_subr_len_map = vec![0; (self.m_max_subr_len + 1) as usize];
        for &s in &self.m_subrs {
            if self.subrs[s].reject {
                continue;
            }
            self.m_subr_len_map[self.subrs[s].length as usize] = 1;
        }
        let mut n2 = 0;
        for n in 0..self.m_subr_len_map.len() {
            if self.m_subr_len_map[n] != 0 {
                n2 = n;
            }
            self.m_subr_len_map[n] = n2 as i16;
        }
    }

    fn create_subr_hash(&mut self) {
        let n2 = self.m_subrs.len() * 2;
        let mut n = 1usize;
        while n2 > n {
            n <<= 1;
        }
        self.m_subr_hash = vec![None; n];
        for i in 0..self.m_subrs.len() {
            let s = self.m_subrs[i];
            let slot = self.lookup_subr_hash(self.subrs[s].cstr, self.subrs[s].length as i32);
            self.m_subr_hash[slot as usize] = Some(s);
        }
        self.update_subr_quick_test_tables();
    }

    fn count_paths(&mut self, e: Option<EdgeId>) -> i32 {
        let e = match e {
            None => return 0,
            Some(e) => e,
        };
        let node = self.son(e);
        if !self.nodes[node].counted {
            let mut n = self.nodes[node].paths as i32;
            n += self.count_paths_for_node(node);
            self.nodes[node].paths = if n > i16::MAX as i32 { i16::MAX } else { n as i16 };
            self.nodes[node].counted = true;
        }
        self.nodes[node].paths as i32
    }

    fn count_paths_for_node(&mut self, node: NodeId) -> i32 {
        let mut n = 0i32;
        let n2 = self.nodes[node].edge_table_size;
        for i in 0..n2 as usize {
            let e = match self.nodes[node].edge_table.as_ref().unwrap()[i] {
                Some(e) => e,
                None => continue,
            };
            if self.edges[e].length == 0 {
                continue;
            }
            n = n.wrapping_add(self.count_paths(Some(e)));
        }
        n
    }

    fn save_subr(&mut self, n: i32, node: NodeId, n2: i32, bl: bool, n3: i32) {
        let s = self.nodes[node].paths as i32;
        if n3 > i16::MAX as i32 {
            return;
        }
        let b = if bl { 1 } else { 0 };
        let ok = match n3 - n2 {
            1 | 2 => false,
            3 => s >= 7 - b,
            4 => s >= 4,
            5 | 6 => s >= 3,
            7 => s >= 3 - b,
            _ => s >= 2,
        };
        if !ok {
            return;
        }
        let id = self.new_subr();
        {
            let sr = &mut self.subrs[id];
            sr.node = node;
            sr.cstr = n - n3;
            sr.length = n3 as i16;
            sr.count = s as i16;
            sr.deltalen = 0;
            sr.numsize = 1;
            sr.maskcnt = n2 as i16;
        }
        self.m_subrs.push(id);
        self.nodes[node].misc = self.m_subrs.len() as i32 - 1;
        if bl {
            self.nodes[node].tail = true;
        }
    }

    fn find_cand_subrs(&mut self, e: EdgeId, mut n: i32, n2: i32) {
        let son = self.son(e);
        if n2 + self.edges[e].length != self.nodes[son].misc {
            return;
        }
        let node = son;
        if self.nodes[node].tested || self.nodes[node].paths == 1 {
            return;
        }
        self.nodes[node].tested = true;
        let label = self.edges[e].label;
        let n3 = label + self.edges[e].length;
        let mut n5 = label;
        while n5 < n3 {
            let n4 = self.op_len(n5);
            let n6 = self.cu(n5);
            if n6 == TX_ENDCHAR {
                if self.nodes[node].paths > 1 {
                    n5 += n4;
                    let len = self.nodes[node].misc - (label + self.edges[e].length - n5);
                    self.save_subr(n5, node, n, true, len);
                }
                return;
            }
            if n6 == T2_HINTMASK || n6 == T2_CNTRMASK {
                n += 1;
            }
            n5 += n4;
        }
        let n2b = self.nodes[node].misc;
        if self.nodes[node].edge_count > 1 {
            self.save_subr(n3, node, n, false, n2b);
            self.walk_edge_table(node, n, n2b);
        } else {
            // Java loops back to the same, now tested, node and returns.
            let e0 = self.nodes[node].edge_table.as_ref().unwrap()[0].expect("java.lang.NullPointerException: edgeTable[0]");
            let s0 = self.son(e0);
            if self.nodes[node].paths > self.nodes[s0].paths {
                self.save_subr(n3, node, n, false, n2b);
            }
        }
    }

    fn subr_saved(&self, s: SubrId) -> i32 {
        let sr = &self.subrs[s];
        let n = sr.length as i32 - sr.maskcnt as i32;
        sr.count as i32 * (n - 1 - sr.numsize as i32) - (self.m_off_size + n + if self.nodes[sr.node].tail { 0 } else { 1 })
    }

    fn subr_saved_by_one_call(&self, s: SubrId) -> i32 {
        let sr = &self.subrs[s];
        let n = sr.length as i32 - sr.maskcnt as i32;
        n - 1 - sr.numsize as i32
    }

    fn build_call_list(&mut self, n: i32, n2: i32, n3: i32, bl: bool, n4: i32) {
        let n6 = n3 + n2;
        let mut lists: Vec<Vec<Call>> = Vec::new();
        self.m_prefix_len = vec![0; n2.max(0) as usize];
        let mut n7 = n3;
        let mut n8 = 0;
        while n8 < n2 {
            let n9 = self.op_len(n7);
            for n5 in 0..n9 {
                self.m_prefix_len[n8 as usize] = n5 as i16;
                n8 += 1;
            }
            n7 += n9;
        }
        let mut n10 = n6 - n3;
        if n10 > self.m_max_subr_len {
            n10 = self.m_max_subr_len;
        }
        while n10 >= self.m_min_subr_len {
            n10 = self.m_subr_len_map[n10 as usize] as i32;
            let mut n12 = n3;
            while n12 + n10 <= n6 {
                let n11 = n12 + n10;
                if self.test_subr_prefix_map(n12) && (n11 >= n6 || self.m_prefix_len[(n11 - n3) as usize] == 0) {
                    let slot = self.lookup_subr_hash(n12, n11 - n12);
                    let subr = if slot >= 0 { self.m_subr_hash[slot as usize] } else { None };
                    if let Some(s) = subr {
                        let sr = &self.subrs[s];
                        let sid = self.nodes[sr.node].id as i32;
                        let skip = n != 0 && !sr.select || n11 == n6 && n12 == n3 && !bl || n != 0 && n4 != sid && sid != NODE_GLOBAL;
                        if !skip {
                            let mut n5 = 0;
                            let n13 = n12 - n3;
                            let n14 = n13 + sr.length as i32;
                            for list in lists.iter_mut() {
                                let mut bl2 = false;
                                let n16 = list.len();
                                let mut n15 = 0;
                                while n15 < n16 {
                                    let call = list[n15];
                                    let clen = self.subrs[call.subr].length as i32;
                                    if n14 <= call.offset {
                                        break;
                                    }
                                    if n14 > call.offset && n13 < call.offset + clen {
                                        bl2 = true;
                                        break;
                                    }
                                    if n13 >= call.offset + clen || n14 <= call.offset {
                                        n15 += 1;
                                        continue;
                                    }
                                    bl2 = true;
                                    break;
                                }
                                if bl2 {
                                    continue;
                                }
                                list.insert(n15, Call { subr: s, offset: n13 });
                                n5 = 1;
                                break;
                            }
                            if n5 == 0 && lists.len() < MAX_CALL_LIST_COUNT {
                                lists.push(vec![Call { subr: s, offset: n13 }]);
                            }
                        }
                    }
                }
                n12 += self.op_len(n12);
            }
            n10 -= 1;
        }
        let mut n17 = 0;
        let mut n18 = 0;
        for (i, list) in lists.iter().enumerate() {
            let mut n19 = 0;
            for c in list {
                n19 += self.subr_saved_by_one_call(c.subr);
            }
            if n19 <= n17 {
                continue;
            }
            n17 = n19;
            n18 = i;
        }
        self.m_calls = if lists.is_empty() { Vec::new() } else { lists.swap_remove(n18) };
        if n == 0 {
            for i in 0..self.m_calls.len() {
                let s = self.m_calls[i].subr;
                self.subrs[s].count = self.subrs[s].count.wrapping_add(1);
            }
        }
    }

    fn set_subr_act_count(&mut self) {
        for i in 0..self.m_subrs.len() {
            let s = self.m_subrs[i];
            self.subrs[s].count = 0;
        }
        for i in 0..self.m_subrs.len() {
            let s = self.m_subrs[i];
            let (len, cstr) = (self.subrs[s].length as i32, self.subrs[s].cstr);
            self.build_call_list(0, len, cstr, false, 0);
            let mut link: Option<LinkId> = None;
            for j in (0..self.m_calls.len()).rev() {
                let call = self.m_calls[j];
                let s2 = call.subr;
                let sups = self.subrs[s2].sups;
                let l = self.new_link(s, call.offset, sups);
                self.subrs[s2].sups = Some(l);
                link = Some(self.new_link(s2, call.offset, link));
            }
            self.subrs[s].infs = link;
        }
    }

    fn sort_inf_subrs(&mut self) {
        for i in 0..self.m_subrs.len() {
            let s = self.m_subrs[i];
            self.subrs[s].misc = self.subr_saved(s) as i16;
        }
        for i in 0..self.m_subrs.len() {
            let s = self.m_subrs[i];
            let mut link2 = self.subrs[s].infs;
            while let Some(l2) = link2 {
                let mut link = match self.links[l2].next {
                    None => break,
                    Some(l) => l,
                };
                let mut link3: Option<LinkId> = None;
                let mut sv = self.subrs[self.links[l2].subr].misc;
                loop {
                    let s2 = self.subrs[self.links[link].subr].misc;
                    if s2 > sv {
                        link3 = Some(link);
                        sv = s2;
                    }
                    match self.links[link].next {
                        None => break,
                        Some(l) => link = l,
                    }
                }
                if let Some(l3) = link3 {
                    let (cs, co) = (self.links[l2].subr, self.links[l2].offset);
                    self.links[l2].subr = self.links[l3].subr;
                    self.links[l2].offset = self.links[l3].offset;
                    self.links[l3].subr = cs;
                    self.links[l3].offset = co;
                }
                link2 = self.links[l2].next;
            }
        }
    }

    fn select_cand_subrs(&mut self) {
        let root = self.m_root.unwrap();
        self.count_paths_for_node(root);
        self.m_subrs = Vec::new();
        self.walk_edge_table(root, 0, 0);
        self.create_subr_hash();
    }

    fn assoc_subrs(&mut self) {
        for i in 0..self.m_num_fonts as usize {
            let mut n = 0;
            for j in 0..self.m_fonts[i].chars.n_strings as usize {
                let n2 = self.m_fonts[i].chars.offset[j];
                self.build_call_list(0, n2 - n, n, true, 0);
                n = n2;
            }
        }
    }

    fn add_member(&mut self, subr: SubrId) {
        let mut n = 0usize;
        let mut arr: Vec<SubrId> = vec![0; LISTSIZE];
        let mut list: Vec<SubrId> = vec![subr];
        while let Some(s) = list.pop() {
            if self.subrs[s].member {
                continue;
            }
            self.m_members.push(s);
            self.subrs[s].member = true;
            let mut link = self.subrs[s].sups;
            while let Some(l) = link {
                arr[n] = self.links[l].subr;
                n += 1;
                if n >= LISTSIZE {
                    return;
                }
                link = self.links[l].next;
            }
            link = self.subrs[s].infs;
            while let Some(l) = link {
                arr[n] = self.links[l].subr;
                n += 1;
                if n >= LISTSIZE {
                    return;
                }
                link = self.links[l].next;
            }
            for i in (0..n).rev() {
                list.push(arr[i]);
            }
            n = 0;
        }
    }

    fn tie_breaker(&self, a: SubrId, b: SubrId) -> i32 {
        let (s, s2) = (&self.subrs[a], &self.subrs[b]);
        if s.cstr > s2.cstr {
            return -1;
        }
        if s.cstr < s2.cstr {
            return 1;
        }
        if s.length > s2.length {
            return -1;
        }
        if s.length < s2.length {
            return 1;
        }
        0
    }

    fn node_id(&self, s: SubrId) -> i32 {
        self.nodes[self.subrs[s].node].id as i32
    }

    fn cmp_global_set_subrs(&self, a: SubrId, b: SubrId) -> i32 {
        if self.node_id(a) == NODE_GLOBAL {
            if self.node_id(b) == NODE_GLOBAL {
                let n2 = self.subr_saved(a);
                let n = self.subr_saved(b);
                if n2 > n {
                    return -1;
                }
                if n2 < n {
                    return 1;
                }
                return self.tie_breaker(a, b);
            }
            return -1;
        }
        if self.node_id(b) == NODE_GLOBAL {
            return 1;
        }
        self.tie_breaker(a, b)
    }

    fn cmp_local_set_subrs(&self, a: SubrId, b: SubrId) -> i32 {
        let (s, s2) = (&self.subrs[a], &self.subrs[b]);
        let mut n = 0;
        if s.reject {
            n |= 8;
        }
        if s.select {
            n |= 4;
        }
        if s2.reject {
            n |= 2;
        }
        if s2.select {
            n |= 1;
        }
        match n {
            0 | 5 => {
                let n2 = self.subr_saved(a);
                let n3 = self.subr_saved(b);
                if n2 > n3 {
                    return -1;
                }
                if n2 < n3 {
                    return 1;
                }
                self.tie_breaker(a, b)
            }
            1 | 8 | 9 => 1,
            2 | 4 | 6 => -1,
            10 => self.tie_breaker(a, b),
            _ => 0,
        }
    }

    fn find_groups(&mut self, n: i32) {
        self.m_leaders = Vec::new();
        self.m_subr_sort_mode = if n == NODE_GLOBAL { 1 } else { 0 };
        for i in 0..self.m_tmp.len() {
            let s = self.m_tmp[i];
            if self.subrs[s].member {
                continue;
            }
            self.m_members = Vec::new();
            self.add_member(s);
            let mut m = std::mem::take(&mut self.m_members);
            self.sort_subrs(&mut m);
            for j in 0..m.len().saturating_sub(1) {
                self.subrs[m[j]].next = Some(m[j + 1]);
            }
            let last = *m.last().unwrap();
            self.subrs[last].next = None;
            self.m_leaders.push(m[0]);
            self.m_members = m;
        }
    }

    /// `Collections.sort` with `Subr.compareTo` under the current sort mode.
    fn sort_subrs(&self, v: &mut Vec<SubrId>) {
        let cmp = |a: &SubrId, b: &SubrId| {
            let c = match self.m_subr_sort_mode {
                0 => self.cmp_local_set_subrs(*a, *b),
                1 => self.cmp_global_set_subrs(*a, *b),
                _ => self.cmp_subr_fitness(*a, *b),
            };
            c.cmp(&0)
        };
        stable_sort(v, &cmp);
    }

    fn update_sups(&mut self, subr: SubrId, n: i32, n2: i32) {
        let mut link = self.subrs[subr].sups;
        while let Some(l) = link {
            let s2 = self.links[l].subr;
            if !self.subrs[s2].select && !self.subrs[s2].reject && self.node_id(s2) == n2 {
                self.update_sups(s2, n, n2);
                self.subrs[s2].deltalen = (self.subrs[s2].deltalen as i32 + n) as i16;
            }
            link = self.links[l].next;
        }
    }

    fn select_subr(&mut self, s: SubrId) {
        let sr = &self.subrs[s];
        let cnt = sr.count as i32;
        let n = sr.length as i32 - sr.maskcnt as i32 + sr.deltalen as i32;
        let n2 = cnt * (n - 1 - sr.numsize as i32) - (self.m_off_size + n + if self.nodes[sr.node].tail { 0 } else { 1 });
        if n2 > 0 {
            let s2 = self.node_id(s);
            let n3 = sr.numsize as i32 + 1 - sr.length as i32 - sr.deltalen as i32;
            self.subrs[s].select = true;
            self.update_sups(s, n3, s2);
        } else {
            self.subrs[s].reject = true;
        }
    }

    fn select_global_subrs(&mut self, mut subr: Option<SubrId>, _n: i32) {
        while let Some(s) = subr {
            if self.node_id(s) != NODE_GLOBAL {
                return;
            }
            if !self.subrs[s].select && !self.subrs[s].reject {
                self.select_subr(s);
            }
            subr = self.subrs[s].next;
        }
    }

    fn select_local_subrs(&mut self, mut subr: Option<SubrId>, n: i32) {
        while let Some(s) = subr {
            if self.node_id(s) == NODE_GLOBAL {
                if !self.subrs[s].select {
                    return;
                }
                let v = self.subrs[s].numsize as i32 + 1 - self.subrs[s].length as i32;
                self.update_sups(s, v, n);
            } else if !self.subrs[s].select && !self.subrs[s].reject {
                self.select_subr(s);
            }
            subr = self.subrs[s].next;
        }
    }

    fn cmp_subr_fitness(&self, a: SubrId, b: SubrId) -> i32 {
        let (s, s2) = (&self.subrs[a], &self.subrs[b]);
        let bl = s.select;
        let bl2 = s2.select;
        if bl == bl2 {
            let n2 = self.subr_saved(a);
            let n = self.subr_saved(b);
            if n2 > n {
                return -1;
            }
            if n2 < n {
                return 1;
            }
            if s.length > s2.length {
                return -1;
            }
            if s.length < s2.length {
                return 1;
            }
            if s.count > s2.count {
                return -1;
            }
            if s.count < s2.count {
                return 1;
            }
            return self.tie_breaker(a, b);
        }
        if !bl && bl2 {
            return 1;
        }
        -1
    }

    fn check_subr_stack_ovl(&mut self, s: SubrId, mut n: i32) {
        if n <= self.subrs[s].misc as i32 {
            return;
        }
        self.subrs[s].misc = n as i16;
        n += 1;
        if self.subrs[s].select && n >= TX_MAX_CALL_STACK {
            self.subrs[s].select = false;
            self.subrs[s].reject = true;
            n -= 1;
            self.m_subr_stack_ovl = true;
        }
        let mut link = self.subrs[s].sups;
        while let Some(l) = link {
            let ls = self.links[l].subr;
            self.check_subr_stack_ovl(ls, n);
            link = self.links[l].next;
        }
    }

    fn select_final_subr_set(&mut self, var1: i32) {
        let mut var3: usize = 0;
        let var4: usize = if self.m_singleton { 2 } else { 1 };
        let mut var5 = 0;
        self.find_groups(var1);
        loop {
            for i in 0..self.m_leaders.len() {
                let s = self.m_leaders[i];
                if self.subrs[s].next.is_none() {
                    if self.subr_saved(s) > 0 {
                        self.subrs[s].select = true;
                    } else {
                        self.subrs[s].reject = true;
                    }
                    continue;
                }
                if var1 == NODE_GLOBAL {
                    self.select_global_subrs(Some(s), var1);
                } else {
                    self.select_local_subrs(Some(s), var1);
                }
            }
            for i in 0..self.m_tmp.len() {
                let s = self.m_tmp[i];
                self.subrs[s].misc = -1;
            }
            for i in 0..self.m_leaders.len() {
                let mut sub = Some(self.m_leaders[i]);
                while let Some(s) = sub {
                    if self.subrs[s].infs.is_none() {
                        self.check_subr_stack_ovl(s, 0);
                    }
                    self.subrs[s].member = false;
                    sub = self.subrs[s].next;
                }
            }
            self.m_subr_sort_mode = 2;
            let mut t = std::mem::take(&mut self.m_tmp);
            self.sort_subrs(&mut t);
            self.m_tmp = t;
            for i in (0..self.m_tmp.len()).rev() {
                if !self.subrs[self.m_tmp[i]].select {
                    continue;
                }
                var3 = i + 1;
                break;
            }
            var5 += 1;
            if var5 == 2 {
                if self.m_singleton && (var3 == 2479 || var3 == 67799) {
                    let s = self.new_subr();
                    let nd = self.new_node(0, 0);
                    self.subrs[s].node = nd;
                    self.m_tmp.insert(var3, s);
                    var3 += 1;
                }
                let mut var6 = self.m_max_num_subrs as usize;
                if var6 == 0 {
                    var6 = 32767;
                }
                var6 *= var4;
                if var3 > var6 {
                    while var6 < self.m_tmp.len() {
                        self.m_tmp.pop();
                    }
                } else {
                    while var3 < self.m_tmp.len() {
                        self.m_tmp.pop();
                    }
                }
                return;
            }
            if var3 < 215 * var4 {
                while var3 < self.m_tmp.len() {
                    self.m_tmp.pop();
                }
                return;
            }
            for i in 0..self.m_tmp.len() {
                let s = self.m_tmp[i];
                self.subrs[s].select = false;
                self.subrs[s].reject = false;
            }
            let mut i = self.m_tmp.len() as i64 - 1;
            while i >= (2263 * var4) as i64 {
                let s = self.m_tmp[i as usize];
                self.subrs[s].numsize = 3;
                i -= 1;
            }
            while i >= (215 * var4) as i64 {
                let s = self.m_tmp[i as usize];
                self.subrs[s].numsize = 2;
                i -= 1;
            }
            while i >= 0 {
                let s = self.m_tmp[i as usize];
                self.subrs[s].numsize = 1;
                i -= 1;
            }
        }
    }

    fn stack_overflow(&self) {}

    fn subrize_cstr(&self, out: &mut [u8], mut n: i32, mut n2: i32, n3: i32) -> i32 {
        let mut n4 = 0;
        for call in &self.m_calls {
            let sr = &self.subrs[call.subr];
            n = self.t2cstrcpy(out, n, n2, call.offset - n4);
            n += Self::cfw_enc_int(sr.subrnum as i64, out, n);
            out[n as usize] = if self.nodes[sr.node].id as i32 == NODE_GLOBAL { T2_CALLGSUBR } else { TX_CALLSUBR };
            n += 1;
            n2 += call.offset - n4 + sr.length as i32;
            n4 = call.offset + sr.length as i32;
        }
        self.t2cstrcpy(out, n, n2, n3 - n4)
    }

    /// `subrizeChars(font.chars, n)` for font `fi`.
    fn subrize_chars(&mut self, fi: usize, n: i32) {
        let mut n2 = 0;
        let mut n3 = 0;
        let ns = self.m_fonts[fi].chars.n_strings;
        let mut out = vec![0u8; self.m_fonts[fi].chars.data.len()];
        for i in 0..ns as usize {
            let n4 = self.m_fonts[fi].chars.offset[i];
            let n5 = n4 - n2 - 4;
            self.build_call_list(1, n5, n2, true, n);
            n3 = self.subrize_cstr(&mut out, n3, n2, n5);
            self.m_fonts[fi].chars.offset[i] = n3;
            n2 = n4;
        }
        out.truncate(n3 as usize);
        self.m_fonts[fi].chars.data = out;
    }

    fn reorder_combined(&mut self, bl: bool) {
        let size = self.m_tmp.len() as i32;
        let (n4, mut n3) = if bl { (size / 2, (size & -2) + 1) } else { ((size + 1) / 2, (size + 1) & -2) };
        let mut reorder = vec![0usize; n4 as usize];
        let mut n2 = n4 - 1;
        let n;
        let take = |n3: &mut i32, tmp: &Vec<SubrId>| {
            *n3 -= 2;
            tmp[*n3 as usize]
        };
        if n4 < 1240 {
            while n2 >= 0 {
                reorder[n2 as usize] = take(&mut n3, &self.m_tmp);
                n2 -= 1;
            }
            n = 107;
        } else if n4 < 33900 {
            while n2 >= 1239 {
                reorder[n2 as usize] = take(&mut n3, &self.m_tmp);
                n2 -= 1;
            }
            while n2 >= 215 {
                reorder[(n2 - 215) as usize] = take(&mut n3, &self.m_tmp);
                n2 -= 1;
            }
            while n2 >= 0 {
                reorder[(n2 + 1024) as usize] = take(&mut n3, &self.m_tmp);
                n2 -= 1;
            }
            n = 1131;
        } else {
            while n2 >= 33900 {
                reorder[n2 as usize] = take(&mut n3, &self.m_tmp);
                n2 -= 1;
            }
            while n2 >= 2263 {
                reorder[(n2 - 2263) as usize] = take(&mut n3, &self.m_tmp);
                n2 -= 1;
            }
            while n2 >= 1239 {
                reorder[(n2 + 31637) as usize] = take(&mut n3, &self.m_tmp);
                n2 -= 1;
            }
            while n2 >= 215 {
                reorder[(n2 + 31422) as usize] = take(&mut n3, &self.m_tmp);
                n2 -= 1;
            }
            while n2 >= 0 {
                reorder[(n2 + 32661) as usize] = take(&mut n3, &self.m_tmp);
                n2 -= 1;
            }
            n = 32768;
        }
        if !bl {
            for (i, &s) in reorder.iter().enumerate() {
                self.subrs[s].subrnum = (i as i32 - n) as i16;
                let nd = self.subrs[s].node;
                self.nodes[nd].id = i16::MAX;
            }
            let mut i = 0;
            while i + 1 < self.m_tmp.len() {
                let v = self.subrs[self.m_tmp[i]].subrnum;
                let t = self.m_tmp[i + 1];
                self.subrs[t].subrnum = v;
                i += 2;
            }
        }
        self.m_reorder = Some(reorder);
    }

    fn reorder_subrs(&mut self) {
        let size = self.m_tmp.len() as i32;
        let mut reorder = vec![0usize; size as usize];
        let mut n2 = size - 1;
        let n;
        let tmp = &self.m_tmp;
        if size < 1240 {
            while n2 >= 0 {
                reorder[n2 as usize] = tmp[n2 as usize];
                n2 -= 1;
            }
            n = 107;
        } else if size < 33900 {
            while n2 >= 1239 {
                reorder[n2 as usize] = tmp[n2 as usize];
                n2 -= 1;
            }
            while n2 >= 215 {
                reorder[(n2 - 215) as usize] = tmp[n2 as usize];
                n2 -= 1;
            }
            while n2 >= 0 {
                reorder[(n2 + 1024) as usize] = tmp[n2 as usize];
                n2 -= 1;
            }
            n = 1131;
        } else {
            while n2 >= 33900 {
                reorder[n2 as usize] = tmp[n2 as usize];
                n2 -= 1;
            }
            while n2 >= 2263 {
                reorder[(n2 - 2263) as usize] = tmp[n2 as usize];
                n2 -= 1;
            }
            while n2 >= 1239 {
                reorder[(n2 + 31637) as usize] = tmp[n2 as usize];
                n2 -= 1;
            }
            while n2 >= 215 {
                reorder[(n2 + 31422) as usize] = tmp[n2 as usize];
                n2 -= 1;
            }
            while n2 >= 0 {
                reorder[(n2 + 32661) as usize] = tmp[n2 as usize];
                n2 -= 1;
            }
            n = 32768;
        }
        for (i, &s) in reorder.iter().enumerate() {
            self.subrs[s].subrnum = (i as i32 - n) as i16;
        }
        self.m_reorder = Some(reorder);
    }

    /// `addSubrs(target, n)`; the target is returned instead of filled in place.
    fn add_subrs(&mut self, n: i32) -> Option<SubrCSData> {
        let reorder = match &self.m_reorder {
            Some(r) if !r.is_empty() => r.clone(),
            _ => return None,
        };
        let mut sd = SubrCSData { n_strings: reorder.len() as i16 as i32, offset: vec![0; reorder.len()], data: Vec::new() };
        let mut n3 = 0usize;
        for &s in &reorder {
            n3 += self.subrs[s].length as usize + 1;
        }
        let mut n2 = 0;
        let mut out = vec![0u8; n3];
        for (i, &s) in reorder.iter().enumerate() {
            let (len, cstr) = (self.subrs[s].length as i32, self.subrs[s].cstr);
            self.build_call_list(1, len, cstr, false, n);
            n2 = self.subrize_cstr(&mut out, n2, cstr, len);
            if !self.nodes[self.subrs[s].node].tail {
                out[n2 as usize] = TX_RETURN;
                n2 += 1;
            }
            sd.offset[i] = n2;
        }
        out.truncate(n2 as usize);
        sd.data = out;
        Some(sd)
    }

    fn subrize_fd_chars(&mut self, fi: usize, n: i32, n2: i32) -> SubrCSData {
        let mut n4 = 0;
        self.m_char_str = self.m_fonts[fi].chars.data.clone();
        let mut out = vec![0u8; self.m_fonts[fi].chars.data.len()];
        let mut sd = SubrCSData::default();
        let ns = self.m_fonts[fi].chars.n_strings as usize;
        for i in 0..ns {
            if self.m_fonts[fi].fd_index[i] as i8 as i32 == n2 {
                sd.n_strings += 1;
            }
        }
        sd.offset = vec![0; sd.n_strings as usize];
        let mut n3 = 0;
        for i in 0..ns {
            if self.m_fonts[fi].fd_index[i] as i8 as i32 != n2 {
                continue;
            }
            let n5 = if i == 0 { 0 } else { self.m_fonts[fi].chars.offset[i - 1] };
            let n6 = self.m_fonts[fi].chars.offset[i] - n5 - 4;
            self.build_call_list(1, n6, n5, true, n + n2);
            n4 = self.subrize_cstr(&mut out, n4, n5, n6);
            sd.offset[n3] = n4;
            n3 += 1;
        }
        out.truncate(n4 as usize);
        sd.data = out;
        sd
    }

    fn join_fd_chars(&mut self, fi: usize) {
        let f = &mut self.m_fonts[fi];
        let fd_info = f.fd_info.as_mut().unwrap();
        let mut n3 = 0usize;
        for n2 in 0..f.fd_count as usize {
            let info = &mut fd_info[n2];
            if info.chars.n_strings != 0 {
                n3 += info.chars.offset[(info.chars.n_strings - 1) as usize] as usize;
            }
            info.i_char = 0;
        }
        f.chars.data = vec![0; n3];
        let mut n2 = 0usize;
        for n in 0..f.chars.n_strings as usize {
            let info = &mut fd_info[f.fd_index[n] as i8 as usize];
            let n4 = if info.i_char == 0 { 0 } else { info.chars.offset[(info.i_char - 1) as usize] as usize };
            let n5 = info.chars.offset[info.i_char as usize] as usize - n4;
            info.i_char += 1;
            f.chars.data[n2..n2 + n5].copy_from_slice(&info.chars.data[n4..n4 + n5]);
            n2 += n5;
            f.chars.offset[n] = n2 as i32;
        }
        for info in fd_info.iter_mut().take(f.fd_count as usize) {
            info.chars.offset = Vec::new();
            info.chars.data = Vec::new();
        }
    }

    fn build_subrs(&mut self, n: i32) -> Option<SubrCSData> {
        self.m_tmp = Vec::new();
        for i in 0..self.m_subrs.len() {
            let s = self.m_subrs[i];
            if self.node_id(s) != n {
                continue;
            }
            self.m_tmp.push(s);
        }
        self.select_final_subr_set(n);
        self.update_subr_quick_test_tables();
        self.reorder_subrs();
        self.add_subrs(n)
    }

    fn cfw_subr_subrize(&mut self, n: i32, fonts: Vec<SubrFont>) {
        self.m_num_fonts = n;
        self.m_fonts = fonts;
        let mut b = [0u8; 2];
        for i in 0..256 {
            b[0] = i as u8;
            self.m_op_len_cache[i] = CFFSubrize::t2oplen(&b) as u8;
        }
        self.m_singleton = self.m_num_fonts == 1 && !self.m_fonts[0].subr_font_cid;
        let mut n3 = 0;
        self.m_sinks = Vec::new();
        for i in 0..self.m_num_fonts as usize {
            let bl = self.m_num_fonts > 1 || self.m_fonts[i].subr_font_cid;
            self.add_font(i, n3, bl);
            n3 += if self.m_fonts[i].subr_font_cid { self.m_fonts[i].fd_count } else { 1 };
        }
        self.select_cand_subrs();
        self.set_subr_act_count();
        self.assoc_subrs();
        self.sort_inf_subrs();
        if self.m_singleton {
            self.m_tmp = self.m_subrs.clone();
            self.m_subr_stack_ovl = false;
            self.select_final_subr_set(0);
            self.update_subr_quick_test_tables();
            if self.m_subr_stack_ovl {
                self.stack_overflow();
            }
            if self.m_tmp.len() >= 215 {
                self.reorder_combined(false);
                self.m_gsubrs = Some(self.add_subrs(NODE_GLOBAL).unwrap_or_default());
                self.reorder_combined(true);
                if let Some(sd) = self.add_subrs(0) {
                    self.m_fonts[0].subrs = sd;
                }
            } else {
                self.reorder_subrs();
                if let Some(sd) = self.add_subrs(0) {
                    self.m_fonts[0].subrs = sd;
                }
            }
            self.subrize_chars(0, 0);
        } else {
            self.m_gsubrs = Some(self.build_subrs(NODE_GLOBAL).unwrap_or_default());
            n3 = 0;
            for fi in 0..self.m_num_fonts as usize {
                self.m_subr_stack_ovl = false;
                if self.m_fonts[fi].subr_font_cid {
                    for i in 0..self.m_fonts[fi].fd_count {
                        if let Some(sd) = self.build_subrs(n3 + i) {
                            self.m_fonts[fi].fd_info.as_mut().unwrap()[i as usize].subrs = sd;
                        }
                        let ch = self.subrize_fd_chars(fi, n3, i);
                        self.m_fonts[fi].fd_info.as_mut().unwrap()[i as usize].chars = ch;
                    }
                    self.join_fd_chars(fi);
                    n3 += self.m_fonts[fi].fd_count;
                } else {
                    if self.m_fonts[fi].chars.n_strings != 0 {
                        if let Some(sd) = self.build_subrs(n3) {
                            self.m_fonts[fi].subrs = sd;
                        }
                        self.subrize_chars(fi, n3);
                    }
                    n3 += 1;
                }
                if self.m_subr_stack_ovl {
                    self.stack_overflow();
                }
            }
        }
    }

    fn op_len(&self, n: i32) -> i32 {
        let mut n2 = self.m_op_len_cache[self.cu(n) as usize] as i32;
        if n2 == 0 {
            n2 = self.cu(n + 1);
        }
        n2
    }

    fn set_subr_prefix_map(&mut self, n: i32) {
        let n2 = self.cu(n);
        let n3 = self.cu(n + 1);
        let n4 = (n2 << 5 | n3 >> 3) as usize;
        let n5 = 1 << (n3 & 7);
        self.m_subr_prefix_map[n4] |= n5 as u8;
    }

    fn test_subr_prefix_map(&self, n: i32) -> bool {
        let n2 = self.cu(n);
        let n3 = self.cu(n + 1);
        let n4 = (n2 << 5 | n3 >> 3) as usize;
        let n5 = 1 << (n3 & 7);
        self.m_subr_prefix_map[n4] as i32 & n5 != 0
    }

    fn bytecmp(&self, mut n: i32, mut n2: i32, mut n3: i32) -> i32 {
        while n3 != 0 {
            n3 -= 1;
            let (a, b) = (self.m_char_str[n as usize], self.m_char_str[n2 as usize]);
            n += 1;
            n2 += 1;
            if a == b {
                continue;
            }
            return a as i32 - b as i32;
        }
        0
    }

    fn cfw_enc_int(l: i64, out: &mut [u8], n: i32) -> i32 {
        let n = n as usize;
        if (-107..=107).contains(&l) {
            out[n] = (l + 139) as u8;
            return 1;
        }
        if (108..=1131).contains(&l) {
            let l = l - 108;
            out[n] = ((l >> 8) + 247) as u8;
            out[n + 1] = l as u8;
            return 2;
        }
        if (-1131..=-108).contains(&l) {
            let l = l + 108;
            out[n] = ((-l >> 8) + 251) as u8;
            out[n + 1] = (-l) as u8;
            return 2;
        }
        if (-32768..=32767).contains(&l) {
            out[n] = 28;
            out[n + 1] = (l >> 8) as u8;
            out[n + 2] = l as u8;
            return 3;
        }
        out[n] = 29;
        out[n + 1] = (l >> 24) as u8;
        out[n + 2] = (l >> 16) as u8;
        out[n + 3] = (l >> 8) as u8;
        out[n + 4] = l as u8;
        5
    }
}

#[cfg(test)]
impl CFFSubrize {
    /// The harness's dump of each subr's `infs` then `sups` as (index in mSubrs, offset), lists ended by -1.
    pub fn debug_link_table(&self) -> Vec<u8> {
        let ix: std::collections::HashMap<usize, i32> = self.m_subrs.iter().enumerate().map(|(i, &s)| (s, i as i32)).collect();
        let mut o = Vec::new();
        for &s in &self.m_subrs {
            for mut l in [self.subrs[s].infs, self.subrs[s].sups] {
                while let Some(k) = l {
                    o.extend_from_slice(&ix.get(&self.links[k].subr).copied().unwrap_or(-2).to_be_bytes());
                    o.extend_from_slice(&self.links[k].offset.to_be_bytes());
                    l = self.links[k].next;
                }
                o.extend_from_slice(&(-1i32).to_be_bytes());
            }
        }
        o
    }

    /// The harness's dump of `mSubrs`: cstr, length, count, deltalen, subrnum, numsize, maskcnt, misc, flags, node id.
    pub fn debug_subr_table(&self) -> Vec<u8> {
        let mut o = Vec::new();
        for &s in &self.m_subrs {
            let r = &self.subrs[s];
            o.extend_from_slice(&r.cstr.to_be_bytes());
            for v in [r.length, r.count, r.deltalen, r.subrnum, r.numsize, r.maskcnt, r.misc] {
                o.extend_from_slice(&v.to_be_bytes());
            }
            o.push(r.select as u8 | (r.reject as u8) << 1 | (r.member as u8) << 2);
            o.extend_from_slice(&self.nodes[r.node].id.to_be_bytes());
        }
        o
    }
}
