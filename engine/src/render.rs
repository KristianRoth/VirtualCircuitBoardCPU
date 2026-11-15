//! PNG snapshots of a board, cropped to the drawn area.

use crate::board::Board;
use crate::format::Result;
use crate::ink::Ink;
use crate::netlist::Components;

const MAX_BYTES: usize = 64 << 20;

/// 3x5 digits, one row per byte, most significant of the low three bits on the left.
const DIGITS: [[u8; 5]; 10] = [
    [7, 5, 5, 5, 7], [2, 6, 2, 2, 7], [7, 1, 7, 4, 7], [7, 1, 7, 1, 7], [5, 5, 7, 1, 1],
    [7, 4, 7, 1, 7], [7, 4, 7, 5, 7], [7, 1, 1, 1, 1], [7, 5, 7, 5, 7], [7, 5, 7, 1, 7],
];

pub struct Image {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

/// The bounding box of the drawn pixels as (x0, y0, x1, y1), end exclusive.
fn bounding_box(board: &Board) -> Result<(usize, usize, usize, usize)> {
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for (i, ink) in board.inks.iter().enumerate() {
        if *ink != Ink::Empty {
            let (x, y) = (i % board.width, i / board.width);
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
        }
    }
    if x1 == 0 {
        return Err("render: the board is empty".into());
    }
    Ok((x0, y0, x1, y1))
}

/// Renders the board. With `state`, an entity that is off has each colour channel halved.
/// `labels` are numbers drawn in black, centred on the given board pixel.
pub fn render(board: &Board, state: Option<(&Components, &[bool])>, labels: &[(usize, usize, usize)]) -> Result<Image> {
    let (x0, y0, x1, y1) = bounding_box(board)?;
    let (bw, bh) = (x1 - x0, y1 - y0);
    let scale = if bw.max(bh) <= 256 { 8 } else { 1 };
    let (width, height) = (bw * scale, bh * scale);
    let bytes = width.checked_mul(height).and_then(|n| n.checked_mul(4)).filter(|n| *n <= MAX_BYTES);
    let mut rgba = vec![0u8; bytes.ok_or(format!("render: {width}x{height} pixels is over the 64 MiB limit"))?];

    for y in 0..bh {
        for x in 0..bw {
            let i = (y0 + y) * board.width + x0 + x;
            let mut px = board.inks[i].to_rgba();
            if let Some((components, on)) = state {
                let e = components.entity_of[i];
                if e != crate::netlist::NONE && !on[e as usize] {
                    px = [px[0] / 2, px[1] / 2, px[2] / 2, px[3]];
                }
            }
            for sy in 0..scale {
                let row = ((y * scale + sy) * width + x * scale) * 4;
                for sx in 0..scale {
                    rgba[row + sx * 4..][..4].copy_from_slice(&px);
                }
            }
        }
    }

    // Labels: font pixels are scale/4 image pixels, so a two-digit number fits a 3x3 block at scale 8.
    let dot = (scale / 4).max(1);
    for &(number, x, y) in labels {
        let text = number.to_string();
        let text_width = (text.len() * 4 - 1) * dot;
        let cx = (x - x0) * scale + scale / 2;
        let cy = (y - y0) * scale + scale / 2;
        for (n, ch) in text.bytes().enumerate() {
            for (row, bits) in DIGITS[(ch - b'0') as usize].iter().enumerate() {
                for col in 0..3 {
                    if bits & (4 >> col) == 0 {
                        continue;
                    }
                    for d in 0..dot * dot {
                        let px = (cx + (n * 4 + col) * dot + d % dot).wrapping_sub(text_width / 2);
                        let py = (cy + row * dot + d / dot).wrapping_sub(5 * dot / 2);
                        if px < width && py < height {
                            rgba[(py * width + px) * 4..][..4].copy_from_slice(&[0, 0, 0, 0xff]);
                        }
                    }
                }
            }
        }
    }
    Ok(Image { width, height, rgba })
}

impl Image {
    pub fn to_png(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, self.width as u32, self.height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("writing a PNG header to memory cannot fail");
        writer.write_image_data(&self.rgba).expect("the buffer matches the header");
        writer.finish().expect("writing a PNG to memory cannot fail");
        out
    }
}
