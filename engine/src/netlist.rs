//! Board → entities and connections. Rules: specs/vcb-engine-v1.md, "Netlist".

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

/// What a pixel is to the netlist builder.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Px {
    /// Empty, Annotation, Filler.
    Nothing,
    Trace,
    Component(Ink),
    /// Buses carry entities but are not simulated themselves.
    Bus,
    Cross,
    Tunnel,
    Mesh,
}

impl Px {
    fn of(ink: Ink) -> Px {
        match ink {
            Ink::Empty | Ink::Annotation | Ink::Filler => Px::Nothing,
            Ink::Trace(_) | Ink::Read | Ink::Write => Px::Trace,
            Ink::Bus(_) => Px::Bus,
            Ink::Cross => Px::Cross,
            Ink::Tunnel => Px::Tunnel,
            Ink::Mesh => Px::Mesh,
            component => Px::Component(component),
        }
    }

    /// Traces, components and buses can be joined by cross, tunnel and mesh.
    fn joinable(self) -> bool {
        matches!(self, Px::Trace | Px::Component(_) | Px::Bus)
    }
}

/// What "same ink" means at a tunnel, mesh or bus: the exact ink of the touching pixel,
/// trace colour included. All bus colours count as one. (Model, facts question 12.)
fn key(ink: Ink) -> Ink {
    match ink {
        Ink::Bus(_) => Ink::Bus(0),
        other => other,
    }
}

fn find(parent: &mut [u32], mut i: u32) -> u32 {
    while parent[i as usize] != i {
        parent[i as usize] = parent[parent[i as usize] as usize]; // path halving
        i = parent[i as usize];
    }
    i
}

/// Joins two pixels' groups. The lower pixel index stays the root, so a root is its group's first pixel.
fn union(parent: &mut [u32], a: usize, b: usize) {
    let (a, b) = (find(parent, a as u32), find(parent, b as u32));
    parent[a.max(b) as usize] = a.min(b);
}

impl Components {
    pub fn build(board: &Board) -> Result<Components> {
        let (w, h) = (board.width, board.height);
        let px: Vec<Px> = board.inks.iter().map(|ink| Px::of(*ink)).collect();
        let mut parent: Vec<u32> = (0..(w * h) as u32).collect();
        // The pixel one step from `i` in direction `d` (0 right, 1 down, 2 left, 3 up), if it is on the board.
        let step = |i: usize, d: usize| match d {
            0 => (i % w + 1 < w).then(|| i + 1),
            1 => (i + w < w * h).then(|| i + w),
            2 => (i % w > 0).then(|| i - 1),
            _ => (i >= w).then(|| i - w),
        };

        // Rules 1-3: touching pixels of the same kind are one entity. One Wireless ink is one component board-wide.
        let mut first_wireless = [None; 4];
        for i in 0..w * h {
            if !px[i].joinable() {
                continue;
            }
            for j in [step(i, 0), step(i, 1)].into_iter().flatten() {
                if px[j] == px[i] {
                    union(&mut parent, i, j);
                }
            }
            if let Ink::Wireless(n) = board.inks[i] {
                match first_wireless[n as usize] {
                    Some(first) => union(&mut parent, first, i),
                    None => first_wireless[n as usize] = Some(i),
                }
            }
        }

        let mut mesh_groups: std::collections::HashMap<Ink, usize> = Default::default();
        for i in 0..w * h {
            match px[i] {
                // Rule 4: left joins right and up joins down when both sides are the same kind.
                Px::Cross => {
                    for d in 0..2 {
                        let (Some(a), Some(b)) = (step(i, d), step(i, d + 2)) else { continue };
                        if px[a] == Px::Cross || px[b] == Px::Cross {
                            return Err(format!("Cross at {},{}: touches another Cross, which the game does not allow", i % w, i / w));
                        }
                        if px[a].joinable() && px[a] == px[b] {
                            union(&mut parent, a, b);
                        }
                    }
                }
                // Rule 5: a tunnel pixel with ink on one side is an entrance. The ink joins the same ink on the
                // far side of the next tunnel pixel that has it there. The game refuses a board where an entrance
                // has no exit, or where another entrance for the same ink comes first (observed 2026-11-15).
                Px::Tunnel => {
                    const NAMES: [&str; 4] = ["left to right", "top to bottom", "right to left", "bottom to top"];
                    for d in 0..4 {
                        let Some(behind) = step(i, (d + 2) % 4).filter(|n| px[*n].joinable()) else { continue };
                        let same_ink = |j: Option<usize>| j.filter(|j| px[*j].joinable() && key(board.inks[*j]) == key(board.inks[behind]));
                        let mut q = step(i, d);
                        loop {
                            let Some(t) = q else {
                                return Err(format!("Tunnel at {},{}: no exit with the same ink ({})", i % w, i / w, NAMES[d]));
                            };
                            if px[t] == Px::Tunnel {
                                if same_ink(step(t, (d + 2) % 4)).is_some() {
                                    return Err(format!("Tunnel at {},{}: another entrance for the same ink at {},{} comes before an exit ({})", i % w, i / w, t % w, t / w, NAMES[d]));
                                }
                                if let Some(far) = same_ink(step(t, d)) {
                                    union(&mut parent, behind, far);
                                    break;
                                }
                            }
                            q = step(t, d);
                        }
                    }
                }
                // Rule 6: everything with the same key that touches any mesh pixel is joined.
                Px::Mesh => {
                    for j in (0..4).filter_map(|d| step(i, d)).filter(|j| px[*j].joinable()) {
                        let first = *mesh_groups.entry(key(board.inks[j])).or_insert(j);
                        union(&mut parent, first, j);
                    }
                }
                _ => {}
            }
        }

        // Rule 7, after the joins above have settled which bus pixels are one bus:
        // traces and components with the same key that touch the same bus are joined.
        let mut bus_groups: std::collections::HashMap<(u32, Ink), usize> = Default::default();
        for i in (0..w * h).filter(|i| matches!(px[*i], Px::Trace | Px::Component(_))) {
            for bus in (0..4).filter_map(|d| step(i, d)).filter(|j| px[*j] == Px::Bus) {
                let bus_root = find(&mut parent, bus as u32);
                let first = *bus_groups.entry((bus_root, key(board.inks[i]))).or_insert(i);
                union(&mut parent, first, i);
            }
        }

        let kind_at: Vec<Option<Kind>> = px.iter().map(|p| match p {
            Px::Trace => Some(Kind::Trace),
            Px::Component(ink) => Some(Kind::Component(*ink)),
            _ => None,
        }).collect();

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
