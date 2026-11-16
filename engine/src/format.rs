//! Reading and writing `.vcb` files (file_version 2). Format: specs/vcb-game-facts.md.

use crate::board::{Board, Settings, VMEM_BYTES, VMEM_WORDS};
use crate::ink::Ink;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{Map, Value};
use std::path::Path;

/// Every reader error is one line naming the field or pixel.
pub type Result<T> = std::result::Result<T, String>;

/// No blob may decompress to more than this.
const MAX_BLOB: usize = 64 << 20;
const TEMPLATE: &str = include_str!("template.json");
/// Fields held as typed values in `Board`. Their copies in `Board::json` are nulled, and `to_vcb` writes them back.
const TYPED_FIELDS: [&str; 8] = ["vmem_data", "vmem_is_external", "is_vmem_enabled", "vmem_settings", "clock_interval", "timer_interval", "random_seed", "random_is_time_seed"];

fn null_typed_fields(json: &mut Map<String, Value>) {
    json["layers"][0] = Value::Null;
    for name in TYPED_FIELDS {
        json[name] = Value::Null;
    }
}

/// Decodes a blob: base64 of a zstd frame plus a trailer of `pairs` (tag 2, value) pairs.
/// Returns the decompressed bytes and the trailer values. The last value is the size.
fn decode_blob(text: &str, pairs: usize, what: &str) -> Result<(Vec<u8>, Vec<usize>)> {
    let bytes = STANDARD.decode(text).map_err(|e| format!("{what}: bad base64 ({e})"))?;
    let trailer_len = pairs * 8;
    if bytes.len() < trailer_len {
        return Err(format!("{what}: too short for its trailer"));
    }
    let (frame, trailer) = bytes.split_at(bytes.len() - trailer_len);
    let mut values = Vec::new();
    for pair in trailer.chunks_exact(8) {
        let tag = i32::from_le_bytes(pair[..4].try_into().unwrap());
        let value = i32::from_le_bytes(pair[4..].try_into().unwrap());
        if tag != 2 || value < 0 {
            return Err(format!("{what}: bad trailer ({tag}, {value})"));
        }
        values.push(value as usize);
    }
    let size = *values.last().unwrap();
    if size > MAX_BLOB {
        return Err(format!("{what}: size {size} is over the 64 MiB limit"));
    }
    let raw = if frame.is_empty() && size == 0 {
        Vec::new() // external VMem: trailer only
    } else {
        zstd::bulk::decompress(frame, size).map_err(|e| format!("{what}: bad zstd data ({e})"))?
    };
    if raw.len() != size {
        return Err(format!("{what}: trailer says {size} bytes, data has {}", raw.len()));
    }
    Ok((raw, values))
}

fn encode_blob(raw: &[u8], values: &[usize]) -> String {
    let mut bytes = zstd::bulk::compress(raw, 19).expect("zstd compression cannot fail in memory");
    for v in values {
        bytes.extend(2i32.to_le_bytes());
        bytes.extend((*v as i32).to_le_bytes());
    }
    STANDARD.encode(bytes)
}

fn field<'a>(json: &'a Map<String, Value>, name: &str) -> Result<&'a Value> {
    json.get(name).ok_or_else(|| format!("{name}: missing field"))
}

fn bool_field(json: &Map<String, Value>, name: &str) -> Result<bool> {
    field(json, name)?.as_bool().ok_or_else(|| format!("{name}: expected true or false"))
}

fn int_field(json: &Map<String, Value>, name: &str) -> Result<i64> {
    field(json, name)?.as_i64().ok_or_else(|| format!("{name}: expected a whole number"))
}

fn str_field<'a>(json: &'a Map<String, Value>, name: &str) -> Result<&'a str> {
    field(json, name)?.as_str().ok_or_else(|| format!("{name}: expected text"))
}

fn words_from_bytes(bytes: &[u8]) -> Vec<u32> {
    bytes.chunks_exact(4).map(|w| u32::from_be_bytes(w.try_into().unwrap())).collect()
}

impl Board {
    /// An empty board with the game's default settings and VMem disabled.
    pub fn new(width: usize, height: usize) -> Board {
        let mut json: Map<String, Value> = serde_json::from_str(TEMPLATE).expect("template.json is valid");
        let empty_layer = encode_blob(&vec![0; width * height * 4], &[height, width, width * height * 4]);
        json["layers"] = vec![Value::Null, empty_layer.clone().into(), empty_layer.into()].into();
        null_typed_fields(&mut json);
        Board {
            width,
            height,
            inks: vec![Ink::Empty; width * height],
            vmem: vec![0; VMEM_WORDS],
            settings: Settings {
                vmem_enabled: false,
                vmem_settings: [0; 16],
                clock_interval: 1,
                timer_interval: 500,
                random_seed: 1,
                random_is_time_seed: false,
            },
            json,
        }
    }

    /// Reads a `.vcb` file, and its `.vcbmem` when VMem is external.
    pub fn read(path: &Path) -> Result<Board> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mem_path = path.with_extension("vcbmem");
        Board::parse(&text, || {
            let bytes = std::fs::read(&mem_path).map_err(|e| format!("{}: {e}", mem_path.display()))?;
            if bytes.len() != VMEM_BYTES {
                return Err(format!("{}: is {} bytes, expected {VMEM_BYTES}", mem_path.display(), bytes.len()));
            }
            Ok(bytes)
        })
    }

    /// Parses the text of a `.vcb`. `external_vmem` is called only when `vmem_is_external` is true.
    pub fn parse(text: &str, external_vmem: impl FnOnce() -> Result<Vec<u8>>) -> Result<Board> {
        if text.trim_start().starts_with("[gd_resource") {
            return Err("old .tres format (starts with [gd_resource) is not supported; re-save the project in the current game".into());
        }
        let mut json: Map<String, Value> = serde_json::from_str(text).map_err(|e| format!("not a .vcb JSON file ({e})"))?;

        let file_type = str_field(&json, "file_type")?;
        if file_type != "RF_VCB_PROJECT" {
            return Err(format!("file_type: unknown value \"{file_type}\""));
        }
        let version = int_field(&json, "file_version")?;
        if version != 2 {
            return Err(format!("file_version: unknown value {version}"));
        }
        for device in ["vinput_is_enabled", "vdisplay_is_enabled"] {
            if bool_field(&json, device)? {
                return Err(format!("{device}: is true, but v1 does not simulate that device"));
            }
        }

        // Logic layer. layers[1] and layers[2] are decoration and stay untouched in `json`.
        let layers = field(&json, "layers")?.as_array().filter(|l| l.len() == 3).ok_or("layers: expected 3 layers")?;
        let logic = layers[0].as_str().ok_or("layers[0]: expected text")?;
        let (pixels, trailer) = decode_blob(logic, 3, "layers[0]")?;
        let (height, width) = (trailer[0], trailer[1]);
        if width.checked_mul(height).and_then(|n| n.checked_mul(4)) != Some(pixels.len()) {
            return Err(format!("layers[0]: {width}x{height} does not match {} bytes", pixels.len()));
        }
        let mut inks = Vec::with_capacity(width * height);
        for (i, px) in pixels.chunks_exact(4).enumerate() {
            let px: [u8; 4] = px.try_into().unwrap();
            let ink = Ink::from_rgba(px).ok_or_else(|| {
                format!("layers[0]: unknown colour at {},{} #{:02x}{:02x}{:02x}{:02x}", i % width, i / width, px[0], px[1], px[2], px[3])
            })?;
            inks.push(ink);
        }

        // VMem: inline, or from the .vcbmem next to the project.
        let vmem_bytes = if bool_field(&json, "vmem_is_external")? {
            field(&json, "vmem_data")?;
            external_vmem()?
        } else {
            let (mut bytes, _) = decode_blob(str_field(&json, "vmem_data")?, 1, "vmem_data")?;
            if bytes.len() == VMEM_BYTES + 1 {
                bytes.pop(); // seen in the CPU board (tests/boards/cpu-32bit-manual-start.vcb); the extra byte is ignored
            }
            if bytes.len() != VMEM_BYTES {
                return Err(format!("vmem_data: is {} bytes, expected {VMEM_BYTES}", bytes.len()));
            }
            bytes
        };
        let mut vmem = words_from_bytes(&vmem_bytes);
        if vmem[0] != 0 {
            // The game's editor always shows word 0 as 0 (facts question 8).
            eprintln!("warning: VMem word 0 was {:#010x} in the file; set to 0", vmem[0]);
            vmem[0] = 0;
        }

        let raw_settings = field(&json, "vmem_settings")?.as_array().filter(|a| a.len() == 16).ok_or("vmem_settings: expected 16 numbers")?;
        let mut vmem_settings = [0i64; 16];
        for (slot, v) in vmem_settings.iter_mut().zip(raw_settings) {
            *slot = v.as_i64().ok_or("vmem_settings: expected 16 whole numbers")?;
        }
        let settings = Settings {
            vmem_enabled: bool_field(&json, "is_vmem_enabled")?,
            vmem_settings,
            clock_interval: int_field(&json, "clock_interval")?,
            timer_interval: int_field(&json, "timer_interval")?,
            random_seed: int_field(&json, "random_seed")?,
            random_is_time_seed: bool_field(&json, "random_is_time_seed")?,
        };

        // The typed fields above are the source of truth; drop their copies from `json`.
        null_typed_fields(&mut json);
        Ok(Board { width, height, inks, vmem, settings, json })
    }

    /// The `.vcb` text for this board. VMem is always written inline.
    pub fn to_vcb(&self) -> String {
        let mut json = self.json.clone();
        let pixels: Vec<u8> = self.inks.iter().flat_map(|ink| ink.to_rgba()).collect();
        let layer_trailer = [self.height, self.width, pixels.len()];
        json["layers"][0] = encode_blob(&pixels, &layer_trailer).into();
        let vmem_bytes: Vec<u8> = self.vmem.iter().flat_map(|w| w.to_be_bytes()).collect();
        json["vmem_data"] = encode_blob(&vmem_bytes, &[vmem_bytes.len()]).into();
        json["vmem_is_external"] = false.into();

        let s = &self.settings;
        json["is_vmem_enabled"] = s.vmem_enabled.into();
        json["vmem_settings"] = s.vmem_settings.to_vec().into();
        json["clock_interval"] = s.clock_interval.into();
        json["timer_interval"] = s.timer_interval.into();
        json["random_seed"] = s.random_seed.into();
        json["random_is_time_seed"] = s.random_is_time_seed.into();
        serde_json::to_string_pretty(&json).expect("a JSON map always serialises")
    }
}
