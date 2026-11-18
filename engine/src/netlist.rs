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
    /// A VMem address or content pin: a latch the game places at the positions in `vmem_settings`.
    Pin,
}

#[derive(Clone)]
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
    /// VMem pins as entity ids, bit 0 first. Empty when VMem is disabled.
    pub address_pins: Vec<u32>,
    pub content_pins: Vec<u32>,
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
    /// A VMem pin. Each pin is its own entity and never merges with a neighbour.
    Pin,
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
        let mut px: Vec<Px> = board.inks.iter().map(|ink| Px::of(*ink)).collect();

        // Step 1: the game replaces whatever is drawn at the pin positions with pin latches.
        let mut pin_pixels: [Vec<usize>; 2] = Default::default();
        let mut pin_rows: Vec<(usize, usize)> = Vec::new(); // (a pin's first pixel, another pixel of it)
        if board.settings.vmem_enabled {
            let s = &board.settings.vmem_settings;
            for (group, name, base, max_bits) in [(0, "address", 0, 20), (1, "content", 7, 32)] {
                let [bits, x, y, offset_x, offset_y, size_x, size_y] = s[base..base + 7] else { unreachable!() };
                // Observed: the game raises a smaller bit count to 4 when it loads the file, which moves the pins.
                if !(4..=max_bits).contains(&bits) {
                    return Err(format!("vmem_settings: {bits} {name} bits, expected 4 to {max_bits}"));
                }
                // Model (question 4): a pin 1 wide and n high is one pin over the n pixels downwards from its position,
                // as the game's 32-bit sample computer draws its pin latches. Wider pins have not been seen.
                if size_x != 1 || size_y < 1 {
                    return Err(format!("vmem_settings: {name} pin size {size_x}x{size_y} is not supported, only a width of 1 has been seen"));
                }
                for k in 0..bits {
                    for row in 0..size_y {
                        let (px_x, px_y) = (x - offset_x * k, y + offset_y * k + row);
                        if px_x < 0 || px_y < 0 || px_x >= w as i64 || px_y >= h as i64 {
                            return Err(format!("vmem_settings: {name} pin {k} at {px_x},{px_y} is outside the board"));
                        }
                        let i = px_y as usize * w + px_x as usize;
                        if px[i] == Px::Pin {
                            return Err(format!("vmem_settings: {name} pin {k} at {px_x},{px_y} is on top of another pin"));
                        }
                        px[i] = Px::Pin;
                        if row == 0 {
                            pin_pixels[group].push(i);
                        } else {
                            pin_rows.push((i - row as usize * w, i));
                        }
                    }
                }
            }
        }
        let mut parent: Vec<u32> = (0..(w * h) as u32).collect();
        // The pixel one step from `i` in direction `d` (0 right, 1 down, 2 left, 3 up), if it is on the board.
        let step = |i: usize, d: usize| match d {
            0 => (i % w + 1 < w).then(|| i + 1),
            1 => (i + w < w * h).then(|| i + w),
            2 => (i % w > 0).then(|| i - 1),
            _ => (i >= w).then(|| i - w),
        };
        for (first, other) in pin_rows {
            union(&mut parent, first, other);
        }

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
                        // Model (question 12): a bus at a tunnel is matched by its own colour, although touching bus pixels of any
                        // colour are one bus. The game's compact 32-bit sample computer needs it: a Bus 4 pair with Bus 3 between.
                        let same_ink = |j: Option<usize>| j.filter(|j| px[*j].joinable() && board.inks[*j] == board.inks[behind]);
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
            Px::Pin => Some(Kind::Pin),
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
            address_pins: Vec::new(),
            content_pins: Vec::new(),
        };
        c.address_pins = pin_pixels[0].iter().map(|i| c.entity_of[*i]).collect();
        c.content_pins = pin_pixels[1].iter().map(|i| c.entity_of[*i]).collect();
        // Connections are found by walking each trace from its first pixel, breadth first. The order matters
        // only for Random: components are evaluated in the order their Read pixels are reached, and each draws
        // from the one generator. Observed on the random recorder boards; the walk order itself is a model (question 22).
        let mut visited = vec![false; w * h];
        let mut queue = std::collections::VecDeque::new();
        for start in 0..w * h {
            if px[start] != Px::Trace || visited[start] {
                continue;
            }
            visited[start] = true;
            queue.push_back(start);
            while let Some(i) = queue.pop_front() {
                let ink = board.inks[i];
                let trace = c.entity_of[i];
                for j in (0..4).filter_map(|d| step(i, d)) {
                    if px[j] == Px::Trace {
                        if !visited[j] {
                            visited[j] = true;
                            queue.push_back(j);
                        }
                        continue;
                    }
                    let component = c.entity_of[j];
                    if component == NONE || (ink != Ink::Read && ink != Ink::Write) {
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
