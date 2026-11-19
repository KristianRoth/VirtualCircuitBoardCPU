//! The tree of parts of a board, as JSON for a picture. Parts come from the netlist alone: at each level,
//! every trace that more than T components share is cut (T = 32, 16, 8, 4, 2: the same for every board), and
//! what stays connected is a part. A part of a finer level lies inside one part of the coarser level.
//!   cargo run --release --example parts_tree -- <board.vcb> <program.vcbasm or -> [ticks] > tree.json
use std::collections::HashMap;
use std::path::Path;
use vcb::netlist::{Components, Kind};
use vcb::sim::Simulation;
use vcb::Ink;

const NONE: u32 = u32::MAX;

fn connected(n: usize, inside: &[bool], cut: &[bool], c: &Components) -> (Vec<u32>, usize) {
    let mut part = vec![NONE; n];
    let mut parts = 0;
    for start in 0..n {
        if !inside[start] || cut[start] || part[start] != NONE {
            continue;
        }
        let mut todo = vec![start];
        part[start] = parts as u32;
        while let Some(e) = todo.pop() {
            let next: Vec<u32> = if c.kinds[e] == Kind::Trace { c.readers[e].iter().chain(&c.writers[e]).copied().collect() } else { c.reads[e].iter().chain(&c.writes[e]).copied().collect() };
            for x in next {
                let x = x as usize;
                if inside[x] && !cut[x] && part[x] == NONE {
                    part[x] = parts as u32;
                    todo.push(x);
                }
            }
        }
        parts += 1;
    }
    (part, parts)
}

#[derive(Default, Clone)]
struct Node {
    name: String,
    kind: &'static str,
    parent: usize,
    entities: usize,
    components: usize,
    latches: usize,
    inputs: usize,
    outputs: usize,
    flips: u64,
    events: u64,
    bbox: [usize; 4],
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let mut board = vcb::Board::read(Path::new(&args[1]))?;
    let source = if args[2] == "-" { board.assembly.clone() } else { std::fs::read_to_string(&args[2]).map_err(|e| e.to_string())?.replace("\r\n", "\n") };
    vcb::asm::assemble(&source, &args[2], &mut board, true)?;
    let ticks: u64 = args.get(3).map_or(2_000_000, |t| t.parse().unwrap());
    let options = vcb::sim::Options { timer_ticks: Some(1000) };
    let c = Components::build(&board)?;
    let n = c.kinds.len();
    let live = vcb::sim2::Engine::new(&board, c.clone(), &options)?.live().to_vec();
    let simulated: Vec<bool> = (0..n).map(|e| live[e] || (c.kinds[e] == Kind::Trace && c.writers[e].len() == 1 && live[c.writers[e][0] as usize])).collect();
    // The self-running part: clock-driven, never influenced by a pin, Random or Timer, and leading to something simulated.
    let reach = |from: &dyn Fn(usize) -> bool, forward: bool| {
        let mut seen = vec![false; n];
        let mut todo: Vec<usize> = (0..n).filter(|e| from(*e)).collect();
        while let Some(e) = todo.pop() {
            if !std::mem::replace(&mut seen[e], true) {
                let next = match (c.kinds[e] == Kind::Trace, forward) {
                    (true, true) => &c.readers[e],
                    (false, true) => &c.writes[e],
                    (true, false) => &c.writers[e],
                    (false, false) => &c.reads[e],
                };
                todo.extend(next.iter().map(|x| *x as usize).filter(|x| !forward || c.kinds[*x] != Kind::Component(Ink::Clock)));
            }
        }
        seen
    };
    let influenced = reach(&|e| matches!(c.kinds[e], Kind::Pin | Kind::Component(Ink::Random | Ink::Timer)), true);
    let driven = reach(&|e| c.kinds[e] == Kind::Component(Ink::Clock), true);
    let needed = reach(&|e| simulated[e], false);
    let own: Vec<bool> = (0..n).map(|e| driven[e] && !influenced[e] && needed[e]).collect();
    let rest: Vec<bool> = (0..n).map(|e| simulated[e] && !own[e]).collect();
    let degree: Vec<usize> = (0..n).map(|e| if c.kinds[e] == Kind::Trace && rest[e] { c.readers[e].iter().chain(&c.writers[e]).filter(|x| rest[**x as usize]).count() } else { 0 }).collect();

    // Pixels per entity, for the boxes.
    let mut bbox = vec![[usize::MAX, usize::MAX, 0, 0]; n];
    for (i, e) in c.entity_of.iter().enumerate() {
        if (*e as usize) < n {
            let (x, y, b) = (i % c.width, i / c.width, &mut bbox[*e as usize]);
            *b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
        }
    }

    // Nodes: 0 the whole simulated circuit, 1 the self-running part, then the levels.
    let mut nodes: Vec<Node> = vec![Node { name: "the simulated circuit".into(), kind: "root", ..Default::default() }, Node { name: "runs by itself (played from a schedule)".into(), kind: "self", parent: 0, ..Default::default() }];
    // Per entity: its node at each level (the deepest is where its flips are counted).
    let mut chain: Vec<Vec<usize>> = (0..n).map(|e| if own[e] { vec![0, 1] } else if rest[e] { vec![0] } else { vec![] }).collect();
    let thresholds = [32usize, 16, 8, 4, 2];
    let mut ended = vec![false; n]; // in a "connecting traces" or "fragments" leaf: goes no deeper
    for t in thresholds {
        let cut: Vec<bool> = degree.iter().map(|d| *d > t).collect();
        let (part, parts) = connected(n, &rest, &cut, &c);
        let mut size = vec![0usize; parts];
        for p in part.iter().filter(|p| **p != NONE) {
            size[*p as usize] += 1;
        }
        let mut node_of: HashMap<(usize, u32), usize> = HashMap::new(); // (parent, part or a marker) -> node
        for e in 0..n {
            if !rest[e] || ended[e] {
                continue;
            }
            let parent = *chain[e].last().unwrap();
            let (key, name, kind) = if cut[e] {
                ended[e] = true;
                (NONE, format!("connecting traces (shared by more than {t})"), "cut")
            } else if size[part[e] as usize] < 4 {
                ended[e] = true;
                (NONE - 1, "fragments (fewer than 4 entities each)".to_string(), "fragments")
            } else {
                (part[e], format!("part at T = {t}"), "part")
            };
            let id = *node_of.entry((parent, key)).or_insert_with(|| {
                nodes.push(Node { name, kind, parent, ..Default::default() });
                nodes.len() - 1
            });
            chain[e].push(id);
        }
    }
    // Sizes, boxes, inputs and outputs of every node.
    for e in 0..n {
        for &id in &chain[e] {
            let node = &mut nodes[id];
            if node.entities == 0 {
                node.bbox = [usize::MAX, usize::MAX, 0, 0];
            }
            node.entities += 1;
            node.components += (c.kinds[e] != Kind::Trace) as usize;
            node.latches += matches!(c.kinds[e], Kind::Component(Ink::LatchOn | Ink::LatchOff)) as usize;
            let b = bbox[e];
            if b[0] != usize::MAX {
                node.bbox = [node.bbox[0].min(b[0]), node.bbox[1].min(b[1]), node.bbox[2].max(b[2]), node.bbox[3].max(b[3])];
            }
        }
    }
    let inside = |e: usize, id: usize| chain[e].contains(&id);
    // feeds[x]: the nodes that read x from outside. An input of a node is such an x.
    let mut feeds: Vec<Vec<usize>> = vec![Vec::new(); n];
    for e in 0..n {
        for &id in chain[e].iter().skip(1) {
            for &x in if c.kinds[e] == Kind::Trace { &c.writers[e] } else { &c.reads[e] } {
                let x = x as usize;
                if !chain[x].is_empty() && !inside(x, id) && !feeds[x].contains(&id) {
                    feeds[x].push(id);
                    nodes[id].inputs += 1;
                }
            }
        }
    }
    for x in 0..n {
        let mut read_by: Vec<usize> = Vec::new();
        for &id in &feeds[x] {
            // x is an output of each of its own nodes that the reading node is not inside of.
            for &mine in chain[x].iter().skip(1) {
                if !read_by.contains(&mine) && nodes[id].parent != mine && id != mine {
                    read_by.push(mine);
                }
            }
        }
        for mine in read_by {
            nodes[mine].outputs += 1;
        }
    }
    // The run: flips per node, and per node the ticks in which one of its inputs changed.
    let mut engine = vcb::sim::Engine::new(&board, c.clone(), &options)?;
    let mut previous = engine.state().to_vec();
    let mut stamp = vec![0u64; nodes.len()];
    let mut run = 0u64;
    while run < ticks && !engine.breakpoint_hit() {
        engine.step();
        run += 1;
        let state = engine.state();
        for e in 0..n {
            if state[e] != previous[e] && !chain[e].is_empty() {
                for &id in &chain[e] {
                    nodes[id].flips += 1;
                }
                for &id in &feeds[e] {
                    if std::mem::replace(&mut stamp[id], run) != run {
                        nodes[id].events += 1;
                    }
                }
            }
        }
        previous.copy_from_slice(state);
    }
    let total = nodes[0].flips.max(1) as f64;
    println!("{{\"ticks\":{run},\"flips_per_tick\":{:.4},\"nodes\":[", nodes[0].flips as f64 / run as f64);
    for (i, node) in nodes.iter().enumerate() {
        let b = if node.bbox[0] == usize::MAX { [0, 0, 0, 0] } else { node.bbox };
        println!(
            "{{\"id\":{i},\"parent\":{},\"name\":\"{}\",\"kind\":\"{}\",\"entities\":{},\"components\":{},\"latches\":{},\"inputs\":{},\"outputs\":{},\"flips\":{:.5},\"share\":{:.5},\"events\":{:.5},\"box\":[{},{},{},{}]}}{}",
            if i == 0 { -1 } else { node.parent as i64 }, node.name, node.kind, node.entities, node.components, node.latches, node.inputs, node.outputs, node.flips as f64 / run as f64, node.flips as f64 / total, node.events as f64 / run as f64, b[0], b[1], b[2], b[3], if i + 1 == nodes.len() { "" } else { "," }
        );
    }
    println!("]}}");
    Ok(())
}
