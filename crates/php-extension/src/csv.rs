//! read_csv(): the native reader behind phibble's DataFrame::fromCsv().
use ext_php_rs::{
    boxed::ZBox,
    convert::{FromZval, StubLiteral},
    flags::DataType,
    prelude::*,
    types::{ZendHashTable, ZendStr, Zval},
};
use stats_core::csv::{self, Options, TypeCode, Values};

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

fn bytes_of<'a>(key: &str, v: &'a Zval) -> PhpResult<&'a [u8]> {
    v.zend_str()
        .map(|s| s.as_bytes())
        .ok_or_else(|| value_error(format!("Phit\\read_csv(): option '{key}' must be a string")))
}

fn bool_of(key: &str, v: &Zval) -> PhpResult<bool> {
    v.bool()
        .filter(|_| v.is_bool())
        .ok_or_else(|| value_error(format!("Phit\\read_csv(): option '{key}' must be a bool")))
}

fn count_of(key: &str, v: &Zval, min: i64) -> PhpResult<usize> {
    match v.long() {
        Some(n) if v.is_long() && n >= min => Ok(n as usize),
        _ => Err(value_error(format!(
            "Phit\\read_csv(): option '{key}' must be an int of at least {min}"
        ))),
    }
}

fn one_byte(key: &str, v: &Zval, allow_empty: bool) -> PhpResult<Option<u8>> {
    match bytes_of(key, v)? {
        [b] => Ok(Some(*b)),
        [] if allow_empty => Ok(None),
        _ => Err(value_error(format!(
            "Phit\\read_csv(): option '{key}' must be {} byte",
            if allow_empty {
                "an empty string or a single"
            } else {
                "a single"
            }
        ))),
    }
}

fn options(table: Option<&ZendHashTable>) -> PhpResult<Options> {
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
                let list = v.array().ok_or_else(|| {
                    value_error("Phit\\read_csv(): option 'na' must be a list of strings")
                })?;
                o.na = list
                    .values()
                    .map(|s| bytes_of("na", s).map(<[u8]>::to_vec))
                    .collect::<PhpResult<_>>()?;
            }
            "types" => {
                let list = v.array().ok_or_else(|| {
                    value_error("Phit\\read_csv(): option 'types' must be an array of type codes")
                })?;
                for (position, code) in list {
                    let position = match position {
                        ext_php_rs::types::ArrayKey::Long(i) if i >= 0 => i as usize,
                        _ => {
                            return Err(value_error(
                                "Phit\\read_csv(): 'types' keys must be 0-based column positions",
                            ));
                        }
                    };
                    let code = TypeCode::from_code(bytes_of("types", code)?).ok_or_else(|| {
                        value_error(format!(
                            "Phit\\read_csv(): unknown type code '{}' at position {position}; use ? l i d c D T or _",
                            String::from_utf8_lossy(code.zend_str().map_or(&[][..], |s| s.as_bytes()))
                        ))
                    })?;
                    if o.types.len() <= position {
                        if position > 1 << 20 {
                            return Err(value_error(
                                "Phit\\read_csv(): 'types' position is too large",
                            ));
                        }
                        o.types.resize(position + 1, TypeCode::Guess);
                    }
                    o.types[position] = code;
                }
            }
            _ => {
                return Err(value_error(format!(
                    "Phit\\read_csv(): unknown option '{key}'"
                )));
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

#[php_function]
#[php(name = "Phit\\read_csv", defaults(options = CsvOptions(None)))]
pub fn read_csv(path: Bytes, options: CsvOptions) -> PhpResult<ZBox<ZendHashTable>> {
    let o = self::options(options.0)?;
    let display = String::from_utf8_lossy(path.0);
    let os_path = {
        use std::os::unix::ffi::OsStrExt;
        std::path::Path::new(std::ffi::OsStr::from_bytes(path.0))
    };
    if !os_path.is_file() {
        return Err(value_error(format!(
            "Phit\\read_csv(): cannot read CSV file '{display}'"
        )));
    }
    let data = std::fs::read(os_path).map_err(|e| {
        value_error(format!(
            "Phit\\read_csv(): cannot read CSV file '{display}': {e}"
        ))
    })?;
    let table = csv::read(data, &o).map_err(|e| value_error(format!("Phit\\read_csv(): {e}")))?;

    let mut names = ZendHashTable::with_capacity(table.names.len() as u32);
    for name in &table.names {
        let mut z = Zval::new();
        z.set_zend_string(ZendStr::new(name, false));
        let _ = names.push(z);
    }
    let mut columns = ZendHashTable::with_capacity(table.columns.len() as u32);
    for column in &table.columns {
        let Some(column) = column else {
            let _ = columns.push(Zval::new());
            continue;
        };
        let values = match &column.values {
            Values::Bool(v) => list(v, |z, &b| z.set_bool(b)),
            Values::Int(v) | Values::Date(v) | Values::DateTime(v) => {
                list(v, |z, &n| z.set_long(n))
            }
            Values::Float(v) => list(v, |z, &x| z.set_double(x)),
            Values::Str(v) => {
                let mut out = ZendHashTable::with_capacity(v.len() as u32);
                for &span in v {
                    let mut z = Zval::new();
                    if span.is_na() {
                        z.set_null();
                    } else {
                        z.set_zend_string(ZendStr::new(table.bytes(span), false));
                    }
                    let _ = out.push(z);
                }
                out
            }
        };
        let mut entry = ZendHashTable::with_capacity(3);
        let _ = entry.insert("type", column.values.type_name());
        let _ = entry.insert("values", values);
        let _ = entry.insert("failed", column.failed as i64);
        let _ = columns.push(entry);
    }
    let mut problems = ZendHashTable::with_capacity(table.problems.len() as u32);
    for p in &table.problems {
        let _ = problems.push(p.as_str());
    }
    let mut out = ZendHashTable::with_capacity(3);
    let _ = out.insert("names", names);
    let _ = out.insert("columns", columns);
    let _ = out.insert("problems", problems);
    Ok(out)
}

pub fn register(module: ModuleBuilder) -> ModuleBuilder {
    module.function(wrap_function!(read_csv))
}
