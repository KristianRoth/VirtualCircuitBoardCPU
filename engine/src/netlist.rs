//! Board → entities and connections. Rules: specs/vcb-engine-v1.md, "Netlist".
//! Implemented so far: rules 1, 2, 8 and 9 (traces, components, connections).

use crate::board::Board;
use crate::format::Result;
use crate::ink::Ink;

/// `entity_of` value for a pixel that belongs to no entity.
pub const NONE: u32 = u32::MAX;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Trace,
    Component(Ink),
}

pub struct Components {
    pub width: usize,
    pub height: usize,
    /// Pixel → entity id, or `NONE`.
    pub entity_of: Vec<u32>,
    /// Entity id → kind. Ids are in order of each entity's top-left-most pixel.
    pub kinds: Vec<Kind>,
    /// Component → the traces it reads / writes. Empty for traces.
    pub reads: Vec<Vec<u32>>,
    pub writes: Vec<Vec<u32>>,
    /// Trace → the components that read / write it. Empty for components.
    pub readers: Vec<Vec<u32>>,
    pub writers: Vec<Vec<u32>>,
}

/// The kind of entity a pixel belongs to, or `None` for inks without logic.
fn kind_of(ink: Ink, x: usize, y: usize) -> Result<Option<Kind>> {
    Ok(match ink {
        Ink::Empty | Ink::Annotation | Ink::Filler => None,
        Ink::Trace(_) | Ink::Read | Ink::Write => Some(Kind::Trace),
        Ink::Cross | Ink::Tunnel | Ink::Mesh | Ink::Bus(_) | Ink::Wireless(_) => {
            return Err(format!("{ink} at {x},{y}: this ink's netlist rule is not implemented yet"));
        }
        component => Some(Kind::Component(component)),
    })
}

fn find(parent: &mut [u32], mut i: u32) -> u32 {
    while parent[i as usize] != i {
        parent[i as usize] = parent[parent[i as usize] as usize]; // path halving
        i = parent[i as usize];
    }
    i
}

impl Components {
    pub fn build(board: &Board) -> Result<Components> {
        let (w, h) = (board.width, board.height);
        let mut kind_at = Vec::with_capacity(w * h);
        for (i, ink) in board.inks.iter().enumerate() {
            kind_at.push(kind_of(*ink, i % w, i / w)?);
        }

        // Rules 1 and 2: touching pixels of the same kind are one entity.
        let mut parent: Vec<u32> = (0..(w * h) as u32).collect();
        for i in 0..w * h {
            if kind_at[i].is_none() {
                continue;
            }
            for j in [(i % w + 1 < w).then_some(i + 1), (i + w < w * h).then_some(i + w)].into_iter().flatten() {
                if kind_at[j] == kind_at[i] {
                    let (a, b) = (find(&mut parent, i as u32), find(&mut parent, j as u32));
                    parent[a.max(b) as usize] = a.min(b);
                }
            }
        }

        // Number entities in row order of their first pixel.
        let mut entity_of = vec![NONE; w * h];
        let mut kinds = Vec::new();
        for i in 0..w * h {
            let Some(kind) = kind_at[i] else { continue };
            let root = find(&mut parent, i as u32) as usize;
            if entity_of[root] == NONE {
                entity_of[root] = kinds.len() as u32; // the root is the entity's first pixel
                kinds.push(kind);
            }
            entity_of[i] = entity_of[root];
        }

        // Rule 8: a component next to a Write pixel writes that trace; next to a Read pixel it reads it.
        let n = kinds.len();
        let mut c = Components {
            width: w,
            height: h,
            entity_of,
            kinds,
            reads: vec![Vec::new(); n],
            writes: vec![Vec::new(); n],
            readers: vec![Vec::new(); n],
            writers: vec![Vec::new(); n],
        };
        for i in 0..w * h {
            let ink = board.inks[i];
            if ink != Ink::Read && ink != Ink::Write {
                continue;
            }
            let trace = c.entity_of[i];
            let (x, y) = (i % w, i / w);
            let neighbours = [(x > 0).then(|| i - 1), (x + 1 < w).then(|| i + 1), (y > 0).then(|| i - w), (y + 1 < h).then(|| i + w)];
            for j in neighbours.into_iter().flatten() {
                let component = c.entity_of[j];
                if component == NONE || c.kinds[component as usize] == Kind::Trace {
                    continue;
                }
                // Connected at most once in each direction.
                let (of_component, of_trace) = if ink == Ink::Read { (&mut c.reads, &mut c.readers) } else { (&mut c.writes, &mut c.writers) };
                if !of_component[component as usize].contains(&trace) {
                    of_component[component as usize].push(trace);
                    of_trace[trace as usize].push(component);
                }
            }
        }
        Ok(c)
    }

    /// The entity at a pixel, if any.
    pub fn entity_at(&self, x: usize, y: usize) -> Option<u32> {
        if x >= self.width || y >= self.height {
            return None;
        }
        Some(self.entity_of[y * self.width + x]).filter(|e| *e != NONE)
    }
}
