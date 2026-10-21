//! A delimited-text reader that reproduces phibble's PHP CSV reader (which
//! follows readr) byte for byte: the same records, names, guessed types,
//! parsed values and problem messages.
//!
//! phibble tokenizes with PHP's `fgetcsv()` when it can (no comment, `""`
//! escapes, no backslash escapes, a quote character) and with a
//! character-level reader otherwise. Both are reproduced here, including
//! their corner cases; the values and types follow phibble's `TextParser`.

use std::borrow::Cow;

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

/// A field: bytes `start..start + len` of the reader's text, where offsets
/// past the text address its arena. `len == NA_LEN` marks a missing value.
#[derive(Clone, Copy, Debug)]
pub struct Span {
    start: usize,
    len: u32,
}

const NA_LEN: u32 = u32::MAX;

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
        let end = span.start + span.len as usize;
        if span.start >= self.text.len() {
            let base = self.text.len();
            &self.arena[span.start - base..end - base]
        } else {
            &self.text[span.start..end]
        }
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

struct Tokenizer<'o> {
    text: Vec<u8>,
    arena: Vec<u8>,
    pos: usize,
    /// The key phibble gives the record (1-based, counting skipped lines).
    line: usize,
    o: &'o Options,
    /// fgetcsv() emulation, or phibble's character-level reader.
    fast: bool,
    /// Set when the last record ended inside an open quote.
    unterminated: bool,
    /// Whether a record has been produced yet, for the leading comment rule.
    emitted: bool,
}

/// C isspace() in the "C" locale.
fn c_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

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

    fn finish(self, text_len: usize, arena: &[u8]) -> Span {
        match (self.owned, self.slice) {
            (Some(start), _) => Span {
                start: text_len + start,
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

impl<'o> Tokenizer<'o> {
    fn new(data: Vec<u8>, o: &'o Options) -> Self {
        let fast =
            o.comment.is_empty() && o.escape_double && !o.escape_backslash && o.quote.is_some();
        // Lines skipped with fgets() before anything else.
        let mut pos = 0;
        let mut line = 0;
        let mut exhausted = false;
        for _ in 0..o.skip {
            if pos >= data.len() {
                exhausted = true;
                break;
            }
            pos = memchr::memchr(b'\n', &data[pos..]).map_or(data.len(), |i| pos + i + 1);
            line += 1;
        }
        let mut text = if exhausted { Vec::new() } else { data };
        pos = pos.min(text.len());
        // As phibble does before tokenizing: drop a byte order mark at the
        // start of the file and read CR as the line ending when there is no LF.
        if line == 0 && text[pos..].starts_with(b"\xEF\xBB\xBF") {
            pos += 3;
        }
        let rest = &text[pos..];
        let cr_only =
            memchr::memchr(b'\n', rest).is_none() && memchr::memchr(b'\r', rest).is_some();
        if cr_only {
            text = rest
                .iter()
                .map(|&b| if b == b'\r' { b'\n' } else { b })
                .collect();
            pos = 0;
        }
        if !fast {
            // The character-level reader works on text with CRLF folded to LF.
            text = replace_crlf(&text[pos..]);
            pos = 0;
        }
        Self {
            text,
            arena: Vec::new(),
            pos,
            line,
            o,
            fast,
            unterminated: false,
            emitted: false,
        }
    }

    fn next(&mut self, fields: &mut Vec<Span>) -> Record {
        fields.clear();
        self.unterminated = false;
        if self.fast {
            self.next_fgetcsv(fields)
        } else {
            self.next_chars(fields)
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

    /// The end of a line without one trailing "\r\n", "\n" or "\r".
    fn strip_eol(&self, start: usize, end: usize) -> usize {
        let t = &self.text;
        if end > start && t[end - 1] == b'\n' {
            if end - start >= 2 && t[end - 2] == b'\r' {
                end - 2
            } else {
                end - 1
            }
        } else if end > start && t[end - 1] == b'\r' {
            end - 1
        } else {
            end
        }
    }

    /// PHP's fgetcsv($stream, null, $delimiter, $quote, '') on the C locale.
    fn next_fgetcsv(&mut self, fields: &mut Vec<Span>) -> Record {
        let len = self.text.len();
        if self.pos >= len {
            return Record::Eof;
        }
        if let Some(end) = self.blank_line(self.pos) {
            self.pos = end;
            self.line += 1;
            return Record::Blank;
        }
        let delim = self.o.delimiter;
        let quote = self.o.quote.unwrap_or(0);
        let mut buf_start = self.pos;
        let mut buf_end = self.line_end(buf_start);
        let mut limit = self.strip_eol(buf_start, buf_end);
        let mut b = buf_start;
        self.line += 1;
        loop {
            let t = &self.text;
            let inc = b < limit;
            if inc {
                let mut s = b;
                while s < buf_end && t[s] != delim && c_space(t[s]) {
                    s += 1;
                }
                if s < buf_end && t[s] == quote {
                    b = s;
                }
            }
            let more;
            if inc && t[b] == quote {
                b += 1;
                let mut hunk = b;
                let mut closing = false; // the previous byte was a quote
                let mut acc = Acc::new();
                loop {
                    if b < limit {
                        let c = self.text[b];
                        if closing {
                            if c != quote {
                                acc.push(&self.text, &mut self.arena, hunk, b - 1);
                                hunk = b;
                                break;
                            }
                            acc.push(&self.text, &mut self.arena, hunk, b);
                            b += 1;
                            hunk = b;
                            closing = false;
                        } else {
                            if c == quote {
                                closing = true;
                            }
                            b += 1;
                        }
                    } else if closing {
                        acc.push(&self.text, &mut self.arena, hunk, b - 1);
                        hunk = b;
                        break;
                    } else {
                        // The field runs on past this line, line ending included.
                        acc.push(&self.text, &mut self.arena, hunk, b);
                        acc.push(&self.text, &mut self.arena, limit, buf_end);
                        if buf_end >= len {
                            // End of file inside quotes: the field closes here.
                            self.unterminated = true;
                            hunk = limit;
                            b = limit;
                            break;
                        }
                        buf_start = buf_end;
                        buf_end = self.line_end(buf_start);
                        limit = self.strip_eol(buf_start, buf_end);
                        b = buf_start;
                        hunk = b;
                    }
                }
                // Anything between the closing quote and the delimiter is kept.
                let t = &self.text;
                b = memchr::memchr(delim, &t[b..limit]).map_or(limit, |i| b + i);
                acc.push(&self.text, &mut self.arena, hunk, b);
                more = b < limit;
                if more {
                    b += 1;
                }
                fields.push(acc.finish(len, &self.arena));
            } else {
                let hunk = b;
                b = memchr::memchr(delim, &t[b..limit]).map_or(limit, |i| b + i);
                let mut end = b;
                if end > hunk && t[end - 1] == b'\r' {
                    end -= 1;
                }
                fields.push(Span {
                    start: hunk,
                    len: (end - hunk) as u32,
                });
                more = b < limit;
                if more {
                    b += 1;
                }
            }
            if !more {
                break;
            }
        }
        self.pos = buf_end;
        Record::Fields
    }

    fn starts_with_at(&self, pos: usize, s: &[u8]) -> bool {
        !s.is_empty() && self.text[pos..].starts_with(s)
    }

    /// phibble's character-level reader (comments, backslash escapes, no quote).
    fn next_chars(&mut self, fields: &mut Vec<Span>) -> Record {
        let o = self.o;
        let len = self.text.len();
        loop {
            if self.pos >= len {
                return Record::Eof;
            }
            self.line += 1;
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
                self.line -= 1;
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
        let base = self.text.len();
        let mut field_start = self.arena.len();
        let mut in_quotes = false;
        let mut pos = self.pos;
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
                    pos += 1;
                    continue;
                }
                self.arena.push(c);
                pos += 1;
                continue;
            }
            if c == b'\n' {
                pos += 1;
                break;
            }
            if self.starts_with_at(pos, &o.comment) {
                pos = self.line_end(pos);
                break;
            }
            if c == delim {
                fields.push(Span {
                    start: base + field_start,
                    len: (self.arena.len() - field_start) as u32,
                });
                field_start = self.arena.len();
                pos += 1;
                continue;
            }
            if Some(c) == quote && php_trim(&self.arena[field_start..]).is_empty() {
                in_quotes = true;
                self.arena.truncate(field_start);
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
        self.unterminated = in_quotes;
        fields.push(Span {
            start: base + field_start,
            len: (self.arena.len() - field_start) as u32,
        });
        self.emitted = true;
        Record::Fields
    }

    fn bytes(&self, span: Span) -> &[u8] {
        let end = span.start + span.len as usize;
        let base = self.text.len();
        if span.start >= base {
            &self.arena[span.start - base..end - base]
        } else {
            &self.text[span.start..end]
        }
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
    o.validate()?;
    let mut tok = Tokenizer::new(data, o);
    let mut fields = Vec::new();
    let mut problems = Vec::new();
    let note_unterminated = |tok: &Tokenizer, problems: &mut Vec<String>| {
        if tok.unterminated {
            problems.push(format!(
                "row {}: unterminated quoted field at end of file",
                tok.line
            ));
        }
    };

    // The first non-empty record is the header (or sets the width).
    loop {
        match tok.next(&mut fields) {
            Record::Eof => {
                note_unterminated(&tok, &mut problems);
                return Ok(Table {
                    names: Vec::new(),
                    columns: Vec::new(),
                    problems,
                    text: tok.text,
                    arena: tok.arena,
                });
            }
            Record::Blank => continue,
            Record::Fields => break,
        }
    }
    note_unterminated(&tok, &mut problems);
    // A UTF-8 byte order mark on the first field.
    if tok.bytes(fields[0]).starts_with(b"\xEF\xBB\xBF") {
        fields[0].start += 3;
        fields[0].len -= 3;
    }
    let width = fields.len();
    let header_line = tok.line;
    let names: Vec<Vec<u8>> = if o.col_names {
        fields
            .iter()
            .map(|&f| {
                let b = tok.bytes(f);
                if o.trim_ws {
                    php_trim(b).to_vec()
                } else {
                    b.to_vec()
                }
            })
            .collect()
    } else {
        (1..=width).map(|i| format!("X{i}").into_bytes()).collect()
    };

    let mut rows = Rows {
        cells: vec![Vec::new(); width],
        count: 0,
        joined: Vec::new(),
    };
    let full = |count: usize| o.n_max.is_some_and(|m| count >= m);
    if !o.col_names && !full(rows.count) {
        rows.take(&mut tok, &fields, header_line, &mut problems);
    }
    while !full(rows.count) {
        match tok.next(&mut fields) {
            Record::Eof => {
                note_unterminated(&tok, &mut problems);
                break;
            }
            Record::Blank => {}
            Record::Fields => {
                note_unterminated(&tok, &mut problems);
                let line = tok.line;
                rows.take(&mut tok, &fields, line, &mut problems);
            }
        }
    }
    let Rows { cells, count, .. } = rows;

    let sample = sample_rows(count, o.guess_max);
    let parser = Parser {
        decimal: o.decimal_mark,
        grouping: o.grouping(),
    };
    let mut columns = Vec::with_capacity(width);
    for (i, column) in cells.into_iter().enumerate() {
        let code = o.types.get(i).copied().unwrap_or(TypeCode::Guess);
        let view = |s: Span| -> Option<&[u8]> { if s.is_na() { None } else { Some(tok.bytes(s)) } };
        let (code, grouping) = if code == TypeCode::Guess {
            // As phibble does: readr reads the columns of a header-only file as
            // text, so zero data rows gives string before the guesser's
            // empty-column rule applies.
            let guessed = if count == 0 {
                TypeCode::String
            } else {
                parser.guess(sample.iter().map(|&r| view(column[r])))
            };
            (guessed, true)
        } else {
            (code, false)
        };
        columns.push(match code {
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
        });
    }
    Ok(Table {
        names,
        columns,
        problems,
        text: tok.text,
        arena: tok.arena,
    })
}

/// Data rows, column by column.
struct Rows {
    cells: Vec<Vec<Span>>,
    count: usize,
    joined: Vec<u8>,
}

impl Rows {
    fn take(
        &mut self,
        tok: &mut Tokenizer,
        fields: &[Span],
        line: usize,
        problems: &mut Vec<String>,
    ) {
        let width = self.cells.len();
        let o = tok.o;
        self.count += 1;
        let n = fields.len();
        if n != width {
            problems.push(format!("row {line}: expected {width} columns, got {n}"));
        }
        let kept = n.min(width);
        for (i, &field) in fields.iter().enumerate().take(kept) {
            let field = if i == width - 1 && n > width {
                // The extra fields, joined by the delimiter, go into the last column.
                self.joined.clear();
                for (k, &f) in fields[width - 1..].iter().enumerate() {
                    if k > 0 {
                        self.joined.push(o.delimiter);
                    }
                    self.joined.extend_from_slice(tok.bytes(f));
                }
                let start = tok.text.len() + tok.arena.len();
                tok.arena.extend_from_slice(&self.joined);
                Span {
                    start,
                    len: self.joined.len() as u32,
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
        let base = tok.text.len();
        let bytes = if field.start >= base {
            &tok.arena[field.start - base..]
        } else {
            &tok.text[field.start..]
        };
        trim_span(bytes, field)
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
