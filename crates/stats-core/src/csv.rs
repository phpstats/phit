//! A delimited-text reader that reproduces phibble's PHP CSV reader (which
//! follows readr) byte for byte: the same records, names, guessed types,
//! parsed values and problem messages.
//!
//! phibble tokenizes by vroom's (readr 2's) quote rules with a fast reader
//! when it can (no comment, `""` escapes, no backslash escapes, a quote
//! character) and with a character-level reader otherwise. Both are
//! reproduced here, including their corner cases; the values and types follow
//! phibble's `TextParser`.
//!
//! The input is read a block at a time. [`read`] reads it all; [`Reader`]
//! reads it a chunk of rows at a time, as phibble's `DataFrame::csvChunks()`
//! does. Neither result depends on the block size.

use std::borrow::Cow;
use std::io::{self, Read};
use std::sync::Arc;

/// Column type requested for one position.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TypeCode {
    Guess,
    Bool,
    Int,
    Double,
    String,
    Date,
    DateTime,
    Skip,
}

impl TypeCode {
    pub fn from_code(code: &[u8]) -> Option<Self> {
        Some(match code {
            b"?" => Self::Guess,
            b"l" => Self::Bool,
            b"i" => Self::Int,
            b"d" => Self::Double,
            b"c" => Self::String,
            b"D" => Self::Date,
            b"T" => Self::DateTime,
            b"_" => Self::Skip,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Options {
    pub delimiter: u8,
    pub quote: Option<u8>,
    pub escape_double: bool,
    pub escape_backslash: bool,
    pub na: Vec<Vec<u8>>,
    pub trim_ws: bool,
    pub skip: usize,
    pub n_max: Option<usize>,
    pub comment: Vec<u8>,
    pub skip_empty_rows: bool,
    pub col_names: bool,
    /// By 0-based position; positions past the end are guessed.
    pub types: Vec<TypeCode>,
    pub guess_max: usize,
    pub decimal_mark: u8,
    /// None: the default (`.` when the decimal mark is `,`, else `,`).
    /// Some(None): no grouping mark.
    pub grouping_mark: Option<Option<u8>>,
    /// Bytes read from the input at a time. Tests shrink it to put block
    /// boundaries everywhere; the result does not depend on it.
    pub block_size: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            delimiter: b',',
            quote: Some(b'"'),
            escape_double: true,
            escape_backslash: false,
            na: vec![Vec::new(), b"NA".to_vec()],
            trim_ws: true,
            skip: 0,
            n_max: None,
            comment: Vec::new(),
            skip_empty_rows: true,
            col_names: true,
            types: Vec::new(),
            guess_max: 1000,
            decimal_mark: b'.',
            grouping_mark: None,
            block_size: 1 << 22,
        }
    }
}

impl Options {
    /// The grouping mark used when guessing and parsing guessed numbers.
    fn grouping(&self) -> Option<u8> {
        match self.grouping_mark {
            Some(mark) => mark,
            None => Some(if self.decimal_mark == b',' {
                b'.'
            } else {
                b','
            }),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.block_size < 1 {
            return Err("block_size must be at least 1".into());
        }
        if self.guess_max < 1 {
            return Err("guess_max must be at least 1".into());
        }
        let reserved = |b: u8| b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'e' | b'E' | b'\n');
        if reserved(self.decimal_mark) {
            return Err(
                "decimal_mark must not be a digit, sign, exponent marker or newline".into(),
            );
        }
        if let Some(g) = self.grouping() {
            if reserved(g) {
                return Err(
                    "grouping_mark must not be a digit, sign, exponent marker or newline".into(),
                );
            }
            if g == self.decimal_mark {
                return Err("decimal_mark and grouping_mark must differ".into());
            }
        }
        Ok(())
    }
}

/// A field: bytes `start..start + len` of the reader's text, or of its arena
/// when `start` has the ARENA bit. `len == NA_LEN` marks a missing value.
#[derive(Clone, Copy, Debug)]
pub struct Span {
    start: usize,
    len: u32,
}

const NA_LEN: u32 = u32::MAX;
const ARENA: usize = 1 << (usize::BITS - 1);

fn view<'a>(text: &'a [u8], arena: &'a [u8], span: Span) -> &'a [u8] {
    let len = span.len as usize;
    if span.start & ARENA != 0 {
        let start = span.start & !ARENA;
        &arena[start..start + len]
    } else {
        &text[span.start..span.start + len]
    }
}

impl Span {
    pub const NA: Span = Span {
        start: 0,
        len: NA_LEN,
    };

    pub fn is_na(self) -> bool {
        self.len == NA_LEN
    }
}

#[derive(Debug)]
pub enum Values {
    Bool(Vec<Option<bool>>),
    Int(Vec<Option<i64>>),
    Float(Vec<Option<f64>>),
    /// Text spans; resolve them with [`Table::bytes`].
    Str(Vec<Span>),
    /// Days since 1970-01-01.
    Date(Vec<Option<i64>>),
    /// Microseconds since the epoch, UTC.
    DateTime(Vec<Option<i64>>),
}

impl Values {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Bool(_) => "bool",
            Self::Int(_) => "int",
            Self::Float(_) => "float",
            Self::Str(_) => "string",
            Self::Date(_) => "date",
            Self::DateTime(_) => "datetime",
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Self::Bool(v) => v.len(),
            Self::Int(v) | Self::Date(v) | Self::DateTime(v) => v.len(),
            Self::Float(v) => v.len(),
            Self::Str(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Debug)]
pub struct Column {
    pub values: Values,
    pub failed: usize,
}

#[derive(Debug)]
pub struct Table {
    pub names: Vec<Vec<u8>>,
    /// None for skipped columns.
    pub columns: Vec<Option<Column>>,
    pub problems: Vec<String>,
    text: Vec<u8>,
    arena: Vec<u8>,
}

impl Table {
    pub fn bytes(&self, span: Span) -> &[u8] {
        view(&self.text, &self.arena, span)
    }
}

// ---------------------------------------------------------------------
// Tokenizers
// ---------------------------------------------------------------------

enum Record {
    Eof,
    /// A blank line that skip_empty_rows drops.
    Blank,
    Fields,
}

/// Tokenizes text read a block at a time. A record is parsed only once the
/// text holds all of it: one that runs into the end of the text before the
/// input is exhausted is parsed again after the next block, so records never
/// depend on where blocks end.
struct Tokenizer {
    text: Vec<u8>,
    arena: Vec<u8>,
    pos: usize,
    o: Arc<Options>,
    /// vroom's quote tokenizer, or phibble's character-level reader.
    fast: bool,
    /// Set when the last record ended inside an open quote.
    unterminated: bool,
    /// Whether a record has been produced yet, for the leading comment rule.
    emitted: bool,
    source: Box<dyn Read + Send>,
    /// The input is exhausted and all of it is in `text`.
    exhausted: bool,
    /// Set when the last record stopped at the end of the text, not at a newline.
    hit_end: bool,
    /// Lines still to skip, as phibble skips them with fgets().
    to_skip: usize,
    /// Input read but not yet in `text`: the start of the file until it can
    /// be checked for a byte order mark, a CR that may begin a CRLF, and the
    /// bytes needed to decide `cr_only`.
    raw: Vec<u8>,
    /// Whether `raw` still starts the file (the byte order mark is unchecked).
    at_start: bool,
    /// Input bytes moved from `raw` into `text` so far.
    consumed: usize,
    /// Whether lines end in CR alone, decided by the window of input holding
    /// the first line break.
    cr_only: Option<bool>,
}

/// phibble reads 4 MiB blocks and reads lines as ending in CR alone when the
/// first block with a line break holds no LF. The same window, aligned the
/// same way, decides it here whatever the block size.
const CR_WINDOW: usize = 1 << 22;

/// Builds a field from pieces of the text, copying into the arena only
/// when it is not one contiguous slice.
struct Acc {
    slice: Option<(usize, usize)>,
    owned: Option<usize>,
}

impl Acc {
    fn new() -> Self {
        Self {
            slice: None,
            owned: None,
        }
    }

    fn push(&mut self, text: &[u8], arena: &mut Vec<u8>, from: usize, to: usize) {
        if from >= to {
            return;
        }
        match (self.owned, self.slice) {
            (Some(_), _) => arena.extend_from_slice(&text[from..to]),
            (None, None) => self.slice = Some((from, to)),
            (None, Some((a, b))) if b == from => self.slice = Some((a, to)),
            (None, Some((a, b))) => {
                self.owned = Some(arena.len());
                arena.extend_from_slice(&text[a..b]);
                arena.extend_from_slice(&text[from..to]);
            }
        }
    }

    fn finish(self, arena: &[u8]) -> Span {
        match (self.owned, self.slice) {
            (Some(start), _) => Span {
                start: ARENA | start,
                len: (arena.len() - start) as u32,
            },
            (None, Some((a, b))) => Span {
                start: a,
                len: (b - a) as u32,
            },
            (None, None) => Span { start: 0, len: 0 },
        }
    }
}

impl Tokenizer {
    fn new(source: Box<dyn Read + Send>, o: Arc<Options>) -> Self {
        let fast =
            o.comment.is_empty() && o.escape_double && !o.escape_backslash && o.quote.is_some();
        Self {
            text: Vec::new(),
            arena: Vec::new(),
            pos: 0,
            fast,
            unterminated: false,
            emitted: false,
            source,
            exhausted: false,
            hit_end: false,
            to_skip: o.skip,
            raw: Vec::new(),
            at_start: true,
            consumed: 0,
            cr_only: None,
            o,
        }
    }

    fn next(&mut self, fields: &mut Vec<Span>) -> io::Result<Record> {
        loop {
            let (pos, arena, emitted) = (self.pos, self.arena.len(), self.emitted);
            fields.clear();
            self.unterminated = false;
            self.hit_end = false;
            let record = if self.fast {
                self.next_quoted(fields)
            } else {
                self.next_chars(fields)
            };
            if self.exhausted || !(self.hit_end || matches!(record, Record::Eof)) {
                return Ok(record);
            }
            self.pos = pos;
            self.arena.truncate(arena);
            self.emitted = emitted;
            self.refill()?;
        }
    }

    /// Whether no record remains, consuming any blank lines before the next one.
    fn at_end(&mut self) -> io::Result<bool> {
        let mut fields = Vec::new();
        loop {
            let (pos, arena, emitted) = (self.pos, self.arena.len(), self.emitted);
            match self.next(&mut fields)? {
                Record::Eof => return Ok(true),
                Record::Blank => {}
                Record::Fields => {
                    self.pos = pos;
                    self.arena.truncate(arena);
                    self.emitted = emitted;
                    return Ok(false);
                }
            }
        }
    }

    /// Drops text and arena bytes no span refers to any more: called between
    /// chunks. The text is moved only once most of it has been read, so the
    /// cost stays linear in the input.
    fn release(&mut self) {
        self.arena.clear();
        if self.pos > 0 && self.pos * 2 >= self.text.len() {
            self.text.drain(..self.pos);
            self.pos = 0;
        }
    }

    /// Appends input to the text, prepared as phibble prepares it: lines
    /// skipped, a byte order mark dropped, and line endings made LF.
    fn refill(&mut self) -> io::Result<()> {
        loop {
            // A record longer than a block is read in growing blocks.
            let size = self.o.block_size.max(self.text.len() - self.pos);
            let mut block = read_up_to(&mut self.source, size)?;
            let eof = block.len() < size;
            while self.to_skip > 0 {
                match memchr::memchr(b'\n', &block) {
                    Some(i) => {
                        block.drain(..=i);
                        self.to_skip -= 1;
                    }
                    None => {
                        block.clear();
                        break;
                    }
                }
            }
            if self.to_skip > 0 {
                if eof {
                    self.exhausted = true;
                    return Ok(());
                }
                continue;
            }
            self.raw.extend_from_slice(&block);
            if self.at_start {
                if self.raw.len() < 3 && !eof {
                    continue;
                }
                if self.o.skip == 0 && self.raw.starts_with(b"\xEF\xBB\xBF") {
                    self.raw.drain(..3);
                }
                self.at_start = false;
            }
            if self.cr_only.is_none()
                && let Some(i) = memchr::memchr2(b'\r', b'\n', &self.raw)
            {
                let first = self.consumed + i;
                let window = first - first % CR_WINDOW;
                let end = window + CR_WINDOW - self.consumed;
                if self.raw.len() < end && !eof {
                    continue;
                }
                let from = window.saturating_sub(self.consumed);
                let to = end.min(self.raw.len());
                self.cr_only = Some(memchr::memchr(b'\n', &self.raw[from..to]).is_none());
            }
            // A CR at the end may start a CRLF that the next block finishes.
            let mut take = self.raw.len();
            if !eof && self.cr_only != Some(true) && self.raw.last() == Some(&b'\r') {
                take -= 1;
            }
            let ready = &self.raw[..take];
            if self.cr_only == Some(true) {
                self.text
                    .extend(ready.iter().map(|&b| if b == b'\r' { b'\n' } else { b }));
            } else if memchr::memchr(b'\r', ready).is_some() {
                self.text.extend_from_slice(&replace_crlf(ready));
            } else {
                self.text.extend_from_slice(ready);
            }
            self.raw.drain(..take);
            self.consumed += take;
            if eof {
                self.exhausted = true;
                return Ok(());
            }
            if take > 0 {
                return Ok(());
            }
        }
    }

    fn line_end(&self, start: usize) -> usize {
        memchr::memchr(b'\n', &self.text[start..]).map_or(self.text.len(), |i| start + i + 1)
    }

    /// The end of the line at `start` when skip_empty_rows drops it as blank.
    ///
    /// readr decides blankness from the raw line rather than from the parsed
    /// fields, so a quoted empty string is a field holding data while a line of
    /// spaces or tabs is blank whatever trim_ws says. It also never drops a last
    /// line the file left unterminated. A blank line holds no quote, so it is
    /// always one line long.
    fn blank_line(&self, start: usize) -> Option<usize> {
        if !self.o.skip_empty_rows {
            return None;
        }
        let end = self.line_end(start);
        let line = &self.text[start..end];
        let terminated = line.last() == Some(&b'\n');
        (terminated
            && line
                .iter()
                .all(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n')))
        .then_some(end)
    }

    /// A record by vroom's (readr 2's) quote rules, as phibble's
    /// quotedRecords(). A field is quoted when its first byte is the quote.
    /// Within it, every lone quote toggles quoting and is dropped, and a
    /// doubled quote is a literal quote, so `"x "y" z"` reads as `x y z` and
    /// `"x "y",z"` as `x y,z`. Quotes in an unquoted field are literal.
    fn next_quoted(&mut self, fields: &mut Vec<Span>) -> Record {
        let len = self.text.len();
        if self.pos >= len {
            return Record::Eof;
        }
        if let Some(end) = self.blank_line(self.pos) {
            self.pos = end;
            return Record::Blank;
        }
        let delim = self.o.delimiter;
        let quote = self.o.quote.unwrap_or(0);
        let mut pos = self.pos;
        loop {
            if self.text[pos] == quote {
                pos += 1;
                let mut acc = Acc::new();
                let mut in_quotes = true;
                loop {
                    if in_quotes {
                        let Some(i) = memchr::memchr(quote, &self.text[pos..]) else {
                            // End of file inside quotes: the field takes the rest.
                            acc.push(&self.text, &mut self.arena, pos, len);
                            fields.push(acc.finish(&self.arena));
                            self.pos = len;
                            self.unterminated = true;
                            self.hit_end = true;
                            return Record::Fields;
                        };
                        acc.push(&self.text, &mut self.arena, pos, pos + i);
                        pos += i + 1;
                        if pos < len && self.text[pos] == quote {
                            acc.push(&self.text, &mut self.arena, pos, pos + 1);
                            pos += 1;
                        } else {
                            in_quotes = false;
                        }
                        continue;
                    }
                    let run = memchr::memchr3(delim, quote, b'\n', &self.text[pos..])
                        .map_or(len, |i| pos + i);
                    acc.push(&self.text, &mut self.arena, pos, run);
                    pos = run;
                    if pos < len && self.text[pos] == quote {
                        if pos + 1 < len && self.text[pos + 1] == quote {
                            acc.push(&self.text, &mut self.arena, pos, pos + 1);
                            pos += 2;
                        } else {
                            in_quotes = true;
                            pos += 1;
                        }
                        continue;
                    }
                    break;
                }
                fields.push(acc.finish(&self.arena));
            } else {
                let run = memchr::memchr2(delim, b'\n', &self.text[pos..]).map_or(len, |i| pos + i);
                let field = self.unquote_trimmed(Span {
                    start: pos,
                    len: (run - pos) as u32,
                });
                fields.push(field);
                pos = run;
            }
            if pos >= len {
                self.hit_end = true;
                break;
            }
            let end = self.text[pos];
            pos += 1;
            if end == b'\n' {
                break;
            }
            if pos >= len {
                // A delimiter at the very end leaves one empty field.
                fields.push(Span { start: 0, len: 0 });
                self.hit_end = true;
                break;
            }
        }
        self.pos = pos;
        Record::Fields
    }

    /// With trim_ws, an unquoted field that starts with a quote once trimmed
    /// is unquoted as vroom does: the first quote dropped, then doubled quotes
    /// made single and other quotes dropped (escape_double), or one closing
    /// quote dropped (without escape_double).
    fn unquote_trimmed(&mut self, field: Span) -> Span {
        let Some(quote) = self.o.quote else {
            return field;
        };
        if !self.o.trim_ws {
            return field;
        }
        let escape_double = self.o.escape_double;
        let bytes = self.bytes(field);
        let trimmed = trim_tabs(bytes);
        if trimmed.first() != Some(&quote) {
            return field;
        }
        let inner = &trimmed[1..];
        let out = if escape_double {
            let mut out = Vec::with_capacity(inner.len());
            let mut i = 0;
            while i < inner.len() {
                if inner[i] == quote {
                    if inner.get(i + 1) == Some(&quote) {
                        out.push(quote);
                        i += 1;
                    }
                } else {
                    out.push(inner[i]);
                }
                i += 1;
            }
            out
        } else {
            drop_closing_quote(inner, quote).to_vec()
        };
        let start = ARENA | self.arena.len();
        self.arena.extend_from_slice(&out);
        Span {
            start,
            len: out.len() as u32,
        }
    }

    fn starts_with_at(&self, pos: usize, s: &[u8]) -> bool {
        !s.is_empty() && self.text[pos..].starts_with(s)
    }

    /// phibble's character-level reader (comments, backslash escapes, no quote).
    fn next_chars(&mut self, fields: &mut Vec<Span>) -> Record {
        let o = Arc::clone(&self.o);
        let len = self.text.len();
        loop {
            if self.pos >= len {
                return Record::Eof;
            }
            // A line that starts with the comment string is dropped entirely.
            // Before the first record readr looks past leading whitespace for
            // it; after one, such a line is a record holding that whitespace.
            let mut at = self.pos;
            if !self.emitted {
                while at < len && matches!(self.text[at], b' ' | b'\t') {
                    at += 1;
                }
            }
            if self.starts_with_at(at, &o.comment) {
                self.pos = self.line_end(at);
                continue;
            }
            if let Some(end) = self.blank_line(self.pos) {
                self.pos = end;
                return Record::Blank;
            }
            break;
        }
        let delim = o.delimiter;
        let quote = o.quote;
        let mut field_start = self.arena.len();
        let mut quoted = false;
        let mut in_quotes = false;
        let mut pos = self.pos;
        let mut ended = false;
        while pos < len {
            let c = self.text[pos];
            if in_quotes {
                if o.escape_backslash && c == b'\\' && pos + 1 < len {
                    self.arena.push(self.text[pos + 1]);
                    pos += 2;
                    continue;
                }
                if Some(c) == quote {
                    if o.escape_double && pos + 1 < len && Some(self.text[pos + 1]) == quote {
                        self.arena.push(c);
                        pos += 2;
                        continue;
                    }
                    in_quotes = false;
                    if !o.escape_double {
                        self.arena.push(c);
                    }
                    pos += 1;
                    continue;
                }
                self.arena.push(c);
                pos += 1;
                continue;
            }
            if c == b'\n' {
                pos += 1;
                ended = true;
                break;
            }
            if Some(c) == quote && (quoted || self.arena.len() == field_start) {
                // Opens a quoted field, or toggles quoting inside one.
                if quoted && o.escape_double && pos + 1 < len && Some(self.text[pos + 1]) == quote {
                    self.arena.push(c);
                    pos += 2;
                    continue;
                }
                if quoted && !o.escape_double {
                    self.arena.push(c);
                }
                quoted = true;
                in_quotes = true;
                pos += 1;
                continue;
            }
            if self.starts_with_at(pos, &o.comment) {
                pos = self.line_end(pos);
                ended = self.text[pos - 1] == b'\n';
                break;
            }
            if c == delim {
                fields.push(self.finish_field(field_start, quoted));
                field_start = self.arena.len();
                quoted = false;
                pos += 1;
                continue;
            }
            if o.escape_backslash && c == b'\\' && pos + 1 < len {
                self.arena.push(self.text[pos + 1]);
                pos += 2;
                continue;
            }
            self.arena.push(c);
            pos += 1;
        }
        self.pos = pos;
        self.hit_end = !ended;
        self.unterminated = in_quotes;
        fields.push(self.finish_field(field_start, quoted));
        self.emitted = true;
        Record::Fields
    }

    /// The arena field from `start`, as phibble's finishField(). Without
    /// escape_double, vroom keeps the quotes inside a quoted field and drops
    /// only the opening and closing ones; next_chars() keeps every inner quote,
    /// so the closing one is dropped here.
    fn finish_field(&mut self, start: usize, quoted: bool) -> Span {
        let span = Span {
            start: ARENA | start,
            len: (self.arena.len() - start) as u32,
        };
        if !quoted {
            return self.unquote_trimmed(span);
        }
        if self.o.escape_double {
            return span;
        }
        let quote = self.o.quote.unwrap_or(0);
        let field = &self.arena[start..];
        let field = if self.o.trim_ws {
            trim_tabs(field)
        } else {
            field
        };
        let out = drop_closing_quote(field, quote).to_vec();
        self.arena.truncate(start);
        self.arena.extend_from_slice(&out);
        Span {
            start: ARENA | start,
            len: out.len() as u32,
        }
    }

    fn bytes(&self, span: Span) -> &[u8] {
        view(&self.text, &self.arena, span)
    }
}

fn replace_crlf(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        match memchr::memchr(b'\r', &text[i..]) {
            None => {
                out.extend_from_slice(&text[i..]);
                break;
            }
            Some(k) => {
                out.extend_from_slice(&text[i..i + k]);
                i += k;
                if i + 1 < text.len() && text[i + 1] == b'\n' {
                    out.push(b'\n');
                    i += 2;
                } else {
                    out.push(b'\r');
                    i += 1;
                }
            }
        }
    }
    out
}

/// PHP trim() with its default characters.
fn php_trim(s: &[u8]) -> &[u8] {
    let t = |b: &u8| matches!(*b, b' ' | b'\t' | b'\n' | b'\r' | 0 | 0x0b);
    let start = s.iter().position(|b| !t(b)).unwrap_or(s.len());
    let end = s.iter().rposition(|b| !t(b)).map_or(start, |i| i + 1);
    &s[start..end]
}

/// trim($v, " \t").
fn trim_tabs(s: &[u8]) -> &[u8] {
    let t = |b: &u8| matches!(*b, b' ' | b'\t');
    let start = s.iter().position(|b| !t(b)).unwrap_or(s.len());
    let end = s.iter().rposition(|b| !t(b)).map_or(start, |i| i + 1);
    &s[start..end]
}

fn drop_closing_quote(field: &[u8], quote: u8) -> &[u8] {
    match field.split_last() {
        Some((&last, rest)) if last == quote => rest,
        _ => field,
    }
}

/// trim($v, " \t") on a span.
fn trim_span(text: &[u8], span: Span) -> Span {
    let mut a = 0usize;
    let mut b = span.len as usize;
    while a < b && matches!(text[a], b' ' | b'\t') {
        a += 1;
    }
    while b > a && matches!(text[b - 1], b' ' | b'\t') {
        b -= 1;
    }
    Span {
        start: span.start + a,
        len: (b - a) as u32,
    }
}

// ---------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------

/// Read delimited text. Errors are option errors.
pub fn read(data: Vec<u8>, o: &Options) -> Result<Table, String> {
    read_from(Box::new(io::Cursor::new(data)), o)
}

/// Read delimited text from a stream. Errors are option and input errors.
pub fn read_from(source: Box<dyn Read + Send>, o: &Options) -> Result<Table, String> {
    let mut reader = Reader::new(source, o.clone())?;
    let (columns, problems) = match reader.batch(usize::MAX).map_err(input_error)? {
        Some(batch) => (batch.columns, batch.problems),
        None => (reader.header_only(), Vec::new()),
    };
    let tok = reader.tok;
    Ok(Table {
        names: reader.names,
        columns,
        problems,
        text: tok.text,
        arena: tok.arena,
    })
}

fn input_error(e: io::Error) -> String {
    format!("cannot read the input: {e}")
}

/// Reads delimited text a chunk of rows at a time, holding only the current
/// chunk, as phibble's DataFrame::csvChunks() reads it. Types not given are
/// guessed from the first chunk's rows and every later chunk is parsed as
/// those types. Problem rows are numbered from the start of the input.
pub struct Reader {
    tok: Tokenizer,
    o: Arc<Options>,
    names: Vec<Vec<u8>>,
    /// The first record, when there is no header line: the first data row.
    first: Option<Vec<Vec<u8>>>,
    /// Data rows read so far.
    count: usize,
    /// Whether the last record read ended inside an open quote.
    unterminated: bool,
    done: bool,
    /// The type and grouping of each column, fixed by the first chunk.
    codes: Option<Vec<(TypeCode, bool)>>,
    fields: Vec<Span>,
    joined: Vec<u8>,
}

/// One chunk's columns and problems; resolve string spans with [`Chunk::bytes`].
pub struct Chunk<'a> {
    /// The 1-based index of the chunk's first row.
    pub position: usize,
    pub rows: usize,
    /// None for skipped columns.
    pub columns: Vec<Option<Column>>,
    pub problems: Vec<String>,
    text: &'a [u8],
    arena: &'a [u8],
}

impl Chunk<'_> {
    pub fn bytes(&self, span: Span) -> &[u8] {
        view(self.text, self.arena, span)
    }
}

struct Batch {
    position: usize,
    rows: usize,
    columns: Vec<Option<Column>>,
    problems: Vec<String>,
}

impl Reader {
    /// Reads up to the header (or the first record, which sets the width).
    pub fn new(source: Box<dyn Read + Send>, o: Options) -> Result<Self, String> {
        o.validate()?;
        let o = Arc::new(o);
        let mut tok = Tokenizer::new(source, Arc::clone(&o));
        let mut fields = Vec::new();
        let mut reader = loop {
            match tok.next(&mut fields).map_err(input_error)? {
                Record::Eof => {
                    break Self::with(tok, o, Vec::new(), None);
                }
                Record::Blank => continue,
                Record::Fields => {}
            }
            // A UTF-8 byte order mark on the first field.
            if tok.bytes(fields[0]).starts_with(b"\xEF\xBB\xBF") {
                fields[0].start += 3;
                fields[0].len -= 3;
            }
            let raw: Vec<Vec<u8>> = fields.iter().map(|&f| tok.bytes(f).to_vec()).collect();
            let unterminated = tok.unterminated;
            let (names, first) = if o.col_names {
                let names = raw
                    .into_iter()
                    .map(|b| if o.trim_ws { php_trim(&b).to_vec() } else { b })
                    .collect();
                (names, None)
            } else {
                let names = (1..=raw.len())
                    .map(|i| format!("X{i}").into_bytes())
                    .collect();
                (names, Some(raw))
            };
            let mut reader = Self::with(tok, o, names, first);
            reader.unterminated = unterminated;
            break reader;
        };
        reader.done = reader.names.is_empty();
        reader.fields = fields;
        Ok(reader)
    }

    fn with(
        tok: Tokenizer,
        o: Arc<Options>,
        names: Vec<Vec<u8>>,
        first: Option<Vec<Vec<u8>>>,
    ) -> Self {
        Self {
            tok,
            o,
            names,
            first,
            count: 0,
            unterminated: false,
            done: false,
            codes: None,
            fields: Vec::new(),
            joined: Vec::new(),
        }
    }

    /// Column names: the header's, or X1..Xn without one. Empty for empty input.
    pub fn names(&self) -> &[Vec<u8>] {
        &self.names
    }

    /// The next chunk of at most `size` rows, or None when no rows remain. A
    /// chunk with no rows is returned only to carry problems found after the
    /// last row (an open quote at the end of an input without data rows).
    pub fn next_chunk(&mut self, size: usize) -> io::Result<Option<Chunk<'_>>> {
        let size = size.max(1);
        Ok(self.batch(size)?.map(|b| Chunk {
            position: b.position,
            rows: b.rows,
            columns: b.columns,
            problems: b.problems,
            text: &self.tok.text,
            arena: &self.tok.arena,
        }))
    }

    fn batch(&mut self, size: usize) -> io::Result<Option<Batch>> {
        if self.done {
            return Ok(None);
        }
        self.tok.release();
        let o = Arc::clone(&self.o);
        let width = self.names.len();
        let mut rows = Rows {
            cells: vec![Vec::new(); width],
            count: self.count,
        };
        let start = self.count;
        let limit = size.min(o.n_max.map_or(usize::MAX, |m| m - start));
        let mut problems = Vec::new();
        if limit > 0
            && let Some(first) = self.first.take()
        {
            self.fields.clear();
            for bytes in first {
                let start = ARENA | self.tok.arena.len();
                self.tok.arena.extend_from_slice(&bytes);
                self.fields.push(Span {
                    start,
                    len: bytes.len() as u32,
                });
            }
            rows.take(
                &mut self.tok,
                &self.fields,
                &o,
                &mut self.joined,
                &mut problems,
            );
        }
        while rows.count - start < limit {
            match self.tok.next(&mut self.fields)? {
                Record::Eof => break,
                Record::Blank => {}
                Record::Fields => {
                    rows.take(
                        &mut self.tok,
                        &self.fields,
                        &o,
                        &mut self.joined,
                        &mut problems,
                    );
                    self.unterminated = self.tok.unterminated;
                }
            }
        }
        self.count = rows.count;
        let capped = o.n_max.is_some_and(|m| self.count >= m);
        // Past any blank lines, to know whether records remain.
        let exhausted = !capped && self.tok.at_end()?;
        if exhausted && self.unterminated {
            problems.push(format!(
                "row {}: unterminated quoted field at end of file",
                self.count + 1
            ));
        }
        self.done = exhausted || capped;
        let n = self.count - start;
        if n == 0 && problems.is_empty() {
            return Ok(None);
        }
        let columns = if n == 0 {
            self.header_only()
        } else {
            self.columns(rows.cells, n)
        };
        Ok(Some(Batch {
            position: start + 1,
            rows: n,
            columns,
            problems,
        }))
    }

    fn code(&self, i: usize) -> TypeCode {
        self.o.types.get(i).copied().unwrap_or(TypeCode::Guess)
    }

    /// The columns of an input without data rows. As phibble does: readr reads
    /// the columns of a header-only file as text, so zero data rows gives
    /// string before the guesser's empty-column rule applies.
    fn header_only(&self) -> Vec<Option<Column>> {
        (0..self.names.len())
            .map(|i| match self.code(i) {
                TypeCode::Skip => None,
                TypeCode::Guess => Some(TypeCode::String),
                code => Some(code),
            })
            .map(|code| {
                code.map(|code| {
                    let values = match code {
                        TypeCode::Bool => Values::Bool(Vec::new()),
                        TypeCode::Int => Values::Int(Vec::new()),
                        TypeCode::Double => Values::Float(Vec::new()),
                        TypeCode::Date => Values::Date(Vec::new()),
                        TypeCode::DateTime => Values::DateTime(Vec::new()),
                        _ => Values::Str(Vec::new()),
                    };
                    Column { values, failed: 0 }
                })
            })
            .collect()
    }

    fn columns(&mut self, cells: Vec<Vec<Span>>, count: usize) -> Vec<Option<Column>> {
        let parser = Parser {
            decimal: self.o.decimal_mark,
            grouping: self.o.grouping(),
        };
        let tok = &self.tok;
        let view = |s: Span| -> Option<&[u8]> { if s.is_na() { None } else { Some(tok.bytes(s)) } };
        let codes = self.codes.get_or_insert_with(|| {
            // Types: guessed from the first chunk, then fixed for the rest.
            let sample = sample_rows(count, self.o.guess_max);
            cells
                .iter()
                .enumerate()
                .map(
                    |(i, column)| match self.o.types.get(i).copied().unwrap_or(TypeCode::Guess) {
                        TypeCode::Guess => {
                            (parser.guess(sample.iter().map(|&r| view(column[r]))), true)
                        }
                        code => (code, false),
                    },
                )
                .collect()
        });
        cells
            .into_iter()
            .zip(codes.iter())
            .map(|(column, &(code, grouping))| match code {
                TypeCode::Skip => None,
                TypeCode::String | TypeCode::Guess => Some(Column {
                    values: Values::Str(column),
                    failed: 0,
                }),
                _ => Some(parser.parse(
                    column.iter().map(|&s| view(s)),
                    code,
                    grouping,
                    column.len(),
                )),
            })
            .collect()
    }
}

/// Data rows, column by column.
struct Rows {
    cells: Vec<Vec<Span>>,
    /// Data rows read from the start of the input.
    count: usize,
}

impl Rows {
    fn take(
        &mut self,
        tok: &mut Tokenizer,
        fields: &[Span],
        o: &Options,
        joined: &mut Vec<u8>,
        problems: &mut Vec<String>,
    ) {
        let width = self.cells.len();
        self.count += 1;
        let n = fields.len();
        if n != width {
            // Rows are named as readr::problems() names them: data row index + 1.
            let row = self.count + 1;
            problems.push(format!("row {row}: expected {width} columns, got {n}"));
        }
        let kept = n.min(width);
        for (i, &field) in fields.iter().enumerate().take(kept) {
            let field = if i == width - 1 && n > width {
                // The extra fields, joined by the delimiter, go into the last column.
                joined.clear();
                for (k, &f) in fields[width - 1..].iter().enumerate() {
                    if k > 0 {
                        joined.push(o.delimiter);
                    }
                    joined.extend_from_slice(tok.bytes(f));
                }
                let start = ARENA | tok.arena.len();
                tok.arena.extend_from_slice(joined);
                Span {
                    start,
                    len: joined.len() as u32,
                }
            } else {
                field
            };
            self.cells[i].push(clean(tok, field, o));
        }
        for column in self.cells.iter_mut().skip(kept) {
            column.push(Span::NA);
        }
    }
}

/// Trim and map na strings to NA.
fn clean(tok: &Tokenizer, field: Span, o: &Options) -> Span {
    let field = if o.trim_ws {
        trim_span(tok.bytes(field), field)
    } else {
        field
    };
    let bytes = tok.bytes(field);
    if o.na.iter().any(|na| na.as_slice() == bytes) {
        Span::NA
    } else {
        field
    }
}

fn read_up_to(source: &mut dyn Read, size: usize) -> io::Result<Vec<u8>> {
    let mut block = Vec::with_capacity(size.min(1 << 22));
    source.take(size as u64).read_to_end(&mut block)?;
    Ok(block)
}

/// vroom's guessing rows: all when there are at most guess_max, else
/// guess_max - 1 evenly spaced rows and the last.
fn sample_rows(count: usize, guess_max: usize) -> Vec<usize> {
    if count <= guess_max {
        return (0..count).collect();
    }
    let step = count / guess_max;
    let mut rows: Vec<usize> = (0..guess_max - 1).map(|i| i * step).collect();
    rows.push(count - 1);
    rows
}

// ---------------------------------------------------------------------
// Values (phibble's TextParser)
// ---------------------------------------------------------------------

const TRUE: [&[u8]; 5] = [b"T", b"TRUE", b"true", b"True", b"t"];
const FALSE: [&[u8]; 5] = [b"F", b"FALSE", b"false", b"False", b"f"];

/// What a PCRE pattern ending in `$` (no D modifier) sees: the text with
/// one trailing newline removed.
fn body(v: &[u8]) -> &[u8] {
    v.strip_suffix(b"\n").unwrap_or(v)
}

fn digits(v: &[u8]) -> bool {
    !v.is_empty() && v.iter().all(u8::is_ascii_digit)
}

/// readr reads unsigned numbers with a leading zero ("007", "00.5", "0e5") as text.
fn leading_zero(v: &[u8]) -> bool {
    v.len() > 1 && v[0] == b'0' && (v[1].is_ascii_digit() || v[1] == b'e' || v[1] == b'E')
}

fn sign(v: &[u8]) -> (bool, &[u8]) {
    match v.first() {
        Some(b'-') => (true, &v[1..]),
        Some(b'+') => (false, &v[1..]),
        _ => (false, v),
    }
}

/// `^[+-]?(inf|nan)$`i
fn inf_nan(v: &[u8]) -> Option<f64> {
    let (neg, rest) = sign(v);
    if rest.eq_ignore_ascii_case(b"nan") {
        Some(f64::NAN)
    } else if rest.eq_ignore_ascii_case(b"inf") {
        Some(if neg {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        })
    } else {
        None
    }
}

fn without(v: &[u8], mark: Option<u8>) -> Cow<'_, [u8]> {
    match mark {
        Some(m) if v.contains(&m) => Cow::Owned(v.iter().copied().filter(|&b| b != m).collect()),
        _ => Cow::Borrowed(v),
    }
}

/// Scan `[eE][+-]?\d+` at i; returns the end, or None when malformed.
fn exponent(v: &[u8], mut i: usize) -> Option<usize> {
    if i < v.len() && (v[i] == b'e' || v[i] == b'E') {
        i += 1;
        if i < v.len() && (v[i] == b'+' || v[i] == b'-') {
            i += 1;
        }
        let start = i;
        while i < v.len() && v[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return None;
        }
    }
    Some(i)
}

/// PHP is_numeric().
fn is_numeric(v: &[u8]) -> bool {
    let ws = |b: u8| matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c);
    let mut i = 0;
    while i < v.len() && ws(v[i]) {
        i += 1;
    }
    if i < v.len() && (v[i] == b'+' || v[i] == b'-') {
        i += 1;
    }
    let int_start = i;
    while i < v.len() && v[i].is_ascii_digit() {
        i += 1;
    }
    if i > int_start {
        if i < v.len() && v[i] == b'.' {
            i += 1;
            while i < v.len() && v[i].is_ascii_digit() {
                i += 1;
            }
        }
    } else if i + 1 < v.len() && v[i] == b'.' && v[i + 1].is_ascii_digit() {
        i += 1;
        while i < v.len() && v[i].is_ascii_digit() {
            i += 1;
        }
    } else {
        return false;
    }
    let Some(mut i) = exponent(v, i) else {
        return false;
    };
    while i < v.len() && ws(v[i]) {
        i += 1;
    }
    i == v.len()
}

fn to_f64(v: &[u8]) -> Option<f64> {
    std::str::from_utf8(v).ok()?.parse::<f64>().ok()
}

fn checkdate(y: i64, m: i64, d: i64) -> bool {
    if !(1..=32767).contains(&y) || !(1..=12).contains(&m) || d < 1 {
        return false;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    d <= days[(m - 1) as usize]
}

/// Days since 1970-01-01 (proleptic Gregorian).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn num(v: &[u8]) -> i64 {
    v.iter().fold(0, |a, &b| a * 10 + i64::from(b - b'0'))
}

/// `^(\d{4})[-/](\d{2})[-/](\d{2})$` on the body.
fn date_parts(v: &[u8]) -> Option<(i64, i64, i64)> {
    let v = body(v);
    if v.len() != 10 || !matches!(v[4], b'-' | b'/') || !matches!(v[7], b'-' | b'/') {
        return None;
    }
    let (y, m, d) = (&v[0..4], &v[5..7], &v[8..10]);
    (digits(y) && digits(m) && digits(d)).then(|| (num(y), num(m), num(d)))
}

struct DateTimeParts {
    y: i64,
    m: i64,
    d: i64,
    hour: i64,
    minute: i64,
    second: i64,
    micros: i64,
    /// Sign, hours, minutes.
    offset: Option<(bool, i64, i64)>,
}

/// phibble's DATETIME pattern:
/// `^(\d{4})-(\d{2})-(\d{2})(?:[T ](\d{2})(?::(\d{2})(?::(\d{2})(\.\d+)?)?)?)?(Z|[+-]\d{2}(?::?\d{2})?)?$`
fn datetime_parts(v: &[u8]) -> Option<DateTimeParts> {
    let v = body(v);
    let two = |i: usize| -> Option<i64> {
        (i + 2 <= v.len() && digits(&v[i..i + 2])).then(|| num(&v[i..i + 2]))
    };
    if v.len() < 10 || !digits(&v[0..4]) || v[4] != b'-' || v[7] != b'-' {
        return None;
    }
    let mut p = DateTimeParts {
        y: num(&v[0..4]),
        m: two(5)?,
        d: two(8)?,
        hour: 0,
        minute: 0,
        second: 0,
        micros: 0,
        offset: None,
    };
    let mut i = 10;
    if i < v.len() && (v[i] == b'T' || v[i] == b' ') {
        p.hour = two(i + 1)?;
        i += 3;
        if i < v.len() && v[i] == b':' && two(i + 1).is_some() {
            p.minute = two(i + 1)?;
            i += 3;
            if i < v.len() && v[i] == b':' && two(i + 1).is_some() {
                p.second = two(i + 1)?;
                i += 3;
                if i + 1 < v.len() && v[i] == b'.' && v[i + 1].is_ascii_digit() {
                    i += 1;
                    let start = i;
                    while i < v.len() && v[i].is_ascii_digit() {
                        i += 1;
                    }
                    let frac = &v[start..i.min(start + 6)];
                    p.micros = num(frac) * 10i64.pow(6 - frac.len() as u32);
                }
            }
        }
    }
    if i < v.len() {
        if v[i] == b'Z' {
            p.offset = Some((false, 0, 0));
            i += 1;
        } else if v[i] == b'+' || v[i] == b'-' {
            let neg = v[i] == b'-';
            let hours = two(i + 1)?;
            i += 3;
            let mut minutes = 0;
            if i < v.len() && v[i] == b':' {
                minutes = two(i + 1)?;
                i += 3;
            } else if two(i).is_some() {
                minutes = two(i)?;
                i += 2;
            }
            p.offset = Some((neg, hours, minutes));
        }
    }
    (i == v.len()).then_some(p)
}

struct Parser {
    decimal: u8,
    grouping: Option<u8>,
}

impl Parser {
    fn guess<'a>(&self, values: impl Iterator<Item = Option<&'a [u8]>>) -> TypeCode {
        let present: Vec<&[u8]> = values.flatten().collect();
        if present.is_empty() {
            return TypeCode::Bool;
        }
        let all = |test: &dyn Fn(&[u8]) -> bool| present.iter().all(|v| test(v));
        if all(&|v| TRUE.contains(&v) || FALSE.contains(&v)) {
            TypeCode::Bool
        } else if all(&|v| self.guesses_as_int(v)) {
            TypeCode::Int
        } else if all(&|v| self.guesses_as_number(v)) {
            TypeCode::Double
        } else if all(&|v| date_parts(v).is_some()) {
            TypeCode::Date
        } else if all(&|v| datetime_parts(v).is_some()) {
            TypeCode::DateTime
        } else {
            TypeCode::String
        }
    }

    /// `^[+-]?\d(?:[\dG]*\d)?$`, no unsigned leading zero, fits in an int.
    fn guesses_as_int(&self, v: &[u8]) -> bool {
        let (_, rest) = sign(body(v));
        let ok = |b: &u8| b.is_ascii_digit() || Some(*b) == self.grouping;
        !rest.is_empty()
            && rest[0].is_ascii_digit()
            && rest[rest.len() - 1].is_ascii_digit()
            && rest.iter().all(ok)
            && !leading_zero(v)
            && self.parse_int(v, true).is_some()
    }

    /// `^[+-]?(inf|nan)$`i, or
    /// `^[+-]?(?:\d[\dG]*(?:D\d*)?|D\d+)(?:[eE][+-]?\d+)?$` without a leading zero.
    fn guesses_as_number(&self, v: &[u8]) -> bool {
        let b = body(v);
        if inf_nan(b).is_some() {
            return true;
        }
        let (_, r) = sign(b);
        let mut i = 0;
        if i < r.len() && r[i].is_ascii_digit() {
            i += 1;
            while i < r.len() && (r[i].is_ascii_digit() || Some(r[i]) == self.grouping) {
                i += 1;
            }
            if i < r.len() && r[i] == self.decimal {
                i += 1;
                while i < r.len() && r[i].is_ascii_digit() {
                    i += 1;
                }
            }
        } else if i < r.len() && r[i] == self.decimal {
            i += 1;
            let start = i;
            while i < r.len() && r[i].is_ascii_digit() {
                i += 1;
            }
            if i == start {
                return false;
            }
        } else {
            return false;
        }
        exponent(r, i) == Some(r.len()) && !leading_zero(v)
    }

    fn parse_bool(v: &[u8]) -> Option<bool> {
        if TRUE.contains(&v) || v == b"1" {
            Some(true)
        } else if FALSE.contains(&v) || v == b"0" {
            Some(false)
        } else {
            None
        }
    }

    fn parse_int(&self, v: &[u8], grouping: bool) -> Option<i64> {
        if v.len() < 19 {
            if digits(v) {
                return Some(num(v));
            }
            if v.len() > 1 && v[0] == b'-' && digits(&v[1..]) {
                return Some(-num(&v[1..]));
            }
        }
        let w = if grouping {
            without(v, self.grouping)
        } else {
            Cow::Borrowed(v)
        };
        let b = body(&w);
        let (neg, ds) = sign(b);
        if !digits(ds) {
            return None;
        }
        // ltrim('0') ?: '0', then filter_var(FILTER_VALIDATE_INT), which
        // fails on the lone "\n" left from "0\n".
        let stripped = &ds[ds.iter().position(|&c| c != b'0').unwrap_or(ds.len())..];
        if stripped.is_empty() {
            return if b.len() < w.len() { None } else { Some(0) };
        }
        if stripped.len() > 19 {
            return None;
        }
        let magnitude: i128 = stripped
            .iter()
            .fold(0i128, |a, &c| a * 10 + i128::from(c - b'0'));
        let value = if neg { -magnitude } else { magnitude };
        i64::try_from(value).ok()
    }

    fn parse_double(&self, v: &[u8], grouping: bool) -> Option<f64> {
        if self.decimal == b'.' && is_numeric(v) && !matches!(v[0], b' ' | b'\t' | b'\n') {
            return to_f64(trim_numeric_ws(v));
        }
        if let Some(x) = inf_nan(body(v)) {
            return Some(x);
        }
        let w = if grouping {
            without(v, self.grouping)
        } else {
            Cow::Borrowed(v)
        };
        let b = body(&w);
        let (_, r) = sign(b);
        let mut i = 0;
        let int_start = i;
        while i < r.len() && r[i].is_ascii_digit() {
            i += 1;
        }
        if i > int_start {
            if i < r.len() && r[i] == self.decimal {
                i += 1;
                while i < r.len() && r[i].is_ascii_digit() {
                    i += 1;
                }
            }
        } else if i < r.len() && r[i] == self.decimal {
            i += 1;
            let start = i;
            while i < r.len() && r[i].is_ascii_digit() {
                i += 1;
            }
            if i == start {
                return None;
            }
        } else {
            return None;
        }
        if exponent(r, i) != Some(r.len()) {
            return None;
        }
        let text: Vec<u8> = b
            .iter()
            .map(|&c| if c == self.decimal { b'.' } else { c })
            .collect();
        to_f64(&text)
    }

    fn parse_date(v: &[u8]) -> Option<i64> {
        let (y, m, d) = date_parts(v)?;
        checkdate(y, m, d).then(|| days_from_civil(y, m, d))
    }

    fn parse_datetime(v: &[u8]) -> Option<i64> {
        let p = datetime_parts(v)?;
        if !checkdate(p.y, p.m, p.d) || p.hour > 23 || p.minute > 59 || p.second > 59 {
            return None;
        }
        let offset = match p.offset {
            None => 0,
            // DateTimeImmutable accepts offsets up to +-24:59.
            Some((_, h, m)) if h > 24 || m > 59 => return None,
            Some((neg, h, m)) => (h * 3600 + m * 60) * if neg { -1 } else { 1 },
        };
        let seconds =
            days_from_civil(p.y, p.m, p.d) * 86_400 + p.hour * 3600 + p.minute * 60 + p.second
                - offset;
        Some(seconds * 1_000_000 + p.micros)
    }

    fn parse<'a>(
        &self,
        values: impl Iterator<Item = Option<&'a [u8]>>,
        code: TypeCode,
        grouping: bool,
        n: usize,
    ) -> Column {
        let mut failed = 0;
        fn collect<'a, T>(
            values: impl Iterator<Item = Option<&'a [u8]>>,
            n: usize,
            failed: &mut usize,
            f: impl Fn(&[u8]) -> Option<T>,
        ) -> Vec<Option<T>> {
            let mut out = Vec::with_capacity(n);
            for v in values {
                out.push(match v {
                    None => None,
                    Some(v) => {
                        let parsed = f(v);
                        if parsed.is_none() {
                            *failed += 1;
                        }
                        parsed
                    }
                });
            }
            out
        }
        let values = match code {
            TypeCode::Bool => Values::Bool(collect(values, n, &mut failed, Self::parse_bool)),
            TypeCode::Int => Values::Int(collect(values, n, &mut failed, |v| {
                self.parse_int(v, grouping)
            })),
            TypeCode::Double => Values::Float(collect(values, n, &mut failed, |v| {
                self.parse_double(v, grouping)
            })),
            TypeCode::Date => Values::Date(collect(values, n, &mut failed, Self::parse_date)),
            TypeCode::DateTime => {
                Values::DateTime(collect(values, n, &mut failed, Self::parse_datetime))
            }
            TypeCode::String | TypeCode::Guess | TypeCode::Skip => {
                unreachable!("handled by the caller")
            }
        };
        Column { values, failed }
    }
}

/// Strip PHP's numeric-string whitespace from both ends.
fn trim_numeric_ws(v: &[u8]) -> &[u8] {
    let ws = |b: &u8| matches!(*b, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c);
    let start = v.iter().position(|b| !ws(b)).unwrap_or(v.len());
    let end = v.iter().rposition(|b| !ws(b)).map_or(start, |i| i + 1);
    &v[start..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str, o: &Options) -> Table {
        read(text.as_bytes().to_vec(), o).unwrap()
    }

    fn strings(t: &Table, i: usize) -> Vec<Option<String>> {
        match &t.columns[i].as_ref().unwrap().values {
            Values::Str(v) => v
                .iter()
                .map(|&s| (!s.is_na()).then(|| String::from_utf8_lossy(t.bytes(s)).into_owned()))
                .collect(),
            other => panic!("not text: {other:?}"),
        }
    }

    #[test]
    fn guesses_types() {
        let t = run(
            "a,b,c,d,e,f\n1,1.5,x,2020-01-02,2020-01-02 03:04:05,T\n-07,2,y,2020-01-03,2020-01-02T00:00Z,f\n",
            &Options::default(),
        );
        let types: Vec<_> = t
            .columns
            .iter()
            .map(|c| c.as_ref().unwrap().values.type_name())
            .collect();
        assert_eq!(
            types,
            ["int", "float", "string", "date", "datetime", "bool"]
        );
        match &t.columns[4].as_ref().unwrap().values {
            Values::DateTime(v) => assert_eq!(v[1], Some(1_577_923_200_000_000)),
            _ => unreachable!(),
        }
    }

    #[test]
    fn leading_zeros_and_grouping() {
        let p = Parser {
            decimal: b'.',
            grouping: Some(b','),
        };
        assert!(!p.guesses_as_int(b"007"));
        assert!(p.guesses_as_int(b"-07"));
        assert!(p.guesses_as_int(b"0"));
        assert!(p.guesses_as_int(b"1,234"));
        assert!(!p.guesses_as_number(b"0e5"));
        assert!(!p.guesses_as_number(b"00.5"));
        assert!(p.guesses_as_number(b"1."));
        assert!(p.guesses_as_number(b"-Inf"));
        assert_eq!(p.parse_int(b"007", false), Some(7));
        assert_eq!(p.parse_int(b"0\n", false), None);
        assert_eq!(p.parse_int(b"9223372036854775808", false), None);
        assert_eq!(p.parse_double(b"1.", false), Some(1.0));
        assert_eq!(p.parse_double(b"1,000.5", true), Some(1000.5));
        assert_eq!(p.parse_double(b"1,000.5", false), None);
    }

    /// Every column as text, or the type name for typed columns.
    fn texts(t: &Table) -> Vec<Vec<Option<String>>> {
        (0..t.columns.len()).map(|i| strings(t, i)).collect()
    }

    #[test]
    fn independent_of_block_size() {
        let inputs = [
            "\u{feff}a,b\r\n\"x,\r\n\"\"y\"\"\",1\r\n\r\r\n\"multi\nline\",2,3\r\nz\r\n\"open",
            "a;b\rx;1\r\ry;\"q\r\"\r",
            "junk\nmore\n# c\na,b\n#x\n1,2\n  # y\n3,4",
        ];
        for (n, input) in inputs.iter().enumerate() {
            let mut o = Options {
                types: vec![TypeCode::String; 2],
                ..Options::default()
            };
            if n == 1 {
                o.delimiter = b';';
            }
            if n == 2 {
                o.skip = 2;
                o.comment = b"#".to_vec();
            }
            let whole = run(input, &o);
            for block_size in 1..40 {
                o.block_size = block_size;
                let t = run(input, &o);
                assert_eq!(
                    (&t.names, texts(&t), &t.problems),
                    (&whole.names, texts(&whole), &whole.problems),
                    "input {n}, block {block_size}"
                );
            }
        }
    }

    #[test]
    fn chunks_follow_the_whole_read() {
        let input = "a,b\n1,x\n\n2,y\n3\n4,z,w\n5,v\n";
        let whole = run(input, &Options::default());
        for size in 1..7 {
            for block_size in [1, 2, 5, 1 << 22] {
                let o = Options {
                    block_size,
                    guess_max: size,
                    ..Options::default()
                };
                let mut reader =
                    Reader::new(Box::new(io::Cursor::new(input.as_bytes().to_vec())), o).unwrap();
                assert_eq!(reader.names(), &whole.names[..]);
                let (mut next, mut problems, mut letters) = (1, Vec::new(), Vec::new());
                while let Some(chunk) = reader.next_chunk(size).unwrap() {
                    assert_eq!(chunk.position, next);
                    assert!(chunk.rows <= size);
                    next += chunk.rows;
                    problems.extend(chunk.problems.iter().cloned());
                    let Some(Column {
                        values: Values::Str(v),
                        ..
                    }) = &chunk.columns[1]
                    else {
                        panic!()
                    };
                    letters.extend(
                        v.iter()
                            .map(|&s| (!s.is_na()).then(|| chunk.bytes(s).to_vec())),
                    );
                }
                assert_eq!(next, 6, "size {size}");
                assert_eq!(problems, whole.problems);
                let want: Vec<_> = strings(&whole, 1)
                    .into_iter()
                    .map(|s| s.map(String::into_bytes))
                    .collect();
                assert_eq!(letters, want);
            }
        }
    }

    #[test]
    fn quoting_ragged_and_bom() {
        let t = run(
            "\u{feff}a,b\n\"x,\"\"y\"\"\",1\n\"multi\nline\",2,3\nz\n",
            &Options::default(),
        );
        assert_eq!(t.names, [b"a".to_vec(), b"b".to_vec()]);
        assert_eq!(
            strings(&t, 0),
            [
                Some("x,\"y\"".into()),
                Some("multi\nline".into()),
                Some("z".into())
            ]
        );
        assert_eq!(
            t.problems,
            [
                "row 3: expected 2 columns, got 3",
                "row 4: expected 2 columns, got 1"
            ]
        );
    }

    #[test]
    fn comments_and_escapes() {
        let o = Options {
            comment: b"#".to_vec(),
            ..Options::default()
        };
        let t = run("# head\nx,y\n1,2 # c\n#\n3,4\n", &o);
        assert_eq!(t.names, [b"x".to_vec(), b"y".to_vec()]);
        assert_eq!(t.columns[1].as_ref().unwrap().values.len(), 2);
        let o = Options {
            escape_backslash: true,
            escape_double: false,
            ..Options::default()
        };
        let t = run("a\n\"say \\\"hi\\\"\"\n", &o);
        assert_eq!(strings(&t, 0), [Some("say \"hi\"".into())]);
    }

    #[test]
    fn empty_and_header_only() {
        assert!(run("", &Options::default()).names.is_empty());
        let t = run("a,b\n", &Options::default());
        assert_eq!(t.columns[0].as_ref().unwrap().values.type_name(), "string");
        let o = Options {
            n_max: Some(0),
            ..Options::default()
        };
        let t = run("a,b\n1,2\n", &o);
        assert_eq!(t.columns[0].as_ref().unwrap().values.type_name(), "string");
        let o = Options {
            types: vec![TypeCode::Bool],
            ..Options::default()
        };
        let t = run("a,b\n", &o);
        assert_eq!(t.columns[0].as_ref().unwrap().values.type_name(), "bool");
    }

    #[test]
    fn civil_days() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(1969, 12, 31), -1);
        assert_eq!(days_from_civil(2000, 3, 1), 11017);
    }
}
