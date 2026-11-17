//! The assembler: the game's assembly language (User Guide chapter 8) to VMem words.
//! Spec: ../specs/vcb-assembler.md. Open questions about the game: question 23 in ../specs/vcb-game-facts.md.
//!
//! The gate: a form the game has been seen to assemble is accepted, and one it has been seen to reject is
//! refused ("the game rejects this"). A feature the manual states but no save shows is "unverified": a gate
//! hit unless `allow_unverified`. Anything else is "undefined": always a gate hit, named by its question
//! letter. Gate hits and rejections do not stop the assembler, so one run lists them all; a statement that
//! cannot be parsed at all stops it at once.
//!
//! Values are 32 bits wide whatever the board's content bits are, and any address from 1 to the end of VMem
//! can be stored to whatever the address bits are (both observed). Arithmetic is signed, with C's precedence.

use crate::board::{Board, VMEM_WORDS};
use std::collections::{HashMap, HashSet};

/// Unverified features that a game save has since shown. Add the name here in the commit that adds the fixture.
/// The probe boards `asm-probe-32`, `asm-probe-16`, `asm-reject` and `asm-probe2` (2026-11-17) showed every
/// one, so no feature is unverified today. The gate stays for the next game update, when each must be shown again.
pub const VERIFIED: &[&str] = &[
    "first address is 1",
    "comment after a statement",
    "underscores in decimal and hex literals",
    "negative literal",
    "more than two primitives in an instruction",
    "resymb",
    "unsymb",
    "pointer inline",
    "unpoint",
    "origin orgprev",
    "origin orgbase",
    "macro argument that is a literal",
    "macro without exactly two placeholders",
    "macro braces without spaces",
    "remac",
    "unmac",
    "bookmark",
    "sub_bookmark",
];

/// A stored value lies in this range; a negative one is stored in two's complement.
const VALUE_RANGE: std::ops::RangeInclusive<i64> = -(1 << 31)..=u32::MAX as i64;

const MAX_SOURCE_BYTES: usize = 16 << 20;
const MAX_NESTING: usize = 64;
const KEYWORDS: [&str; 15] = [
    "symbol", "resymb", "unsymb", "pointer", "repoint", "unpoint", "inline", "origin", "orgprev", "orgbase", "macro", "remac", "unmac",
    "bookmark", "sub_bookmark",
];
const OPERATORS: [&str; 11] = ["+", "-", "*", "/", "%", "~", "&", "|", "^", "<<", ">>"];

#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    pub address: usize,
    pub value: u32,
    pub line: usize,
    /// The statement as written (a macro use is not expanded).
    pub text: String,
}

#[derive(Debug, Default, PartialEq)]
pub struct Report {
    /// Stored words in source order.
    pub words: Vec<Word>,
    /// Labels and pointers with their addresses, sorted by address.
    pub names: Vec<(String, usize)>,
    /// Unverified features the program used (only with `allow_unverified`).
    pub unverified: Vec<&'static str>,
    /// Words that replaced a different non-zero word: the board already held another program there.
    pub replaced: usize,
}

impl Report {
    /// The line `vcb` prints after assembling.
    pub fn summary(&self, source_name: &str) -> String {
        let mut line = format!("assembled {} words from {source_name}", self.words.len());
        if self.replaced > 0 {
            line += &format!(", {} replaced a different non-zero word", self.replaced);
        }
        if !self.unverified.is_empty() {
            let names: Vec<String> = self.unverified.iter().map(|f| format!("asm {f}")).collect();
            line += &format!(" (unverified: {})", names.join(", "));
        }
        line
    }

    /// The text of `vcb asm --listing`.
    pub fn listing(&self) -> String {
        let mut words: Vec<&Word> = self.words.iter().collect();
        words.sort_by_key(|w| w.address);
        let mut out = String::new();
        for w in words {
            out += &format!("{:#07X} {:#010X}  {}  {}\n", w.address, w.value, w.line, escape(&w.text));
        }
        out.push('\n');
        for (name, address) in &self.names {
            out += &format!("{address:#07X}  {name}\n");
        }
        out
    }
}

/// Source text shown to the user never carries control characters.
fn escape(text: &str) -> String {
    text.chars().flat_map(|c| if c.is_control() && c != '\t' { c.escape_default().collect::<Vec<_>>() } else { vec![c] }).collect()
}

#[derive(Clone)]
struct Tok {
    s: String,
    col: usize,
}

struct Stmt {
    toks: Vec<Tok>,
    line: usize,
    text: String,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Symbol(i64),
    Pointer(i64),
    Macro,
}

enum Ast {
    /// A literal, symbol or pointer value.
    Val(i64),
    Label(String),
    Bin(&'static str, Box<Ast>, Box<Ast>),
    Paren(Box<Ast>),
    Not(Box<Ast>),
    Neg(Box<Ast>),
    /// An instruction without operators: its primitives are ORed.
    Prims(Vec<Ast>),
    /// A gate hit made this statement's value meaningless.
    Unknown,
}

struct Emit {
    address: i64,
    ast: Ast,
    stmt: usize,
    col: usize,
    /// The macro this instruction is a use of.
    in_macro: Option<String>,
}

struct Asm<'a> {
    source_name: &'a str,
    allow: bool,
    hits: Vec<String>,
    used: Vec<&'static str>,
    names: HashMap<String, Kind>,
    macros: HashMap<String, Vec<Tok>>,
    labels: HashMap<String, Option<i64>>,
    pointers: Vec<(String, i64)>,
}

type Fatal = String;

impl Asm<'_> {
    fn at(&self, line: usize, col: usize) -> String {
        format!("{}:{line}:{col}", self.source_name)
    }

    fn unverified(&mut self, feature: &'static str, line: usize, col: usize) {
        if VERIFIED.contains(&feature) {
        } else if !self.allow {
            let hit = format!("{}: '{feature}' is not verified against the game; pass --allow-unverified", self.at(line, col));
            self.hits.push(hit);
        } else if !self.used.contains(&feature) {
            self.used.push(feature);
        }
    }

    /// `letter` is the letter of facts question 23, or "" for a case that has none yet.
    fn undefined(&mut self, letter: &str, what: &str, line: usize, col: usize) {
        let why = if letter.is_empty() { "no observed rule".to_string() } else { format!("the game's result is not known (facts question 23{letter})") };
        let hit = format!("{}: {what}: {why}", self.at(line, col));
        self.hits.push(hit);
    }

    /// The game was seen to refuse this with an error in its editor.
    fn rejected(&mut self, what: &str, line: usize, col: usize) {
        let hit = format!("{}: {what}: the game rejects this", self.at(line, col));
        self.hits.push(hit);
    }

    /// A numeric literal: decimal, 0x or 0b, underscores allowed, an optional sign. `None` after a gate hit.
    fn literal(&mut self, tok: &Tok, line: usize) -> Result<Option<i64>, Fatal> {
        let s = tok.s.as_str();
        let bad = format!("{}: '{}' is not a number", self.at(line, tok.col), escape(s));
        let unsigned = s.strip_prefix(['-', '+']).unwrap_or(s);
        if unsigned.starts_with("0X") || unsigned.starts_with("0B") {
            self.rejected("an upper-case 0X or 0B prefix", line, tok.col);
            return Ok(None);
        }
        let (digits, radix) = match (unsigned.strip_prefix("0x"), unsigned.strip_prefix("0b")) {
            (Some(hex), _) => (hex, 16),
            (_, Some(bin)) => (bin, 2),
            _ => (unsigned, 10),
        };
        if radix != 2 && digits.contains('_') {
            self.unverified("underscores in decimal and hex literals", line, tok.col);
        }
        let v = u128::from_str_radix(&digits.replace('_', ""), radix).map_err(|_| bad)?;
        if v > u32::MAX as u128 {
            self.rejected("a value wider than 32 bits", line, tok.col);
            return Ok(None);
        }
        if s.starts_with('-') {
            self.unverified("negative literal", line, tok.col);
            if v > 1 << 31 {
                self.rejected("a value below -2^31", line, tok.col);
                return Ok(None);
            }
            return Ok(Some(-(v as i64)));
        }
        Ok(Some(v as i64))
    }

    /// An address in a directive: a literal from 1 to the last VMem word (the game rejects anything else).
    fn address(&mut self, tok: &Tok, line: usize) -> Result<Option<i64>, Fatal> {
        if !is_literal_like(&tok.s) {
            self.rejected("an address that is not a number", line, tok.col);
            return Ok(None);
        }
        let address = self.literal(tok, line)?;
        if address.is_some_and(|a| !(1..VMEM_WORDS as i64).contains(&a)) {
            self.rejected("an address outside VMem (1 to 1,048,575)", line, tok.col);
            return Ok(None);
        }
        Ok(address)
    }

    /// Checks a name being defined. Returns false when the definition should not go ahead.
    fn check_new_name(&mut self, tok: &Tok, line: usize) -> bool {
        let name = tok.s.as_str();
        if !is_name(name) {
            self.rejected("a name with characters other than letters, digits and underscore, or starting with a digit", line, tok.col);
            return false;
        }
        if KEYWORDS.contains(&name) {
            self.rejected("a keyword used as a name", line, tok.col);
            return false;
        }
        let existing = match self.names.get(name) {
            Some(Kind::Symbol(_)) => Some("symbol"),
            Some(Kind::Pointer(_)) => Some("pointer"),
            Some(Kind::Macro) => Some("macro"),
            None if self.labels.contains_key(name) => Some("label"),
            None => None,
        };
        match existing {
            Some(k) => self.rejected(&format!("the name '{name}' is already defined as a {k}"), line, tok.col),
            None => {}
        }
        true
    }

    /// The value of a name used in an instruction.
    fn primitive(&mut self, tok: &Tok, line: usize) -> Result<Ast, Fatal> {
        let name = tok.s.as_str();
        let glued_operator = name.len() > 1 && OPERATORS.iter().any(|op| name[1..].contains(op));
        if is_literal_like(name) && !glued_operator {
            return Ok(self.literal(tok, line)?.map_or(Ast::Unknown, |v| Ast::Val(v)));
        }
        if !is_name(name) {
            self.rejected(&format!("'{}' is not a name or a number (operators need spaces around them)", escape(name)), line, tok.col);
            return Ok(Ast::Unknown);
        }
        match self.names.get(name) {
            Some(Kind::Symbol(v)) | Some(Kind::Pointer(v)) => return Ok(Ast::Val(*v)),
            Some(Kind::Macro) => self.rejected("a macro name inside a macro body", line, tok.col),
            None if self.labels.contains_key(name) => return Ok(Ast::Label(name.to_string())),
            None if KEYWORDS.contains(&name) => self.rejected("a keyword inside an instruction", line, tok.col),
            None => self.rejected(&format!("the name '{name}' must be defined before it is used"), line, tok.col),
        }
        Ok(Ast::Unknown)
    }

    /// One level of an expression: operand { operator operand }, up to `)` or the end.
    /// Precedence is C's and chains run left to right (the game showed every pair of operators).
    fn expression(&mut self, toks: &[Tok], i: &mut usize, line: usize, depth: usize) -> Result<Ast, Fatal> {
        if depth > MAX_NESTING {
            return Err(format!("{}: parentheses nested more than {MAX_NESTING} deep", self.at(line, toks[*i - 1].col)));
        }
        let mut operands = vec![self.operand(toks, i, line, depth)?];
        let mut ops: Vec<&'static str> = Vec::new();
        let mut usable = true;
        while *i < toks.len() && toks[*i].s != ")" {
            let tok = &toks[*i];
            match OPERATORS.iter().find(|op| **op == tok.s) {
                Some(op) if *op != "~" => {
                    ops.push(op);
                    *i += 1;
                }
                _ => {
                    self.rejected("primitives side by side inside an expression", line, tok.col);
                    ops.push("|");
                    usable = false;
                }
            }
            operands.push(self.operand(toks, i, line, depth)?);
        }
        if !usable {
            return Ok(Ast::Unknown);
        }
        // Fold the tightest operators first, each level left to right.
        for level in [&["*", "/", "%"][..], &["+", "-"], &["<<", ">>"], &["&"], &["^"], &["|"]] {
            let mut k = 0;
            while k < ops.len() {
                if !level.contains(&ops[k]) {
                    k += 1;
                    continue;
                }
                let (op, right) = (ops.remove(k), operands.remove(k + 1));
                let left = std::mem::replace(&mut operands[k], Ast::Unknown);
                operands[k] = Ast::Bin(op, Box::new(left), Box::new(right));
            }
        }
        Ok(operands.remove(0))
    }

    fn operand(&mut self, toks: &[Tok], i: &mut usize, line: usize, depth: usize) -> Result<Ast, Fatal> {
        let Some(tok) = toks.get(*i) else {
            let col = toks.last().map_or(1, |t| t.col + t.s.len());
            return Err(format!("{}: an operand is missing", self.at(line, col)));
        };
        *i += 1;
        match tok.s.as_str() {
            "(" => {
                let inner = self.expression(toks, i, line, depth + 1)?;
                if toks.get(*i).map(|t| t.s.as_str()) != Some(")") {
                    return Err(format!("{}: this parenthesis is never closed", self.at(line, tok.col)));
                }
                *i += 1;
                Ok(Ast::Paren(Box::new(inner)))
            }
            ")" => Err(format!("{}: an operand is missing before ')'", self.at(line, tok.col))),
            // `~ x` and `- x` bind tighter than any binary operator (observed). `~ ~ 5` gave -5 in the game,
            // which no reading explains, so one sign directly after another is question 23u.
            sign @ ("~" | "-") if *i < toks.len() => {
                if matches!(toks[*i].s.as_str(), "~" | "-") {
                    self.undefined("u", "two signs (~ or -) in a row", line, tok.col);
                    self.operand(toks, i, line, depth)?;
                    return Ok(Ast::Unknown);
                }
                let inner = Box::new(self.operand(toks, i, line, depth)?);
                Ok(if sign == "~" { Ast::Not(inner) } else { Ast::Neg(inner) })
            }
            op if OPERATORS.contains(&op) => Err(format!("{}: an operand is missing before '{op}'", self.at(line, tok.col))),
            _ => self.primitive(tok, line),
        }
    }

    /// An instruction's tokens (after macro expansion) as an expression tree.
    fn instruction(&mut self, toks: &[Tok], line: usize) -> Result<Ast, Fatal> {
        if !toks.iter().any(|t| t.s == "(" || t.s == ")" || OPERATORS.contains(&t.s.as_str())) {
            let mut prims = Vec::new();
            for tok in toks {
                prims.push(self.primitive(tok, line)?);
            }
            if prims.len() > 2 {
                self.unverified("more than two primitives in an instruction", line, toks[2].col);
            }
            return Ok(Ast::Prims(prims));
        }
        let mut i = 0;
        let ast = self.expression(toks, &mut i, line, 0)?;
        if i < toks.len() {
            return Err(format!("{}: ')' without an opening parenthesis", self.at(line, toks[i].col)));
        }
        Ok(ast)
    }

    /// The value of a tree, once every label has its address. `None` after a gate hit.
    /// Arithmetic is signed (`-7 / 2` is -3, `-7 % 3` is -1, `-8 >> 1` is -4). The game rejects a result
    /// outside `VALUE_RANGE`; whether it also checks the steps in between is question 23b, so a step outside
    /// the range is refused under that letter.
    fn eval(&mut self, ast: &Ast, top: bool, line: usize, col: usize) -> Option<i64> {
        let value: i128 = match ast {
            Ast::Unknown => return None,
            Ast::Val(v) => return Some(*v),
            Ast::Label(name) => return self.labels[name],
            Ast::Paren(inner) => return self.eval(inner, top, line, col),
            Ast::Not(inner) => !self.eval(inner, false, line, col)? as i128,
            Ast::Neg(inner) => -(self.eval(inner, false, line, col)? as i128),
            Ast::Prims(prims) if prims.len() == 1 => return self.eval(&prims[0], top, line, col),
            Ast::Prims(prims) => {
                let mut value = 0;
                for p in prims {
                    value |= self.eval(p, false, line, col)?;
                }
                value as i128
            }
            Ast::Bin(op, left, right) => {
                let (a, b) = (self.eval(left, false, line, col), self.eval(right, false, line, col));
                let (a, b) = (a? as i128, b? as i128);
                match *op {
                    "|" => a | b,
                    "&" => a & b,
                    "^" => a ^ b,
                    "+" => a + b,
                    "-" => a - b,
                    "*" => a * b,
                    "/" | "%" if b == 0 => {
                        self.rejected("division or modulo by zero", line, col);
                        return None;
                    }
                    "/" => a / b,
                    "%" => a % b,
                    _ if b < 0 => {
                        self.rejected("a shift by a negative count", line, col);
                        return None;
                    }
                    ">>" if b < 64 => a >> b,
                    "<<" if b < 64 => a << b,
                    "<<" if a != 0 => {
                        self.rejected("a value wider than 32 bits", line, col);
                        return None;
                    }
                    _ => {
                        self.undefined("c", "a shift by 64 or more", line, col);
                        return None;
                    }
                }
            }
        };
        if value >= *VALUE_RANGE.start() as i128 && value <= *VALUE_RANGE.end() as i128 {
            return Some(value as i64);
        }
        if top {
            self.rejected("a value outside 32 bits (-2^31 to 2^32-1)", line, col);
        } else {
            self.undefined("b", "a value outside 32 bits in the middle of an expression", line, col);
        }
        None
    }
}

fn is_name(s: &str) -> bool {
    !s.is_empty() && !s.starts_with(|c: char| c.is_ascii_digit()) && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_literal_like(s: &str) -> bool {
    let digits = s.strip_prefix(['-', '+']).unwrap_or(s);
    digits.starts_with(|c: char| c.is_ascii_digit())
}

fn is_placeholder(s: &str) -> bool {
    s.len() == 2 && s.ends_with('?') && s.starts_with(|c: char| c.is_ascii_lowercase())
}

/// Splits the source into statements. Comments are removed first; a `;` ends a statement like a newline.
fn statements(asm: &mut Asm, source: &str) -> Vec<Stmt> {
    let mut out = Vec::new();
    for (n, full_line) in source.split('\n').enumerate() {
        let line = n + 1;
        let code = match full_line.find('#') {
            Some(at) => {
                if !full_line[..at].trim().is_empty() {
                    asm.unverified("comment after a statement", line, at + 1);
                }
                &full_line[..at]
            }
            None => full_line,
        };
        let first = code.split_whitespace().next().unwrap_or("");
        if code.contains(';') && matches!(first, "macro" | "remac") {
            asm.rejected("a ; on the line of a macro definition", line, code.find(';').unwrap() + 1);
        }
        let mut start = 0;
        for piece in code.split(';') {
            let mut toks: Vec<Tok> = Vec::new();
            let mut word_start = None;
            let mut glued_brace = false;
            for (i, c) in piece.char_indices().chain([(piece.len(), ' ')]) {
                let bracket = matches!(c, '(' | ')' | '{' | '}');
                if c.is_whitespace() || bracket {
                    if let Some(s) = word_start.take() {
                        toks.push(Tok { s: piece[s..i].to_string(), col: start + s + 1 });
                        glued_brace |= c == '}';
                    }
                    if bracket {
                        let next = piece[i + 1..].chars().next();
                        glued_brace |= c == '{' && next.is_some_and(|n| !n.is_whitespace());
                        toks.push(Tok { s: c.to_string(), col: start + i + 1 });
                    }
                } else if word_start.is_none() {
                    word_start = Some(i);
                }
            }
            if glued_brace {
                asm.unverified("macro braces without spaces", line, toks[0].col);
            }
            if !toks.is_empty() {
                out.push(Stmt { toks, line, text: piece.trim().to_string() });
            }
            start += piece.len() + 1;
        }
    }
    out
}

/// Assembles `source` into `board.vmem`, as the game does when a simulation starts.
/// `source_name` is used in messages. On an error the board is unchanged, and the message has one line per
/// gate hit followed by the real error, if any.
pub fn assemble(source: &str, source_name: &str, board: &mut Board, allow_unverified: bool) -> Result<Report, String> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(format!("{source_name}: the source is over 16 MiB"));
    }
    if !board.settings.vmem_enabled {
        return Err(format!("{source_name}: is_vmem_enabled is false, so there is no VMem to assemble into"));
    }
    let mut asm = Asm {
        source_name,
        allow: allow_unverified,
        hits: Vec::new(),
        used: Vec::new(),
        names: HashMap::new(),
        macros: HashMap::new(),
        labels: HashMap::new(),
        pointers: Vec::new(),
    };
    match walk(&mut asm, source) {
        Ok(words) if asm.hits.is_empty() => {
            let mut report = Report { unverified: asm.used, ..Default::default() };
            // When two statements store to one address, the later one wins (observed).
            let last: HashMap<usize, u32> = words.iter().map(|w| (w.address, w.value)).collect();
            for (address, value) in last {
                let old = std::mem::replace(&mut board.vmem[address], value);
                report.replaced += (old != 0 && old != value) as usize;
            }
            let labels = asm.labels.into_iter().filter_map(|(name, a)| Some((name, a? as usize)));
            report.names = labels.chain(asm.pointers.into_iter().map(|(name, a)| (name, a as usize))).collect();
            report.names.sort_by(|a, b| (a.1, &a.0).cmp(&(b.1, &b.0)));
            report.words = words;
            Ok(report)
        }
        Ok(_) => Err(asm.hits.join("\n")),
        Err(fatal) => {
            asm.hits.push(fatal);
            Err(asm.hits.join("\n"))
        }
    }
}

fn walk(asm: &mut Asm, source: &str) -> Result<Vec<Word>, Fatal> {
    for (n, line) in source.split('\n').enumerate() {
        if let Some(at) = line.find(|c: char| c.is_control() && c != '\t') {
            if line[at..].starts_with('\r') {
                asm.rejected("a carriage return in the program text", n + 1, at + 1);
            } else {
                return Err(format!("{}: a control character in the source", asm.at(n + 1, at + 1)));
            }
        }
    }
    let stmts = statements(asm, source);

    // Labels are global, so their names are needed before the first use.
    for st in &stmts {
        if st.toks[0].s == "@" {
            if let Some(name) = st.toks.get(1) {
                asm.labels.entry(name.s.clone()).or_insert(None);
            }
        }
    }

    let limit = VMEM_WORDS as i64;
    let mut emits: Vec<Emit> = Vec::new();
    let mut seen_labels: HashSet<String> = HashSet::new();
    // A label names the next instruction, also across `origin` and pointers (observed).
    let mut pending_labels: Vec<(String, usize, usize)> = Vec::new();
    let mut cur: i64 = 1;
    let mut origin_seen = false;
    // One entry per `origin <address>`: where the assembler was when it met the directive. `orgprev` goes
    // back to the last entry, `orgbase` to the first (observed, also with words stored in between).
    let mut origins: Vec<i64> = Vec::new();
    let mut skip_to_brace = false;

    for (index, st) in stmts.iter().enumerate() {
        let (line, toks) = (st.line, &st.toks);
        let (head, col) = (toks[0].s.as_str(), toks[0].col);
        if skip_to_brace {
            skip_to_brace = !toks.iter().any(|t| t.s == "}");
            continue;
        }
        let source_name = asm.source_name;
        let args = |n: usize| -> Result<(), Fatal> {
            if toks.len() == n + 1 { Ok(()) } else { Err(format!("{}:{line}:{col}: '{head}' takes {n} argument{}", source_name, if n == 1 { "" } else { "s" })) }
        };
        // Storing a word at the current address: an instruction or an inline pointer.
        let place = |asm: &mut Asm, cur: &mut i64| -> i64 {
            if !origin_seen {
                asm.unverified("first address is 1", line, col);
            }
            if *cur >= limit {
                // The game accepted a word after the last one; where it went could not be seen.
                asm.undefined("n", "a word stored past the last VMem word", line, col);
            }
            *cur += 1;
            *cur - 1
        };
        match head {
            "symbol" | "resymb" => {
                args(2)?;
                // The value is a literal, or the name of a symbol, a pointer or a label defined above (observed).
                let value = match asm.names.get(&toks[2].s) {
                    Some(Kind::Symbol(v)) | Some(Kind::Pointer(v)) => Some(*v),
                    _ if is_literal_like(&toks[2].s) => asm.literal(&toks[2], line)?,
                    _ if seen_labels.contains(&toks[2].s) && !pending_labels.iter().any(|(name, _, _)| *name == toks[2].s) => asm.labels[&toks[2].s],
                    _ => {
                        asm.undefined("p", "a symbol value that is not a literal or a name defined above", line, toks[2].col);
                        None
                    }
                };
                if head == "symbol" {
                    if !asm.check_new_name(&toks[1], line) {
                        continue;
                    }
                } else {
                    asm.unverified("resymb", line, col);
                    if !matches!(asm.names.get(&toks[1].s), Some(Kind::Symbol(_))) {
                        asm.rejected("resymb of a name that is not a symbol", line, toks[1].col);
                    }
                }
                asm.names.insert(toks[1].s.clone(), Kind::Symbol(value.unwrap_or(0)));
            }
            "unsymb" | "unpoint" | "unmac" => {
                args(1)?;
                let (feature, is_kind): (&'static str, fn(&Kind) -> bool) = match head {
                    "unsymb" => ("unsymb", |k| matches!(k, Kind::Symbol(_))),
                    "unpoint" => ("unpoint", |k| matches!(k, Kind::Pointer(_))),
                    _ => ("unmac", |k| matches!(k, Kind::Macro)),
                };
                asm.unverified(feature, line, col);
                if asm.names.get(&toks[1].s).is_some_and(is_kind) {
                    asm.names.remove(&toks[1].s);
                } else {
                    asm.rejected(&format!("{head} of a name that is not defined as that kind"), line, toks[1].col);
                }
            }
            // `pointer <name> <address>|inline <value>` stores the value and names its address.
            // `repoint` does the same for an existing pointer; the old word stays (observed).
            "pointer" | "repoint" => {
                args(3)?;
                // The value is a literal or the name of a symbol (observed).
                let value = match asm.names.get(&toks[3].s) {
                    Some(Kind::Symbol(v)) => Some(*v),
                    _ if is_literal_like(&toks[3].s) => asm.literal(&toks[3], line)?,
                    _ => {
                        asm.undefined("p", "a pointer value that is not a literal or a symbol", line, toks[3].col);
                        None
                    }
                };
                let address = if toks[2].s == "inline" {
                    asm.unverified("pointer inline", line, toks[2].col);
                    Some(place(asm, &mut cur))
                } else {
                    asm.address(&toks[2], line)?
                };
                let Some(address) = address else { continue };
                if head == "repoint" {
                    if !matches!(asm.names.get(&toks[1].s), Some(Kind::Pointer(_))) {
                        asm.rejected("repoint of a name that is not a pointer", line, toks[1].col);
                        continue;
                    }
                    asm.pointers.retain(|(name, _)| *name != toks[1].s);
                } else if !asm.check_new_name(&toks[1], line) {
                    continue;
                }
                asm.names.insert(toks[1].s.clone(), Kind::Pointer(address));
                asm.pointers.push((toks[1].s.clone(), address));
                emits.push(Emit { address, ast: value.map_or(Ast::Unknown, Ast::Val), stmt: index, col, in_macro: None });
            }
            "origin" => {
                args(1)?;
                match toks[1].s.as_str() {
                    back @ ("orgprev" | "orgbase") => {
                        asm.unverified(if back == "orgprev" { "origin orgprev" } else { "origin orgbase" }, line, toks[1].col);
                        match if back == "orgprev" { origins.pop() } else { origins.drain(..).next() } {
                            None => asm.rejected("orgprev or orgbase with no origin to revert", line, toks[1].col),
                            Some(address) => cur = address,
                        }
                    }
                    _ => {
                        let Some(address) = asm.address(&toks[1], line)? else { continue };
                        origins.push(cur);
                        cur = address;
                    }
                }
                origin_seen = true;
            }
            "@" => {
                if toks.len() < 2 {
                    return Err(format!("{}: '@' needs a label name", asm.at(line, col)));
                }
                if toks.len() > 2 {
                    asm.rejected("a label and an instruction in one statement", line, toks[2].col);
                }
                let name = &toks[1];
                if !is_name(&name.s) {
                    asm.rejected("a name with characters other than letters, digits and underscore, or starting with a digit", line, name.col);
                } else if KEYWORDS.contains(&name.s.as_str()) {
                    asm.rejected("a keyword used as a name", line, name.col);
                } else if asm.names.contains_key(&name.s) {
                    asm.rejected(&format!("the name '{}' is already defined", name.s), line, name.col);
                } else if !seen_labels.insert(name.s.clone()) {
                    asm.rejected(&format!("the name '{}' is already defined as a label", name.s), line, name.col);
                }
                pending_labels.push((name.s.clone(), line, col));
            }
            "macro" | "remac" => {
                if toks.len() < 3 || toks[2].s != "{" {
                    return Err(format!("{}: expected '{head} <name> {{ <body> }}'", asm.at(line, col)));
                }
                if toks.last().unwrap().s != "}" {
                    asm.rejected("a macro body over several lines", line, col);
                    skip_to_brace = true;
                    continue;
                }
                let body = toks[3..toks.len() - 1].to_vec();
                let mut letters: Vec<u8> = body.iter().filter(|t| is_placeholder(&t.s)).map(|t| t.s.as_bytes()[0] - b'a').collect();
                letters.sort();
                letters.dedup();
                if letters.iter().enumerate().any(|(i, l)| *l as usize != i) {
                    asm.rejected("macro placeholders that skip a letter", line, col);
                }
                if letters.len() != 2 {
                    asm.unverified("macro without exactly two placeholders", line, col);
                }
                if head == "macro" {
                    if !asm.check_new_name(&toks[1], line) {
                        continue;
                    }
                } else {
                    asm.unverified("remac", line, col);
                    if !matches!(asm.names.get(&toks[1].s), Some(Kind::Macro)) {
                        asm.undefined("", "remac of a name that is not a macro", line, toks[1].col);
                    }
                }
                asm.names.insert(toks[1].s.clone(), Kind::Macro);
                asm.macros.insert(toks[1].s.clone(), body);
            }
            "bookmark" => asm.unverified("bookmark", line, col),
            "sub_bookmark" => asm.unverified("sub_bookmark", line, col),
            _ => {
                // An instruction: one word at the current address. A macro name takes the tokens after it as
                // its arguments and is replaced by its body, wherever it stands (`pair one two | four`, observed).
                let mut expanded: Vec<Tok> = Vec::new();
                let (mut in_macro, mut usable, mut k) = (None, true, 0);
                while k < toks.len() {
                    let tok = &toks[k];
                    k += 1;
                    let Some(body) = asm.macros.get(&tok.s).filter(|_| asm.names.get(&tok.s) == Some(&Kind::Macro)).cloned() else {
                        expanded.push(tok.clone());
                        continue;
                    };
                    in_macro = in_macro.or(Some(tok.s.clone()));
                    let placeholders = body.iter().filter(|t| is_placeholder(&t.s)).map(|t| t.s.as_bytes()[0]).collect::<HashSet<_>>().len();
                    let call = &toks[k..(k + placeholders).min(toks.len())];
                    k += call.len();
                    if call.len() != placeholders {
                        asm.rejected(&format!("the macro '{}' takes {placeholders} arguments, not {}", tok.s, call.len()), line, tok.col);
                        usable = false;
                    }
                    for arg in call {
                        if is_literal_like(&arg.s) {
                            asm.unverified("macro argument that is a literal", line, arg.col);
                        } else if !is_name(&arg.s) {
                            asm.rejected("a macro argument that is not a name or a number", line, arg.col);
                            usable = false;
                        }
                    }
                    if body.is_empty() {
                        asm.undefined("", "the use of a macro with an empty body", line, tok.col);
                        usable = false;
                    }
                    expanded.extend(body.iter().map(|t| if is_placeholder(&t.s) { call.get((t.s.as_bytes()[0] - b'a') as usize).unwrap_or(t).clone() } else { Tok { s: t.s.clone(), col: tok.col } }));
                }
                let hits_before = asm.hits.len();
                let ast = match usable {
                    true => asm.instruction(&expanded, line).map_err(|e| match &in_macro {
                        Some(name) => format!("{e} (in macro {name})"),
                        None => e,
                    })?,
                    false => Ast::Unknown,
                };
                if let Some(name) = &in_macro {
                    for hit in &mut asm.hits[hits_before..] {
                        *hit += &format!(" (in macro {name})");
                    }
                }
                let address = place(asm, &mut cur);
                for (name, _, _) in pending_labels.drain(..) {
                    asm.labels.insert(name, Some(address));
                }
                emits.push(Emit { address, ast, stmt: index, col, in_macro });
            }
        }
    }
    if skip_to_brace {
        return Err(format!("{}: a macro body is never closed with '}}'", asm.source_name));
    }
    for (name, line, col) in pending_labels {
        asm.rejected(&format!("no instruction follows the label '{name}'"), line, col);
    }

    let mut words: Vec<Word> = Vec::new();
    for emit in &emits {
        let st = &stmts[emit.stmt];
        let hits_before = asm.hits.len();
        let value = asm.eval(&emit.ast, true, st.line, emit.col);
        if let Some(name) = &emit.in_macro {
            for hit in &mut asm.hits[hits_before..] {
                *hit += &format!(" (in macro {name})");
            }
        }
        if let Some(value) = value {
            if (1..limit).contains(&emit.address) {
                // A negative result is stored in two's complement.
                words.push(Word { address: emit.address as usize, value: value as u32, line: st.line, text: st.text.clone() });
            }
        }
    }
    Ok(words)
}
