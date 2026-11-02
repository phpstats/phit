//! Fit and table objects. Every model function returns one of four final classes behind the `Phit\Fit` interface,
//! chosen by the result's `model` key; `Phit\tidy()` returns a `Phit\Table`. Both hold the result array: reading
//! a key works as on the array, writes are refused, `echo` prints the summary or the table, and `toArray()` returns
//! the array.
use crate::printer;
use ext_php_rs::{
    boxed::ZBox,
    convert::IntoZval,
    exception::PhpException,
    prelude::*,
    types::{ZendClassObject, ZendHashTable, ZendObject, Zval},
    zend::{ClassEntry, ce},
};

// Class entries PHP exports from ext/json and ext/spl, which are always compiled in. They are read directly
// because class lookup by name is not available while the module starts.
unsafe extern "C" {
    static php_json_serializable_ce: *mut ClassEntry;
    static spl_ce_LogicException: *mut ClassEntry;
}

fn json_serializable() -> &'static ClassEntry {
    // SAFETY: set by ext/json before extensions load, and never freed.
    unsafe { php_json_serializable_ce.as_ref() }.expect("JsonSerializable is part of PHP")
}

fn logic_exception() -> &'static ClassEntry {
    // SAFETY: set by ext/spl before extensions load, and never freed.
    unsafe { spl_ce_LogicException.as_ref() }.expect("LogicException is part of PHP")
}

fn read_only(what: &str) -> PhpException {
    PhpException::new(
        format!("{what} is read-only; call toArray() for an array copy"),
        0,
        logic_exception(),
    )
}

fn array_zval(table: &ZendHashTable) -> Zval {
    let mut z = Zval::new();
    z.set_hashtable(table.to_owned());
    z
}

fn key_text(offset: &Zval) -> Option<String> {
    offset
        .str()
        .map(str::to_owned)
        .or_else(|| offset.long().map(|l| l.to_string()))
}

/// `$x[$key]` on the held array, with PHP's warning for a missing key.
fn offset_get(table: &ZendHashTable, offset: &Zval) -> Zval {
    match key_text(offset).and_then(|k| table.get(k.as_str())) {
        Some(v) => v.shallow_clone(),
        None => {
            let shown = key_text(offset).unwrap_or_default();
            ext_php_rs::error::php_error(
                &ext_php_rs::flags::ErrorType::Warning,
                &format!("Undefined array key \"{shown}\""),
            );
            Zval::new()
        }
    }
}

fn offset_exists(table: &ZendHashTable, offset: &Zval) -> bool {
    key_text(offset)
        .and_then(|k| table.get(k.as_str()))
        .is_some_and(|v| !v.is_null())
}

fn key_at(table: &ZendHashTable, position: usize) -> Option<String> {
    table
        .iter()
        .nth(position)
        .map(|(k, _)| String::try_from(k).unwrap_or_default())
}

fn value_at(table: &ZendHashTable, position: usize) -> Zval {
    table
        .iter()
        .nth(position)
        .map(|(_, v)| v.shallow_clone())
        .unwrap_or_default()
}

/// A Jupyter display bundle: the printed form as text/plain and the array as application/json.
fn mime_bundle(text: String, table: &ZendHashTable) -> PhpResult<ZBox<ZendHashTable>> {
    let mut bundle = ZendHashTable::new();
    bundle.insert("text/plain", text)?;
    bundle.insert("application/json", array_zval(table))?;
    Ok(bundle)
}

/// The model fit interface: every model function's result implements it.
#[php_interface]
#[php(name = "Phit\\Fit")]
#[php(extends(ce = ce::arrayaccess, stub = "\\ArrayAccess"))]
#[php(extends(ce = ce::iterator, stub = "\\Iterator"))]
#[php(extends(ce = ce::countable, stub = "\\Countable"))]
#[php(extends(ce = json_serializable, stub = "\\JsonSerializable"))]
#[php(extends(ce = ce::stringable, stub = "\\Stringable"))]
#[allow(dead_code)] // The interface is used from PHP.
pub trait Fit {}

macro_rules! fit_class {
    ($class:ident, $name:literal) => {
        #[php_class]
        #[php(name = $name)]
        #[php(flags = ext_php_rs::flags::ClassFlags::Final)]
        #[php(implements(PhpInterfaceFit))]
        #[derive(Clone)]
        pub struct $class {
            data: ZBox<ZendHashTable>,
            position: usize,
        }

        #[php_impl]
        #[allow(non_snake_case)] // PHP parameter names are camelCase.
        impl $class {
            pub fn __to_string(&self) -> PhpResult<String> {
                printer::summary_text(&self.data)
            }

            pub fn summary(&self) -> PhpResult<String> {
                printer::summary_text(&self.data)
            }

            #[php(defaults(confInt = false, level = 0.95, effects = "\"fixed\""))]
            pub fn tidy(&self, confInt: bool, level: f64, effects: &str) -> PhpResult<Zval> {
                into_table(printer::tidy_columns(&self.data, confInt, level, effects)?)
            }

            #[php(defaults(
                r#type = "\"link\"",
                seFit = false,
                interval = "\"none\"",
                level = 0.95,
                random = true,
                allowNewLevels = false
            ))]
            #[allow(clippy::too_many_arguments)]
            pub fn predict(
                &self,
                newdata: &Zval,
                r#type: &str,
                seFit: bool,
                interval: &str,
                level: f64,
                random: bool,
                allowNewLevels: bool,
            ) -> PhpResult<ZBox<ZendHashTable>> {
                crate::formula::predict_from(
                    &self.data,
                    newdata,
                    r#type,
                    seFit,
                    interval,
                    level,
                    random,
                    allowNewLevels,
                )
            }

            pub fn to_array(&self) -> ZBox<ZendHashTable> {
                self.data.to_owned()
            }

            pub fn json_serialize(&self) -> ZBox<ZendHashTable> {
                self.data.to_owned()
            }

            pub fn to_mime_bundle(&self) -> PhpResult<ZBox<ZendHashTable>> {
                mime_bundle(printer::summary_text(&self.data)?, &self.data)
            }

            pub fn __debug_info(&self) -> ZBox<ZendHashTable> {
                self.data.to_owned()
            }

            pub fn offset_exists(&self, offset: &Zval) -> bool {
                offset_exists(&self.data, offset)
            }

            pub fn offset_get(&self, offset: &Zval) -> Zval {
                offset_get(&self.data, offset)
            }

            pub fn offset_set(&mut self, _offset: &Zval, _value: &Zval) -> PhpResult {
                Err(read_only("A Phit fit"))
            }

            pub fn offset_unset(&mut self, _offset: &Zval) -> PhpResult {
                Err(read_only("A Phit fit"))
            }

            pub fn count(&self) -> i64 {
                self.data.len() as i64
            }

            pub fn current(&self) -> Zval {
                value_at(&self.data, self.position)
            }

            pub fn key(&self) -> Option<String> {
                key_at(&self.data, self.position)
            }

            pub fn next(&mut self) {
                self.position += 1;
            }

            pub fn rewind(&mut self) {
                self.position = 0;
            }

            pub fn valid(&self) -> bool {
                self.position < self.data.len()
            }
        }
    };
}

fit_class!(LinearFit, "Phit\\LinearFit");
fit_class!(GeneralizedLinearFit, "Phit\\GeneralizedLinearFit");
fit_class!(MixedGaussianFit, "Phit\\MixedGaussianFit");
fit_class!(MixedLogisticFit, "Phit\\MixedLogisticFit");

/// The fit object for a model function's result, by its `model` key.
pub fn into_fit(data: ZBox<ZendHashTable>) -> PhpResult<Zval> {
    let kind = data
        .get("model")
        .and_then(Zval::str)
        .unwrap_or_default()
        .to_owned();
    let object = |z: Result<Zval, _>| {
        z.map_err(|_| PhpException::from_message("could not create the fit object".into()))
    };
    match kind.as_str() {
        "linear" => object(ZendClassObject::new(LinearFit { data, position: 0 }).into_zval(false)),
        "generalized_linear" => object(
            ZendClassObject::new(GeneralizedLinearFit { data, position: 0 }).into_zval(false),
        ),
        "mixed_gaussian" => {
            object(ZendClassObject::new(MixedGaussianFit { data, position: 0 }).into_zval(false))
        }
        "mixed_logistic" => {
            object(ZendClassObject::new(MixedLogisticFit { data, position: 0 }).into_zval(false))
        }
        _ => unreachable!("model functions set a known model key"),
    }
}

/// The result array behind a fit argument: a Phit fit object, or an array in the same shape.
pub fn fit_data(fit: &Zval) -> Option<&ZendHashTable> {
    if let Some(table) = fit.array() {
        return Some(table);
    }
    let object: &ZendObject = fit.object()?;
    macro_rules! held {
        ($($class:ident),*) => {
            $(if let Some(o) = ZendClassObject::<$class>::from_zend_obj(object) {
                return Some(&o.data);
            })*
        };
    }
    held!(
        LinearFit,
        GeneralizedLinearFit,
        MixedGaussianFit,
        MixedLogisticFit
    );
    None
}

/// A table of named columns, as `tidy()` returns: printed in the layout of R's `print.data.frame()`.
#[php_class]
#[php(name = "Phit\\Table")]
#[php(flags = ext_php_rs::flags::ClassFlags::Final)]
#[php(implements(ce = ce::arrayaccess, stub = "\\ArrayAccess"))]
#[php(implements(ce = ce::iterator, stub = "\\Iterator"))]
#[php(implements(ce = ce::countable, stub = "\\Countable"))]
#[php(implements(ce = json_serializable, stub = "\\JsonSerializable"))]
#[derive(Clone)]
pub struct Table {
    columns: ZBox<ZendHashTable>,
    position: usize,
}

#[php_impl]
impl Table {
    pub fn __to_string(&self) -> PhpResult<String> {
        printer::table_text(&self.columns)
    }

    pub fn to_array(&self) -> ZBox<ZendHashTable> {
        self.columns.to_owned()
    }

    pub fn to_columns(&self) -> ZBox<ZendHashTable> {
        self.columns.to_owned()
    }

    pub fn json_serialize(&self) -> ZBox<ZendHashTable> {
        self.columns.to_owned()
    }

    pub fn to_mime_bundle(&self) -> PhpResult<ZBox<ZendHashTable>> {
        mime_bundle(printer::table_text(&self.columns)?, &self.columns)
    }

    pub fn __debug_info(&self) -> ZBox<ZendHashTable> {
        self.columns.to_owned()
    }

    pub fn offset_exists(&self, offset: &Zval) -> bool {
        offset_exists(&self.columns, offset)
    }

    pub fn offset_get(&self, offset: &Zval) -> Zval {
        offset_get(&self.columns, offset)
    }

    pub fn offset_set(&mut self, _offset: &Zval, _value: &Zval) -> PhpResult {
        Err(read_only("A Phit table"))
    }

    pub fn offset_unset(&mut self, _offset: &Zval) -> PhpResult {
        Err(read_only("A Phit table"))
    }

    /// The number of rows.
    pub fn count(&self) -> i64 {
        self.columns
            .values()
            .next()
            .and_then(Zval::array)
            .map_or(0, |c| c.len() as i64)
    }

    pub fn current(&self) -> Zval {
        value_at(&self.columns, self.position)
    }

    pub fn key(&self) -> Option<String> {
        key_at(&self.columns, self.position)
    }

    pub fn next(&mut self) {
        self.position += 1;
    }

    pub fn rewind(&mut self) {
        self.position = 0;
    }

    pub fn valid(&self) -> bool {
        self.position < self.columns.len()
    }
}

pub fn into_table(columns: ZBox<ZendHashTable>) -> PhpResult<Zval> {
    ZendClassObject::new(Table {
        columns,
        position: 0,
    })
    .into_zval(false)
    .map_err(|_| PhpException::from_message("could not create the table object".into()))
}

pub fn register(module: ModuleBuilder) -> ModuleBuilder {
    module
        .interface::<PhpInterfaceFit>()
        .class::<LinearFit>()
        .class::<GeneralizedLinearFit>()
        .class::<MixedGaussianFit>()
        .class::<MixedLogisticFit>()
        .class::<Table>()
}
