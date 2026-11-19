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
#[derive(Clone, Copy, PartialEq, Default)]
pub struct Meta {
    /// The address the address pins hold.
    pub address: u32,
    pub locked: bool,
    pub load_pending: bool,
    /// The tick modulo the clock interval, and modulo the timer interval.
    pub clock_phase: u32,
    pub timer_phase: u32,
}

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

pub struct Table {
    /// u64 words of signals per node.
    words: usize,
    bits: Vec<u64>,
    /// Per node: its queued components, then its just-risen traces.
    lists: Vec<u32>,
    nodes: Vec<Node>,
    edges: Vec<(Edge, Arrival)>,
    by_hash: HashMap<u64, u32, BuildHasherDefault<Mix>>,
    /// For an edge that ends in a load: (node, word loaded) to where it arrives.
    by_word: HashMap<u64, Arrival, BuildHasherDefault<Mix>>,
    /// Bytes the table may use. When it is reached no new node is stored; known ones keep working.
    pub memory_limit: usize,
    /// Ticks crossed by replay, for the statistics.
    pub replayed_ticks: u64,
}

impl Table {
    pub fn new(words: usize, memory_limit: usize) -> Table {
        Table { words, bits: Vec::new(), lists: Vec::new(), nodes: Vec::new(), edges: Vec::new(), by_hash: Default::default(), by_word: Default::default(), memory_limit, replayed_ticks: 0 }
    }

    pub fn nodes(&self) -> usize {
        self.nodes.len()
    }

    /// A close estimate of the bytes in use (the maps counted at 32 bytes per entry).
    pub fn memory(&self) -> usize {
        self.bits.len() * 8 + self.lists.len() * 4 + self.nodes.len() * std::mem::size_of::<Node>() + self.edges.len() * std::mem::size_of::<(Edge, Arrival)>() + (self.by_hash.len() + self.by_word.len()) * 32
    }

    fn hash(bits: &[u64], dirty: &[u32], rose: &[u32], meta: Meta) -> u64 {
        // Four lanes, so the multiplications of neighbouring words do not wait for each other.
        let mut h = [0x243f_6a88_85a3_08d3u64, 0x1319_8a2e_0370_7344, 0xa409_3822_299f_31d0, 0x082e_fa98_ec4e_6c89];
        for (i, w) in bits.iter().enumerate() {
            h[i & 3] = (h[i & 3] ^ *w).wrapping_mul(0x9e37_79b9_7f4a_7c15).rotate_left(31);
        }
        let mut x = h[0] ^ h[1].rotate_left(17) ^ h[2].rotate_left(34) ^ h[3].rotate_left(51);
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
            self.nodes[from as usize].edge = self.edges.len() as u32;
            self.edges.push((edge, to));
        } else {
            // The circuit is deterministic: a node always leads to the same event after the same number of ticks.
            assert!(self.edges[at as usize].0 == edge, "replay: one state led to two different events");
        }
        if edge.fetch != NONE {
            self.by_word.insert((from as u64) << 32 | word as u64, to);
        }
    }

    /// The edge of a node, if it has been seen.
    pub fn edge(&self, from: u32) -> Option<Edge> {
        let at = self.nodes[from as usize].edge;
        (at != NONE).then(|| self.edges[at as usize].0)
    }

    /// Where a node's edge arrives. `word` is the word that the event loads, if it loads.
    #[cfg_attr(profiling, inline(never))]
    pub fn arrival(&self, from: u32, edge: Edge, word: u32) -> Option<Arrival> {
        if edge.fetch == NONE {
            Some(self.edges[self.nodes[from as usize].edge as usize].1)
        } else {
            self.by_word.get(&((from as u64) << 32 | word as u64)).copied()
        }
    }
}
