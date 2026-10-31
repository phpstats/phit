//! read_csv() and CsvReader: the native readers behind phibble's
//! DataFrame::fromCsv() and DataFrame::csvChunks().
use ext_php_rs::{
    boxed::ZBox,
    convert::{FromZval, StubLiteral},
    flags::DataType,
    prelude::*,
    types::{ZendHashTable, ZendStr, Zval},
};
use stats_core::csv::{self, Column, Options, Span, TypeCode, Values};

fn value_error(message: impl Into<String>) -> PhpException {
    PhpException::new(message.into(), 0, ext_php_rs::zend::ce::value_error())
}

/// A PHP string taken as bytes (paths need not be UTF-8).
pub struct Bytes<'a>(&'a [u8]);

impl<'a> FromZval<'a> for Bytes<'a> {
    const TYPE: DataType = DataType::String;
    fn from_zval(zval: &'a Zval) -> Option<Self> {
        zval.zend_str().map(|s| Bytes(s.as_bytes()))
    }
}

/// The options array; omitted means `[]`.
pub struct CsvOptions<'a>(Option<&'a ZendHashTable>);

impl<'a> FromZval<'a> for CsvOptions<'a> {
    const TYPE: DataType = DataType::Array;
    fn from_zval(zval: &'a Zval) -> Option<Self> {
        zval.array().map(|a| CsvOptions(Some(a)))
    }
}

impl StubLiteral for CsvOptions<'_> {
    fn stub_literal(&self) -> String {
        "[]".into()
    }
}

fn bytes_of<'a>(key: &str, v: &'a Zval) -> Result<&'a [u8], String> {
    v.zend_str()
        .map(|s| s.as_bytes())
        .ok_or_else(|| format!("option '{key}' must be a string"))
}

fn bool_of(key: &str, v: &Zval) -> Result<bool, String> {
    v.bool()
        .filter(|_| v.is_bool())
        .ok_or_else(|| format!("option '{key}' must be a bool"))
}

fn count_of(key: &str, v: &Zval, min: i64) -> Result<usize, String> {
    match v.long() {
        Some(n) if v.is_long() && n >= min => Ok(n as usize),
        _ => Err(format!("option '{key}' must be an int of at least {min}")),
    }
}

fn one_byte(key: &str, v: &Zval, allow_empty: bool) -> Result<Option<u8>, String> {
    match bytes_of(key, v)? {
        [b] => Ok(Some(*b)),
        [] if allow_empty => Ok(None),
        _ => Err(format!(
            "option '{key}' must be {} byte",
            if allow_empty {
                "an empty string or a single"
            } else {
                "a single"
            }
        )),
    }
}

fn options(table: Option<&ZendHashTable>) -> Result<Options, String> {
    let mut o = Options::default();
    let Some(table) = table else { return Ok(o) };
    for (key, v) in table {
        let key = key.to_string();
        match key.as_str() {
            "delimiter" => o.delimiter = one_byte(&key, v, false)?.unwrap_or(b','),
            "quote" => o.quote = one_byte(&key, v, true)?,
            "escape_double" => o.escape_double = bool_of(&key, v)?,
            "escape_backslash" => o.escape_backslash = bool_of(&key, v)?,
            "trim_ws" => o.trim_ws = bool_of(&key, v)?,
            "skip_empty_rows" => o.skip_empty_rows = bool_of(&key, v)?,
            "col_names" => o.col_names = bool_of(&key, v)?,
            "skip" => o.skip = count_of(&key, v, 0)?,
            "guess_max" => o.guess_max = count_of(&key, v, 1)?,
            "block_size" => o.block_size = count_of(&key, v, 1)?,
            "n_max" => {
                o.n_max = if v.is_null() {
                    None
                } else {
                    Some(count_of(&key, v, 0)?)
                }
            }
            "comment" => o.comment = bytes_of(&key, v)?.to_vec(),
            "decimal_mark" => o.decimal_mark = one_byte(&key, v, false)?.unwrap_or(b'.'),
            "grouping_mark" => {
                o.grouping_mark = if v.is_null() {
                    None
                } else {
                    Some(one_byte(&key, v, true)?)
                }
            }
            "na" => {
                let list = v
                    .array()
                    .ok_or_else(|| "option 'na' must be a list of strings".to_string())?;
                o.na = list
                    .values()
                    .map(|s| bytes_of("na", s).map(<[u8]>::to_vec))
                    .collect::<Result<_, String>>()?;
            }
            "types" => {
                let list = v
                    .array()
                    .ok_or_else(|| "option 'types' must be an array of type codes".to_string())?;
                for (position, code) in list {
                    let position = match position {
                        ext_php_rs::types::ArrayKey::Long(i) if i >= 0 => i as usize,
                        _ => {
                            return Err("'types' keys must be 0-based column positions".to_string());
                        }
                    };
                    let code = TypeCode::from_code(bytes_of("types", code)?).ok_or_else(|| {
                        format!(
                            "unknown type code '{}' at position {position}; use ? l i d c D T or _",
                            String::from_utf8_lossy(
                                code.zend_str().map_or(&[][..], |s| s.as_bytes())
                            )
                        )
                    })?;
                    if o.types.len() <= position {
                        if position > 1 << 20 {
                            return Err("'types' position is too large".to_string());
                        }
                        o.types.resize(position + 1, TypeCode::Guess);
                    }
                    o.types[position] = code;
                }
            }
            _ => {
                return Err(format!("unknown option '{key}'"));
            }
        }
    }
    Ok(o)
}

fn list<T>(values: &[Option<T>], set: impl Fn(&mut Zval, &T)) -> ZBox<ZendHashTable> {
    let mut out = ZendHashTable::with_capacity(values.len() as u32);
    for v in values {
        let mut z = Zval::new();
        match v {
            Some(v) => set(&mut z, v),
            None => z.set_null(),
        }
        let _ = out.push(z);
    }
    out
}

fn os_path(path: &[u8]) -> &std::path::Path {
    use std::os::unix::ffi::OsStrExt;
    std::path::Path::new(std::ffi::OsStr::from_bytes(path))
}

fn open(function: &str, path: &[u8]) -> PhpResult<std::fs::File> {
    let display = String::from_utf8_lossy(path);
    let os_path = os_path(path);
    if !os_path.is_file() {
        return Err(value_error(format!(
            "{function}: cannot read CSV file '{display}'"
        )));
    }
    std::fs::File::open(os_path)
        .map_err(|e| value_error(format!("{function}: cannot read CSV file '{display}': {e}")))
}

fn names_list(names: &[Vec<u8>]) -> ZBox<ZendHashTable> {
    let mut out = ZendHashTable::with_capacity(names.len() as u32);
    for name in names {
        let mut z = Zval::new();
        z.set_zend_string(ZendStr::new(name, false));
        let _ = out.push(z);
    }
    out
}

/// The `columns` list: per column its type, values and failed count, or null
/// for a skipped column.
fn columns_list<'a>(
    columns: &[Option<Column>],
    bytes: impl Fn(Span) -> &'a [u8],
) -> ZBox<ZendHashTable> {
    let mut out = ZendHashTable::with_capacity(columns.len() as u32);
    for column in columns {
        let Some(column) = column else {
            let _ = out.push(Zval::new());
            continue;
        };
        let values = match &column.values {
            Values::Bool(v) => list(v, |z, &b| z.set_bool(b)),
            Values::Int(v) | Values::Date(v) | Values::DateTime(v) => {
                list(v, |z, &n| z.set_long(n))
            }
            Values::Float(v) => list(v, |z, &x| z.set_double(x)),
            Values::Str(v) => {
                let mut values = ZendHashTable::with_capacity(v.len() as u32);
                for &span in v {
                    let mut z = Zval::new();
                    if span.is_na() {
                        z.set_null();
                    } else {
                        z.set_zend_string(ZendStr::new(bytes(span), false));
                    }
                    let _ = values.push(z);
                }
                values
            }
        };
        let mut entry = ZendHashTable::with_capacity(3);
        let _ = entry.insert("type", column.values.type_name());
        let _ = entry.insert("values", values);
        let _ = entry.insert("failed", column.failed as i64);
        let _ = out.push(entry);
    }
    out
}

fn problems_list(problems: &[String]) -> ZBox<ZendHashTable> {
    let mut out = ZendHashTable::with_capacity(problems.len() as u32);
    for p in problems {
        let _ = out.push(p.as_str());
    }
    out
}

#[php_function]
#[php(name = "Phit\\read_csv", defaults(options = CsvOptions(None)))]
pub fn read_csv(path: Bytes, options: CsvOptions) -> PhpResult<ZBox<ZendHashTable>> {
    const FUNCTION: &str = "Phit\\read_csv()";
    let o = self::options(options.0).map_err(|e| value_error(format!("{FUNCTION}: {e}")))?;
    let file = open(FUNCTION, path.0)?;
    let table =
        csv::read_from(Box::new(file), &o).map_err(|e| value_error(format!("{FUNCTION}: {e}")))?;
    let mut out = ZendHashTable::with_capacity(3);
    let _ = out.insert("names", names_list(&table.names));
    let _ = out.insert("columns", columns_list(&table.columns, |s| table.bytes(s)));
    let _ = out.insert("problems", problems_list(&table.problems));
    Ok(out)
}

/// Reads a CSV file a chunk of rows at a time, holding one chunk in memory.
/// Takes read_csv()'s options; types not given are guessed from the first
/// chunk and fixed for the rest.
#[php_class]
#[php(name = "Phit\\CsvReader")]
#[php(flags = ext_php_rs::flags::ClassFlags::Final)]
pub struct CsvReader {
    reader: csv::Reader,
}

#[php_impl]
impl CsvReader {
    /// Opens the file and reads its header line.
    #[php(defaults(options = CsvOptions(None)))]
    pub fn __construct(path: Bytes, options: CsvOptions) -> PhpResult<Self> {
        const FUNCTION: &str = "Phit\\CsvReader::__construct()";
        let o = self::options(options.0).map_err(|e| value_error(format!("{FUNCTION}: {e}")))?;
        let file = open(FUNCTION, path.0)?;
        let reader = csv::Reader::new(Box::new(file), o)
            .map_err(|e| value_error(format!("{FUNCTION}: {e}")))?;
        Ok(Self { reader })
    }

    /// The column names: the header's, or X1..Xn without one; [] for an empty file.
    pub fn names(&self) -> ZBox<ZendHashTable> {
        names_list(self.reader.names())
    }

    /// The next chunk of at most $rows rows, as read_csv() returns a file,
    /// with 'position', the 1-based index of its first row, and 'rows'. Null once no
    /// rows remain. A chunk without rows comes only to carry problems found
    /// after the last row.
    pub fn next(&mut self, rows: i64) -> PhpResult<Option<ZBox<ZendHashTable>>> {
        if rows < 1 {
            return Err(value_error(format!(
                "Phit\\CsvReader::next(): rows must be at least 1, got {rows}"
            )));
        }
        let names = names_list(self.reader.names());
        let chunk = self.reader.next_chunk(rows as usize).map_err(|e| {
            value_error(format!(
                "Phit\\CsvReader::next(): cannot read the input: {e}"
            ))
        })?;
        let Some(chunk) = chunk else { return Ok(None) };
        let mut out = ZendHashTable::with_capacity(5);
        let _ = out.insert("position", chunk.position as i64);
        let _ = out.insert("rows", chunk.rows as i64);
        let _ = out.insert("names", names);
        let _ = out.insert("columns", columns_list(&chunk.columns, |s| chunk.bytes(s)));
        let _ = out.insert("problems", problems_list(&chunk.problems));
        Ok(Some(out))
    }
}

pub fn register(module: ModuleBuilder) -> ModuleBuilder {
    module
        .function(wrap_function!(read_csv))
        .class::<CsvReader>()
}
