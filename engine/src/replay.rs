//! The table behind v2's replay (attempt 8 in specs/vcb-engine-v2-log.md).
//!
//! A **node** is the circuit's full state at the end of a tick with an event (a VMem load, a VMem write, a
//! Breakpoint): everything the future depends on. From a node the circuit is deterministic up to the next
//! event, so the number of ticks to it and what it is are stored once, as the node's **edge**. If that event
//! is a load, the state after it also depends on the word loaded: the edge then has one successor per word seen.
//! A node is only ever reused after its whole recorded state has been compared; the hash only finds candidates.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

pub const NONE: u32 = u32::MAX;

/// What a node holds besides the signals and the two lists.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub struct Meta {
    /// The address the address pins hold.
    pub address: u32,
    pub locked: bool,
    pub load_pending: bool,
    /// The tick modulo the clock interval, and modulo the timer interval.
    pub clock_phase: u32,
    pub timer_phase: u32,
}

#[derive(Clone)]
struct Node {
    hash: u64,
    /// The next node with the same hash.
    next: u32,
    lists_at: u32,
    dirty_len: u32,
    rose_len: u32,
    meta: Meta,
    edge: u32,
}

/// From a node to the next event.
#[derive(Clone, Copy, PartialEq)]
pub struct Edge {
    pub ticks: u64,
    /// The address loaded from at the event, or NONE.
    pub fetch: u32,
    /// The address written to at the event, or NONE.
    pub write: u32,
    pub breakpoint: bool,
}

/// Where a step ends: the node after the event and the value written at it (if the event writes).
#[derive(Clone, Copy, PartialEq)]
pub struct Arrival {
    pub node: u32,
    pub written: u32,
}

/// As two words, for `Mix` (the derived one hashes five fields a byte at a time there).
impl std::hash::Hash for Meta {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.address as u64 | (self.locked as u64) << 32 | (self.load_pending as u64) << 33);
        state.write_u64(self.clock_phase as u64 | (self.timer_phase as u64) << 32);
    }
}

/// The table's keys are already well mixed or small numbers; a multiplication is enough.
#[derive(Default)]
pub struct Mix(u64);
impl Hasher for Mix {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 = (self.0 ^ *b as u64).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    fn write_u64(&mut self, n: u64) {
        self.0 = (self.0 ^ n).wrapping_mul(0x9e37_79b9_7f4a_7c15).rotate_left(29);
    }
}

#[derive(Clone)]
pub struct Table {
    /// u64 words of signals per node.
    words: usize,
    bits: Vec<u64>,
    /// Per node: its queued components, then its just-risen traces.
    lists: Vec<u32>,
    nodes: Vec<Node>,
    /// Per edge: the event, an arrival, and for an edge that ends in a load the word that arrival belongs to:
    /// the last word seen there, checked before `by_word` (an instruction fetch nearly always loads the same word).
    edges: Vec<(Edge, Arrival, u32)>,
    by_hash: HashMap<u64, u32, BuildHasherDefault<Mix>>,
    /// For an edge that ends in a load: (node, word loaded) to where it arrives.
    by_word: HashMap<u64, Arrival, BuildHasherDefault<Mix>>,
    /// Bytes the table may use. When it is reached no new node is stored; known ones keep working.
    pub memory_limit: usize,
    /// Ticks crossed by replay, for the statistics.
    pub replayed_ticks: u64,
    /// How often the table was emptied because it had reached its limit.
    pub flushes: u64,
    /// Whether anything was learned or forgotten since the table was made or loaded.
    pub changed: bool,
}

impl Table {
    pub fn new(words: usize, memory_limit: usize) -> Table {
        Table { words, bits: Vec::new(), lists: Vec::new(), nodes: Vec::new(), edges: Vec::new(), by_hash: Default::default(), by_word: Default::default(), memory_limit, replayed_ticks: 0, flushes: 0, changed: false }
    }

    /// Forgets everything (the memory limit and the statistics stay).
    pub fn clear(&mut self) {
        self.bits.clear();
        self.lists.clear();
        self.nodes.clear();
        self.edges.clear();
        self.by_hash.clear();
        self.by_word.clear();
        self.flushes += 1;
        self.changed = true;
    }

    pub fn nodes(&self) -> usize {
        self.nodes.len()
    }

    /// A close estimate of the bytes in use (the maps counted at 32 bytes per entry).
    pub fn memory(&self) -> usize {
        self.bits.len() * 8 + self.lists.len() * 4 + self.nodes.len() * std::mem::size_of::<Node>() + self.edges.len() * std::mem::size_of::<(Edge, Arrival, u32)>() + (self.by_hash.len() + self.by_word.len()) * 32
    }

    fn hash(bits: &[u64], dirty: &[u32], rose: &[u32], meta: Meta) -> u64 {
        // Four lanes, so the multiplications of neighbouring words do not wait for each other. They are four
        // variables and the words come four at a time: as an array indexed by the word's number the lanes
        // were kept in memory and did wait (attempt 29). The values are the same.
        let lane = |h: u64, w: u64| (h ^ w).wrapping_mul(0x9e37_79b9_7f4a_7c15).rotate_left(31);
        let (mut a, mut b, mut c, mut d) = (0x243f_6a88_85a3_08d3u64, 0x1319_8a2e_0370_7344u64, 0xa409_3822_299f_31d0u64, 0x082e_fa98_ec4e_6c89u64);
        let mut fours = bits.chunks_exact(4);
        for four in &mut fours {
            (a, b, c, d) = (lane(a, four[0]), lane(b, four[1]), lane(c, four[2]), lane(d, four[3]));
        }
        match *fours.remainder() {
            [x] => a = lane(a, x),
            [x, y] => (a, b) = (lane(a, x), lane(b, y)),
            [x, y, z] => (a, b, c) = (lane(a, x), lane(b, y), lane(c, z)),
            _ => {}
        }
        let mut x = a ^ b.rotate_left(17) ^ c.rotate_left(34) ^ d.rotate_left(51);
        let mut mix = |v: u64| x = (x ^ v).wrapping_mul(0xff51_afd7_ed55_8ccd).rotate_left(27);
        for e in dirty {
            mix(*e as u64);
        }
        mix(dirty.len() as u64);
        for e in rose {
            mix(*e as u64 | 1 << 40);
        }
        mix(meta.address as u64 | (meta.locked as u64) << 32 | (meta.load_pending as u64) << 33);
        mix(meta.clock_phase as u64 | (meta.timer_phase as u64) << 32);
        x ^ x >> 29
    }

    /// The node with exactly this state: an existing one, a new one, or NONE when it is new and the table is full.
    #[cfg_attr(profiling, inline(never))]
    pub fn intern(&mut self, bits: &[u64], dirty: &[u32], rose: &[u32], meta: Meta) -> u32 {
        debug_assert_eq!(bits.len(), self.words);
        let hash = Table::hash(bits, dirty, rose, meta);
        let first = self.by_hash.get(&hash).copied().unwrap_or(NONE);
        let mut id = first;
        while id != NONE {
            let (b, d, r, m) = self.node(id);
            if m == meta && d == dirty && r == rose && b == bits {
                return id;
            }
            id = self.nodes[id as usize].next;
        }
        if self.memory() + self.words * 8 + (dirty.len() + rose.len()) * 4 + 128 > self.memory_limit || self.nodes.len() >= NONE as usize - 1 || self.lists.len() + dirty.len() + rose.len() >= NONE as usize {
            return NONE;
        }
        let id = self.nodes.len() as u32;
        self.changed = true;
        self.nodes.push(Node { hash, next: first, lists_at: self.lists.len() as u32, dirty_len: dirty.len() as u32, rose_len: rose.len() as u32, meta, edge: NONE });
        self.bits.extend_from_slice(bits);
        self.lists.extend_from_slice(dirty);
        self.lists.extend_from_slice(rose);
        self.by_hash.insert(hash, id);
        debug_assert_eq!(self.nodes[id as usize].hash, hash);
        id
    }

    /// A node's signals, queued components, just-risen traces and the rest.
    pub fn node(&self, id: u32) -> (&[u64], &[u32], &[u32], Meta) {
        let n = &self.nodes[id as usize];
        let at = n.lists_at as usize;
        let (dirty, rose) = self.lists[at..at + (n.dirty_len + n.rose_len) as usize].split_at(n.dirty_len as usize);
        (&self.bits[id as usize * self.words..(id as usize + 1) * self.words], dirty, rose, n.meta)
    }

    /// Stores that `from` leads over `edge` to `to`. `word` is the word loaded at the event, if it loads.
    #[cfg_attr(profiling, inline(never))]
    pub fn record(&mut self, from: u32, edge: Edge, word: u32, to: Arrival) {
        let at = self.nodes[from as usize].edge;
        if at == NONE {
            self.changed = true;
            self.nodes[from as usize].edge = self.edges.len() as u32;
            self.edges.push((edge, to, word));
        } else {
            // The circuit is deterministic: a node always leads to the same event after the same number of ticks.
            assert!(self.edges[at as usize].0 == edge, "replay: one state led to two different events");
        }
        if edge.fetch != NONE {
            let entry = &mut self.edges[self.nodes[from as usize].edge as usize];
            (entry.1, entry.2) = (to, word);
            self.changed |= self.by_word.insert((from as u64) << 32 | word as u64, to).is_none();
        }
    }

    /// The edge of a node, if it has been seen.
    pub fn edge(&self, from: u32) -> Option<Edge> {
        let at = self.nodes[from as usize].edge;
        (at != NONE).then(|| self.edges[at as usize].0)
    }

    /// Where a node's edge arrives. `word` is the word that the event loads, if it loads.
    #[cfg_attr(profiling, inline(never))]
    pub fn arrival(&mut self, from: u32, edge: Edge, word: u32) -> Option<Arrival> {
        let entry = &mut self.edges[self.nodes[from as usize].edge as usize];
        if edge.fetch == NONE || entry.2 == word {
            return Some(entry.1);
        }
        // Another word than last time: the map knows it or not, and the edge remembers it for the next time.
        let to = *self.by_word.get(&((from as u64) << 32 | word as u64))?;
        (entry.1, entry.2) = (to, word);
        Some(to)
    }
}

// ---------------------------------------------------------------------------------------------------------
// The table on disk (v2.1 step 5): so that a run can start with what an earlier run learned.
//
// The file is only read back by the same build of the engine for the same circuit (the fingerprint says so),
// and everything in it is checked before it is used: a number in it may index the engine's arrays without a
// bounds check later. The words are written as this machine holds them; a file from a machine of the other
// byte order has a different magic number and is not accepted.
// ---------------------------------------------------------------------------------------------------------

const MAGIC: u32 = 0x5643_4252; // "VCBR"
const VERSION: u32 = 1;
const NODE_WORDS: usize = 11;
const EDGE_WORDS: usize = 7;
const WORD_WORDS: usize = 4;

fn bytes_of<T: Copy>(v: &[T]) -> &[u8] {
    // Safety: u32 and u64 have no padding and any byte pattern; the length is exact.
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

fn read_vec<T: Copy + Default>(file: &mut impl std::io::Read, len: usize) -> std::io::Result<Vec<T>> {
    let mut v = vec![T::default(); len];
    // Safety: as above; the vector owns `len` initialised elements.
    file.read_exact(unsafe { std::slice::from_raw_parts_mut(v.as_mut_ptr() as *mut u8, len * std::mem::size_of::<T>()) })?;
    Ok(v)
}

impl Table {
    /// Writes the table to `path` (through a temporary file beside it, so a half-written file is never left).
    pub fn save(&self, path: &std::path::Path, fingerprint: &[u8; 32]) -> std::io::Result<()> {
        use std::io::Write;
        let nodes: Vec<u32> = self.nodes.iter().flat_map(|n| [n.hash as u32, (n.hash >> 32) as u32, n.next, n.lists_at, n.dirty_len, n.rose_len, n.meta.address, n.meta.locked as u32 | (n.meta.load_pending as u32) << 1, n.meta.clock_phase, n.meta.timer_phase, n.edge]).collect();
        let edges: Vec<u32> = self.edges.iter().flat_map(|(e, a, _)| [e.ticks as u32, (e.ticks >> 32) as u32, e.fetch, e.write, e.breakpoint as u32, a.node, a.written]).collect();
        let by_word: Vec<u32> = self.by_word.iter().flat_map(|(k, a)| [*k as u32, (*k >> 32) as u32, a.node, a.written]).collect();
        let header = [self.words as u64, self.bits.len() as u64, self.lists.len() as u64, self.nodes.len() as u64, self.edges.len() as u64, self.by_word.len() as u64];
        let temporary = path.with_extension("tmp");
        let mut file = std::io::BufWriter::new(std::fs::File::create(&temporary)?);
        file.write_all(bytes_of(&[MAGIC, VERSION]))?;
        file.write_all(fingerprint)?;
        file.write_all(bytes_of(&header))?;
        file.write_all(bytes_of(&self.bits))?;
        for block in [&self.lists, &nodes, &edges, &by_word] {
            file.write_all(bytes_of(block))?;
        }
        file.into_inner()?.sync_all()?;
        std::fs::rename(&temporary, path)
    }

    /// Reads a table written by `save`. `Ok(None)` with a reason when the file is for another circuit or another
    /// build; an error when it cannot be read or is damaged. `entities` and `vmem_words` bound what it may name.
    pub fn load(path: &std::path::Path, fingerprint: &[u8; 32], words: usize, memory_limit: usize, entities: u32, vmem_words: u32) -> Result<std::result::Result<Table, String>, String> {
        use std::io::Read;
        let damaged = |what: &str| format!("{}: damaged ({what})", path.display());
        let mut file = std::io::BufReader::new(std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?);
        let io = |e: std::io::Error| format!("{}: {e}", path.display());
        let head: Vec<u32> = read_vec(&mut file, 2).map_err(io)?;
        let mut theirs = [0u8; 32];
        file.read_exact(&mut theirs).map_err(io)?;
        if head != [MAGIC, VERSION] {
            return Ok(Err("it is not a replay table of this version".into()));
        }
        if theirs != *fingerprint {
            return Ok(Err("it was made for another board or by another build of the engine".into()));
        }
        let h: Vec<u64> = read_vec(&mut file, 6).map_err(io)?;
        let (n_bits, n_lists, n_nodes, n_edges, n_words) = (h[1] as usize, h[2] as usize, h[3] as usize, h[4] as usize, h[5] as usize);
        if h[0] as usize != words || n_bits != n_nodes.saturating_mul(words) || n_nodes >= NONE as usize || n_lists >= NONE as usize || n_edges > n_nodes {
            return Err(damaged("sizes"));
        }
        let size = n_bits * 8 + (n_lists + n_nodes * NODE_WORDS + n_edges * EDGE_WORDS + n_words * WORD_WORDS) * 4;
        if size > memory_limit {
            return Ok(Err(format!("it holds {} MiB, more than the memory replay may use", size >> 20)));
        }
        let bits: Vec<u64> = read_vec(&mut file, n_bits).map_err(io)?;
        let lists: Vec<u32> = read_vec(&mut file, n_lists).map_err(io)?;
        let raw_nodes: Vec<u32> = read_vec(&mut file, n_nodes * NODE_WORDS).map_err(io)?;
        let raw_edges: Vec<u32> = read_vec(&mut file, n_edges * EDGE_WORDS).map_err(io)?;
        let raw_words: Vec<u32> = read_vec(&mut file, n_words * WORD_WORDS).map_err(io)?;
        if lists.iter().any(|e| *e >= entities) {
            return Err(damaged("an entity number"));
        }
        let address_ok = |a: u32| a == NONE || a < vmem_words;
        let mut table = Table::new(words, memory_limit);
        table.by_hash.reserve(n_nodes);
        for (id, n) in raw_nodes.chunks_exact(NODE_WORDS).enumerate() {
            let node = Node { hash: n[0] as u64 | (n[1] as u64) << 32, next: n[2], lists_at: n[3], dirty_len: n[4], rose_len: n[5], meta: Meta { address: n[6], locked: n[7] & 1 == 1, load_pending: n[7] & 2 == 2, clock_phase: n[8], timer_phase: n[9] }, edge: n[10] };
            let end = node.lists_at as u64 + node.dirty_len as u64 + node.rose_len as u64;
            if end > n_lists as u64 || (node.next != NONE && node.next as usize >= id) || (node.edge != NONE && node.edge as usize >= n_edges) || node.meta.address >= vmem_words {
                return Err(damaged("a node"));
            }
            table.by_hash.insert(node.hash, id as u32);
            table.nodes.push(node);
        }
        for e in raw_edges.chunks_exact(EDGE_WORDS) {
            if e[5] as usize >= n_nodes || !address_ok(e[2]) || !address_ok(e[3]) {
                return Err(damaged("a step"));
            }
            // The file does not say which word a load edge's arrival belongs to: it is found among the words below.
            table.edges.push((Edge { ticks: e[0] as u64 | (e[1] as u64) << 32, fetch: e[2], write: e[3], breakpoint: e[4] != 0 }, Arrival { node: if e[2] == NONE { e[5] } else { NONE }, written: e[6] }, 0));
        }
        table.by_word.reserve(n_words);
        for w in raw_words.chunks_exact(WORD_WORDS) {
            if w[1] as usize >= n_nodes || w[2] as usize >= n_nodes {
                return Err(damaged("an arrival"));
            }
            table.by_word.insert(w[0] as u64 | (w[1] as u64) << 32, Arrival { node: w[2], written: w[3] });
            let at = table.nodes[w[1] as usize].edge;
            if at != NONE && table.edges[at as usize].0.fetch != NONE {
                (table.edges[at as usize].1, table.edges[at as usize].2) = (Arrival { node: w[2], written: w[3] }, w[0]);
            }
        }
        if table.edges.iter().any(|e| e.1.node == NONE) {
            return Err(damaged("a load without a word"));
        }
        table.bits = bits;
        table.lists = lists;
        Ok(Ok(table))
    }
}
