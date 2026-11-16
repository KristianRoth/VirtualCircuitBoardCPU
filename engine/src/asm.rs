//! The assembler: the game's assembly language (User Guide chapter 8) to VMem words.
//! Spec: ../specs/vcb-assembler.md. Open questions about the game: question 23 in ../specs/vcb-game-facts.md.
//!
//! The gate has three classes. A shape the game has been seen to assemble is accepted. A feature the manual
//! states but no save shows is "unverified": a gate hit unless `allow_unverified`. Anything else is
//! "undefined": always a gate hit, named by its question letter. Gate hits do not stop the assembler, so one
//! run lists them all; a real error (bad syntax) stops it at once.

use crate::board::Board;
use std::collections::{HashMap, HashSet};

/// Unverified features that a game save has since shown. Add the name here in the commit that adds the fixture.
pub const VERIFIED: &[&str] = &[];

/// `@ name;` and an indented label occur only in day 1 part 2. Set to true when the game's assembled save of
/// part 2 (`cpu-aoc1p2.assembled.vcb`, milestone A0) is a fixture; until then they are question 23q.
const PART2_LABEL_FORMS_VERIFIED: bool = false;

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
    /// The statement ends with `;` (not with the end of its line).
    semi: bool,
    /// First statement of a line that starts with spaces or tabs.
    indented: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Symbol(i64),
    Pointer(i64),
    Macro,
}

enum Ast {
    /// A literal, symbol or pointer value.
    Val { v: i64, literal: bool },
    Label(String),
    Bin(&'static str, Box<Ast>, Box<Ast>),
    Paren(Box<Ast>),
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
    content_bits: u32,
    address_bits: u32,
    hits: Vec<String>,
    used: Vec<&'static str>,
    names: HashMap<String, Kind>,
    macros: HashMap<String, Vec<Tok>>,
    removed: HashSet<String>,
    /// Names a later directive defines (for "used before its definition").
    defined_later: HashSet<String>,
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

    fn max_value(&self) -> i64 {
        (1i64 << self.content_bits) - 1
    }

    /// A numeric literal. `None` after a gate hit.
    fn literal(&mut self, tok: &Tok, line: usize) -> Result<Option<i64>, Fatal> {
        let s = tok.s.as_str();
        let bad = format!("{}: '{}' is not a number", self.at(line, tok.col), escape(s));
        if let Some(digits) = s.strip_prefix('-') {
            if self.content_bits != 32 {
                self.undefined("b", "a negative literal on a board with fewer than 32 content bits", line, tok.col);
                return Ok(None);
            }
            self.unverified("negative literal", line, tok.col);
            let v: i64 = digits.replace('_', "").parse().map_err(|_| bad.clone())?;
            if v > 1 << 31 {
                self.undefined("b", "a literal wider than 32 bits", line, tok.col);
                return Ok(None);
            }
            return Ok(Some((-v) as i32 as u32 as i64));
        }
        if s.starts_with('+') {
            self.undefined("q", "a literal with a plus sign", line, tok.col);
            return Ok(None);
        }
        if s.starts_with("0X") || s.starts_with("0B") {
            self.undefined("q", "an upper-case 0X or 0B prefix", line, tok.col);
            return Ok(None);
        }
        let (digits, radix) = match (s.strip_prefix("0x"), s.strip_prefix("0b")) {
            (Some(hex), _) => (hex, 16),
            (_, Some(bin)) => (bin, 2),
            _ => (s, 10),
        };
        if radix == 16 && digits.chars().any(|c| c.is_ascii_lowercase()) {
            self.undefined("q", "lower-case hex digits", line, tok.col);
            return Ok(None);
        }
        if radix != 2 && digits.contains('_') {
            self.unverified("underscores in decimal and hex literals", line, tok.col);
        }
        let v = u128::from_str_radix(&digits.replace('_', ""), radix).map_err(|_| bad)?;
        if v > self.max_value() as u128 {
            self.undefined("b", "a literal wider than the content bits", line, tok.col);
            return Ok(None);
        }
        Ok(Some(v as i64))
    }

    /// A literal that must be there (a directive's argument). A name or an expression is question 23p.
    fn directive_literal(&mut self, tok: &Tok, line: usize) -> Result<Option<i64>, Fatal> {
        if is_literal_like(&tok.s) {
            return self.literal(tok, line);
        }
        self.undefined("p", "a directive argument that is not a literal", line, tok.col);
        Ok(None)
    }

    /// Checks a name being defined. Returns false when the definition should not go ahead.
    fn check_new_name(&mut self, tok: &Tok, line: usize, kind: &str) -> bool {
        let name = tok.s.as_str();
        if !is_name(name) {
            self.undefined("x", "a name with characters other than letters, digits and underscore, or starting with a digit", line, tok.col);
            return false;
        }
        if let Some(keyword) = KEYWORDS.iter().find(|k| k.eq_ignore_ascii_case(name)) {
            let _ = keyword;
            self.undefined("e", "a keyword used as a name", line, tok.col);
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
            Some(k) if k == kind => self.undefined("d", "a name defined twice", line, tok.col),
            Some(_) => self.undefined("e", "one name used for two kinds", line, tok.col),
            None => {}
        }
        true
    }

    /// The value of a name used in an instruction.
    fn primitive(&mut self, tok: &Tok, line: usize) -> Result<Ast, Fatal> {
        let name = tok.s.as_str();
        let glued_operator = name.len() > 1 && OPERATORS.iter().any(|op| name[1..].contains(op));
        if is_literal_like(name) && !glued_operator {
            return Ok(self.literal(tok, line)?.map_or(Ast::Unknown, |v| Ast::Val { v, literal: true }));
        }
        if !is_name(name) {
            if glued_operator || OPERATORS.iter().any(|op| name.contains(op)) {
                self.undefined("p", "an operator without spaces around it", line, tok.col);
            } else {
                self.undefined("x", "a name with characters other than letters, digits and underscore", line, tok.col);
            }
            return Ok(Ast::Unknown);
        }
        match self.names.get(name) {
            Some(Kind::Symbol(v)) | Some(Kind::Pointer(v)) => return Ok(Ast::Val { v: *v, literal: false }),
            Some(Kind::Macro) => self.undefined("j", "a macro use together with other primitives", line, tok.col),
            None if self.labels.contains_key(name) => return Ok(Ast::Label(name.to_string())),
            None if KEYWORDS.contains(&name) => self.undefined("", "a keyword inside an instruction", line, tok.col),
            None if self.removed.contains(name) => self.undefined("f", "a name used after it was undefined", line, tok.col),
            None if self.defined_later.contains(name) => self.undefined("g", "a name used before its definition", line, tok.col),
            None => self.undefined("f", &format!("the name '{name}' is not defined"), line, tok.col),
        }
        Ok(Ast::Unknown)
    }

    /// One level of an expression: operand { operator operand }, up to `)` or the end.
    fn expression(&mut self, toks: &[Tok], i: &mut usize, line: usize, depth: usize) -> Result<Ast, Fatal> {
        if depth > MAX_NESTING {
            return Err(format!("{}: parentheses nested more than {MAX_NESTING} deep", self.at(line, toks[*i - 1].col)));
        }
        let mut operands = vec![self.operand(toks, i, line, depth)?];
        let mut ops: Vec<&'static str> = Vec::new();
        while *i < toks.len() && toks[*i].s != ")" {
            let tok = &toks[*i];
            match OPERATORS.iter().find(|op| **op == tok.s) {
                Some(op) if *op != "~" => {
                    ops.push(op);
                    *i += 1;
                }
                _ => {
                    self.undefined("", "primitives side by side inside an expression", line, tok.col);
                    ops.push("|");
                }
            }
            operands.push(self.operand(toks, i, line, depth)?);
        }
        let col = toks[(*i).min(toks.len() - 1)].col;
        if ops.iter().any(|op| *op != ops[0]) {
            self.undefined("a", "different operators without parentheses", line, col);
            return Ok(Ast::Unknown);
        }
        if ops.len() > 1 && ops[0] != "|" {
            self.undefined("w", "a chain of one operator other than | without parentheses", line, col);
            return Ok(Ast::Unknown);
        }
        let mut operands = operands.into_iter();
        let mut ast = operands.next().unwrap();
        for (op, right) in ops.into_iter().zip(operands) {
            ast = Ast::Bin(op, Box::new(ast), Box::new(right));
        }
        Ok(ast)
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
            "~" => {
                self.undefined("u", "the operator ~", line, tok.col);
                self.operand(toks, i, line, depth)?;
                Ok(Ast::Unknown)
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
    fn eval(&mut self, ast: &Ast, line: usize, col: usize) -> Option<i64> {
        let max = self.max_value();
        match ast {
            Ast::Unknown => None,
            Ast::Val { v, .. } => Some(*v),
            Ast::Label(name) => self.labels[name],
            Ast::Paren(inner) => self.eval(inner, line, col),
            Ast::Prims(prims) => {
                let mut value = 0;
                for p in prims {
                    let v = self.eval(p, line, col)?;
                    if value & v != 0 {
                        self.unverified("OR of overlapping bits", line, col);
                    }
                    value |= v;
                }
                Some(value)
            }
            Ast::Bin(op, left, right) => {
                let (a, b) = (self.eval(left, line, col), self.eval(right, line, col));
                let (a, b) = (a?, b?);
                let (letter, what) = match *op {
                    "|" if a & b != 0 => {
                        self.unverified("OR of overlapping bits", line, col);
                        return Some(a | b);
                    }
                    "|" => return Some(a | b),
                    "&" => return Some(a & b),
                    "^" => return Some(a ^ b),
                    "+" | "*" => match if *op == "+" { a + b } else { a * b } {
                        v if v <= max => return Some(v),
                        _ => ("b", "a result wider than the content bits"),
                    },
                    "-" if a >= b => return Some(a - b),
                    "-" => ("b", "a negative result"),
                    "/" | "%" if b == 0 => ("c", "division or modulo by zero"),
                    "/" if a % b == 0 => return Some(a / b),
                    "/" => ("u", "a division with an inexact result"),
                    "%" => return Some(a % b),
                    _ if b >= 32 => ("c", "a shift by 32 or more"),
                    ">>" => return Some(a >> b),
                    _ if a << b <= max => return Some(a << b),
                    _ => ("b", "a result wider than the content bits"),
                };
                self.undefined(letter, what, line, col);
                None
            }
        }
    }

    /// Gate checks on the form of an expression: which operators, and whether it is the shape the game has
    /// assembled (a macro body `x | y | (z >> literal)`).
    fn check_shape(&mut self, ast: &Ast, in_macro: bool, line: usize, col: usize) {
        fn operators(ast: &Ast, found: &mut Vec<&'static str>) {
            match ast {
                Ast::Bin(op, a, b) => {
                    found.push(op);
                    operators(a, found);
                    operators(b, found);
                }
                Ast::Paren(inner) => operators(inner, found),
                _ => {}
            }
        }
        fn term(ast: &Ast) -> bool {
            match ast {
                Ast::Val { literal: false, .. } | Ast::Label(_) => true,
                Ast::Paren(inner) => matches!(&**inner, Ast::Bin(">>", a, b)
                    if matches!(**a, Ast::Val { literal: false, .. } | Ast::Label(_)) && matches!(**b, Ast::Val { literal: true, .. })),
                _ => false,
            }
        }
        fn or_chain(ast: &Ast) -> bool {
            match ast {
                Ast::Bin("|", a, b) => or_chain(a) && term(b),
                other => term(other),
            }
        }
        if matches!(ast, Ast::Prims(_) | Ast::Unknown) || (in_macro && or_chain(ast)) {
            return;
        }
        self.unverified("an expression outside the verified macro shape", line, col);
        let mut found = Vec::new();
        operators(ast, &mut found);
        for op in found {
            let feature = match op {
                "+" => "operator +",
                "-" => "operator -",
                "*" => "operator *",
                "/" => "operator /",
                "%" => "operator %",
                "&" => "operator &",
                "^" => "operator ^",
                "<<" => "operator <<",
                _ => continue,
            };
            self.unverified(feature, line, col);
        }
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
                if full_line[at..].contains(';') {
                    asm.undefined("t", "a ; inside a comment", line, at + 1);
                }
                if !full_line[..at].trim().is_empty() {
                    asm.unverified("comment after a statement", line, at + 1);
                }
                &full_line[..at]
            }
            None => full_line,
        };
        let mut start = 0;
        let pieces: Vec<&str> = code.split(';').collect();
        for (k, piece) in pieces.iter().enumerate() {
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
                let indented = k == 0 && piece.starts_with(|c: char| c.is_whitespace());
                out.push(Stmt { toks, line, text: piece.trim().to_string(), semi: k + 1 < pieces.len(), indented });
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
    let (address_bits, content_bits) = (board.settings.vmem_settings[0], board.settings.vmem_settings[7]);
    if !(1..=20).contains(&address_bits) || !(1..=32).contains(&content_bits) {
        return Err(format!("vmem_settings: {address_bits} address bits and {content_bits} content bits cannot be assembled for"));
    }
    let mut asm = Asm {
        source_name,
        allow: allow_unverified,
        content_bits: content_bits as u32,
        address_bits: address_bits as u32,
        hits: Vec::new(),
        used: Vec::new(),
        names: HashMap::new(),
        macros: HashMap::new(),
        removed: HashSet::new(),
        defined_later: HashSet::new(),
        labels: HashMap::new(),
        pointers: Vec::new(),
    };
    match walk(&mut asm, source) {
        Ok(words) if asm.hits.is_empty() => {
            let mut report = Report { unverified: asm.used, ..Default::default() };
            for w in &words {
                let old = board.vmem[w.address];
                report.replaced += (old != 0 && old != w.value) as usize;
            }
            for w in &words {
                board.vmem[w.address] = w.value;
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
                asm.undefined("r", "a carriage return in the source", n + 1, at + 1);
            } else {
                return Err(format!("{}: a control character in the source", asm.at(n + 1, at + 1)));
            }
        }
    }
    let stmts = statements(asm, source);

    // Labels are global, so their names are needed before the first use.
    for st in &stmts {
        let name = |i: usize| st.toks.get(i).map(|t| t.s.clone());
        match st.toks[0].s.as_str() {
            "@" => {
                if let Some(name) = name(1) {
                    asm.labels.entry(name).or_insert(None);
                }
            }
            "symbol" | "pointer" | "macro" => {
                asm.defined_later.extend(name(1));
            }
            _ => {}
        }
    }

    let limit = 1i64 << asm.address_bits;
    let mut emits: Vec<Emit> = Vec::new();
    let mut seen_labels: HashSet<String> = HashSet::new();
    let mut pending_labels: Vec<String> = Vec::new();
    let mut cur: i64 = 1;
    let mut origin_seen = false;
    // After `pointer <name> <address> <value>` the current address is question 23l until an `origin` sets it.
    let mut after_pointer = false;
    // One entry per `origin <address>`: the address to go back to, and whether the manual pins it down
    // (it does when nothing was stored since the origin before it).
    let mut origins: Vec<(i64, bool)> = Vec::new();
    let mut stored_since_origin = false;
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
        let mut place = |asm: &mut Asm, cur: &mut i64| -> i64 {
            if after_pointer {
                asm.undefined("l", "the current address after a pointer with an address (put an origin before this statement)", line, col);
            } else if !origin_seen {
                asm.unverified("first address is 1", line, col);
            }
            if *cur >= limit {
                asm.undefined("n", "an address beyond the address bits", line, col);
            }
            stored_since_origin = true;
            *cur += 1;
            *cur - 1
        };
        if head != "@" && !pending_labels.is_empty() && matches!(head, "origin" | "pointer") {
            asm.undefined("s", "a label followed by origin or pointer", line, col);
            for name in pending_labels.drain(..) {
                asm.labels.insert(name, Some(cur));
            }
        }
        match head {
            "symbol" | "resymb" => {
                args(2)?;
                let value = asm.directive_literal(&toks[2], line)?;
                if head == "symbol" {
                    if !asm.check_new_name(&toks[1], line, "symbol") {
                        continue;
                    }
                } else {
                    asm.unverified("resymb", line, col);
                    if !matches!(asm.names.get(&toks[1].s), Some(Kind::Symbol(_))) {
                        asm.undefined("", "resymb of a name that is not a symbol", line, toks[1].col);
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
                    asm.removed.insert(toks[1].s.clone());
                } else {
                    asm.undefined("", &format!("{head} of a name that is not defined as that kind"), line, toks[1].col);
                }
            }
            "repoint" => asm.undefined("v", "repoint", line, col),
            "pointer" => {
                args(3)?;
                let value = asm.directive_literal(&toks[3], line)?;
                let address = if toks[2].s == "inline" {
                    asm.unverified("pointer inline", line, toks[2].col);
                    Some(place(asm, &mut cur))
                } else {
                    after_pointer = true;
                    asm.directive_literal(&toks[2], line)?
                };
                if address.is_some_and(|a| a == 0 || a >= limit) {
                    asm.undefined("n", "address 0 or an address beyond the address bits", line, toks[2].col);
                    continue;
                }
                if !asm.check_new_name(&toks[1], line, "pointer") {
                    continue;
                }
                let address = address.unwrap_or(0);
                asm.names.insert(toks[1].s.clone(), Kind::Pointer(address));
                asm.pointers.push((toks[1].s.clone(), address));
                let ast = value.map_or(Ast::Unknown, |v| Ast::Val { v, literal: true });
                emits.push(Emit { address, ast, stmt: index, col, in_macro: None });
            }
            "origin" => {
                args(1)?;
                match toks[1].s.as_str() {
                    back @ ("orgprev" | "orgbase") => {
                        asm.unverified(if back == "orgprev" { "origin orgprev" } else { "origin orgbase" }, line, toks[1].col);
                        let entry = if back == "orgprev" { origins.pop() } else { origins.drain(..).next() };
                        match entry {
                            None => asm.undefined("o", "orgprev or orgbase with nothing to revert", line, toks[1].col),
                            Some((address, pinned)) => {
                                if !pinned || stored_since_origin {
                                    asm.undefined("", "orgprev or orgbase after words were stored since the origin", line, toks[1].col);
                                }
                                cur = address;
                            }
                        }
                    }
                    _ => {
                        let Some(address) = asm.directive_literal(&toks[1], line)? else { continue };
                        if address == 0 || address >= limit {
                            asm.undefined("n", "address 0 or an address beyond the address bits", line, toks[1].col);
                            continue;
                        }
                        origins.push((cur, origins.is_empty() || !stored_since_origin));
                        cur = address;
                    }
                }
                (origin_seen, after_pointer, stored_since_origin) = (true, false, false);
            }
            "@" => {
                if toks.len() < 2 {
                    return Err(format!("{}: '@' needs a label name", asm.at(line, col)));
                }
                if toks.len() > 2 {
                    asm.undefined("q", "a label and an instruction in one statement", line, toks[2].col);
                }
                if !PART2_LABEL_FORMS_VERIFIED && (st.semi || st.indented) {
                    asm.undefined("q", "a label followed by ; or an indented label", line, col);
                }
                let name = &toks[1];
                if !is_name(&name.s) {
                    asm.undefined("x", "a name with characters other than letters, digits and underscore, or starting with a digit", line, name.col);
                } else if KEYWORDS.contains(&name.s.as_str()) || asm.names.contains_key(&name.s) {
                    asm.undefined("e", "one name used for two kinds", line, name.col);
                } else if !seen_labels.insert(name.s.clone()) {
                    asm.undefined("d", "a name defined twice", line, name.col);
                }
                pending_labels.push(name.s.clone());
            }
            "macro" | "remac" => {
                if toks.len() < 3 || toks[2].s != "{" {
                    return Err(format!("{}: expected '{head} <name> {{ <body> }}'", asm.at(line, col)));
                }
                if toks.last().unwrap().s != "}" {
                    asm.undefined("k", "a macro body over several lines", line, col);
                    skip_to_brace = true;
                    continue;
                }
                let body = toks[3..toks.len() - 1].to_vec();
                let mut letters: Vec<u8> = body.iter().filter(|t| is_placeholder(&t.s)).map(|t| t.s.as_bytes()[0] - b'a').collect();
                letters.sort();
                letters.dedup();
                if letters.iter().enumerate().any(|(i, l)| *l as usize != i) {
                    asm.undefined("", "macro placeholders that skip a letter", line, col);
                }
                if letters.len() != 2 {
                    asm.unverified("macro without exactly two placeholders", line, col);
                }
                if head == "macro" {
                    if !asm.check_new_name(&toks[1], line, "macro") {
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
            _ if KEYWORDS.iter().any(|k| k.eq_ignore_ascii_case(head)) => {
                asm.undefined("q", "a keyword that is not in lower case", line, col);
            }
            _ => {
                // An instruction: one word at the current address.
                let (expanded, in_macro) = match asm.macros.get(head).filter(|_| asm.names.get(head) == Some(&Kind::Macro)) {
                    None => (Some(toks.clone()), None),
                    Some(body) => {
                        let body = body.clone();
                        let placeholders = body.iter().filter(|t| is_placeholder(&t.s)).map(|t| t.s.as_bytes()[0]).collect::<HashSet<_>>().len();
                        let call = &toks[1..];
                        let mut usable = true;
                        for arg in call {
                            if is_literal_like(&arg.s) {
                                asm.unverified("macro argument that is a literal", line, arg.col);
                            } else if !is_name(&arg.s) {
                                asm.undefined("i", "a macro argument that is an expression", line, arg.col);
                                usable = false;
                            }
                        }
                        if usable && call.len() != placeholders {
                            asm.undefined("h", "a macro used with fewer or more arguments than it has placeholders", line, col);
                            usable = false;
                        }
                        let expanded: Vec<Tok> = body
                            .iter()
                            .map(|t| if is_placeholder(&t.s) { call.get((t.s.as_bytes()[0] - b'a') as usize).unwrap_or(t).clone() } else { Tok { s: t.s.clone(), col } })
                            .collect();
                        if usable && expanded.iter().any(|t| asm.names.get(&t.s) == Some(&Kind::Macro)) {
                            asm.undefined("k", "a macro inside a macro", line, col);
                            usable = false;
                        }
                        if usable && expanded.is_empty() {
                            asm.undefined("", "a macro with an empty body", line, col);
                            usable = false;
                        }
                        (usable.then_some(expanded), Some(head.to_string()))
                    }
                };
                let ast = match expanded {
                    Some(toks) => asm.instruction(&toks, line).map_err(|e| match &in_macro {
                        Some(name) => format!("{e} (in macro {name})"),
                        None => e,
                    })?,
                    None => Ast::Unknown,
                };
                let hits_before = asm.hits.len();
                asm.check_shape(&ast, in_macro.is_some(), line, col);
                if let Some(name) = &in_macro {
                    for hit in &mut asm.hits[hits_before..] {
                        *hit += &format!(" (in macro {name})");
                    }
                }
                let address = place(asm, &mut cur);
                for name in pending_labels.drain(..) {
                    asm.labels.insert(name, Some(address));
                }
                emits.push(Emit { address, ast, stmt: index, col, in_macro });
            }
        }
    }
    if skip_to_brace {
        return Err(format!("{}: a macro body is never closed with '}}'", asm.source_name));
    }
    if let Some(name) = pending_labels.first() {
        let line = stmts.last().map_or(1, |s| s.line);
        asm.undefined("s", &format!("the label '{name}' is followed by the end of the source"), line, 1);
    }

    let mut words: Vec<Word> = Vec::new();
    let mut written: HashSet<i64> = HashSet::new();
    for emit in &emits {
        let st = &stmts[emit.stmt];
        let hits_before = asm.hits.len();
        let value = asm.eval(&emit.ast, st.line, emit.col);
        if value.is_some_and(|v| v > asm.max_value()) {
            asm.undefined("b", "a value wider than the content bits", st.line, emit.col);
        }
        if let Some(name) = &emit.in_macro {
            for hit in &mut asm.hits[hits_before..] {
                *hit += &format!(" (in macro {name})");
            }
        }
        if !written.insert(emit.address) {
            asm.undefined("m", "two statements store to the same address", st.line, emit.col);
        }
        if let Some(value) = value {
            if (1..limit).contains(&emit.address) {
                words.push(Word { address: emit.address as usize, value: value as u32, line: st.line, text: st.text.clone() });
            }
        }
    }
    Ok(words)
}
