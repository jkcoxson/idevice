// Jackson Coxson
// Ported from serde's json!

/// Construct a `plist::Value` from a JSON-like literal.
///
/// ```
/// # use idevice::plist;
/// #
/// let value = plist!({
///     "code": 200,
///     "success": true,
///     "payload": {
///         "features": [
///             "serde",
///             "plist"
///         ],
///         "homepage": null
///     }
/// });
/// ```
///
/// Variables or expressions can be interpolated into the plist literal. Any type
/// interpolated into an array element or object value must implement `Into<plist::Value>`.
/// If the conversion fails, the `plist!` macro will panic.
///
/// ```
/// # use idevice::plist;
/// #
/// let code = 200;
/// let features = vec!["serde", "plist"];
///
/// let value = plist!({
///     "code": code,
///     "success": code == 200,
///     "payload": {
///         features[0]: features[1]
///     }
/// });
/// ```
///
/// Trailing commas are allowed inside both arrays and objects.
///
/// ```
/// # use idevice::plist;
/// #
/// let value = plist!([
///     "notice",
///     "the",
///     "trailing",
///     "comma -->",
/// ]);
/// ```
#[macro_export]
macro_rules! plist {
    // Force: dictionary out
    (dict { $($tt:tt)+ }) => {{
        let mut object = plist::Dictionary::new();
        $crate::plist_internal!(@object object () ($($tt)+) ($($tt)+));
        object
    }};

    // Force: value out (explicit, though default already does this)
    (value { $($tt:tt)+ }) => {
        $crate::plist_internal!({ $($tt)+ })
    };

    // Force: raw vec of plist::Value out
    (array [ $($tt:tt)+ ]) => {
        $crate::plist_internal!(@array [] $($tt)+)
    };

    // Hide distracting implementation details from the generated rustdoc.
    ($($plist:tt)+) => {
        $crate::plist_internal!($($plist)+)
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! plist_internal {
    //////////////////////////////////////////////////////////////////////////
    // TT muncher for parsing the inside of an array [...]. Produces a vec![...]
    // of the elements.
    //
    // Must be invoked as: plist_internal!(@array [] $($tt)*)
    //////////////////////////////////////////////////////////////////////////

    // Done with trailing comma.
    (@array [$($elems:expr,)*]) => {
        vec![$($elems,)*]
    };

    // Done without trailing comma.
    (@array [$($elems:expr),*]) => {
        vec![$($elems),*]
    };

    // Next element is `null`.
    (@array [$($elems:expr,)*] null $($rest:tt)*) => {
        $crate::plist_internal!(@array [$($elems,)* $crate::plist_internal!(null)] $($rest)*)
    };

    // Next element is `true`.
    (@array [$($elems:expr,)*] true $($rest:tt)*) => {
        $crate::plist_internal!(@array [$($elems,)* $crate::plist_internal!(true)] $($rest)*)
    };

    // Next element is `false`.
    (@array [$($elems:expr,)*] false $($rest:tt)*) => {
        $crate::plist_internal!(@array [$($elems,)* $crate::plist_internal!(false)] $($rest)*)
    };

    // Next element is an array.
    (@array [$($elems:expr,)*] [$($array:tt)*] $($rest:tt)*) => {
        $crate::plist_internal!(@array [$($elems,)* $crate::plist_internal!([$($array)*])] $($rest)*)
    };

    // Next element is a map.
    (@array [$($elems:expr,)*] {$($map:tt)*} $($rest:tt)*) => {
        $crate::plist_internal!(@array [$($elems,)* $crate::plist_internal!({$($map)*})] $($rest)*)
    };

    // Next element is an expression followed by comma.
    (@array [$($elems:expr,)*] $next:expr, $($rest:tt)*) => {
        $crate::plist_internal!(@array [$($elems,)* $crate::plist_internal!($next),] $($rest)*)
    };

    // Last element is an expression with no trailing comma.
    (@array [$($elems:expr,)*] $last:expr) => {
        $crate::plist_internal!(@array [$($elems,)* $crate::plist_internal!($last)])
    };

    // Comma after the most recent element.
    (@array [$($elems:expr),*] , $($rest:tt)*) => {
        $crate::plist_internal!(@array [$($elems,)*] $($rest)*)
    };

    // Unexpected token after most recent element.
    (@array [$($elems:expr),*] $unexpected:tt $($rest:tt)*) => {
        $crate::plist_unexpected!($unexpected)
    };

    (@array [$($elems:expr,)*] ? $maybe:expr , $($rest:tt)*) => {
        if let Some(__v) = $crate::plist_macro::plist_maybe($maybe) {
            $crate::plist_internal!(@array [$($elems,)* __v,] $($rest)*)
        } else {
            $crate::plist_internal!(@array [$($elems,)*] $($rest)*)
        }
    };
    (@array [$($elems:expr,)*] ? $maybe:expr) => {
        if let Some(__v) = $crate::plist_macro::plist_maybe($maybe) {
            $crate::plist_internal!(@array [$($elems,)* __v])
        } else {
            $crate::plist_internal!(@array [$($elems,)*])
        }
    };

    //////////////////////////////////////////////////////////////////////////
    // TT muncher for parsing the inside of an object {...}. Each entry is
    // inserted into the given map variable.
    //
    // Must be invoked as: plist_internal!(@object $map () ($($tt)*) ($($tt)*))
    //
    // We require two copies of the input tokens so that we can match on one
    // copy and trigger errors on the other copy.
    //////////////////////////////////////////////////////////////////////////

    // Done.
    (@object $object:ident () () ()) => {};

    // Insert the current entry followed by trailing comma.
    (@object $object:ident [$($key:tt)+] ($value:expr) , $($rest:tt)*) => {
        let _ = $object.insert(($($key)+).into(), $value);
        $crate::plist_internal!(@object $object () ($($rest)*) ($($rest)*));
    };

    // Current entry followed by unexpected token.
    (@object $object:ident [$($key:tt)+] ($value:expr) $unexpected:tt $($rest:tt)*) => {
        $crate::plist_unexpected!($unexpected);
    };

    // Insert the last entry without trailing comma.
    (@object $object:ident [$($key:tt)+] ($value:expr)) => {
        let _ = $object.insert(($($key)+).into(), $value);
    };

    // Next value is `null`.
    (@object $object:ident ($($key:tt)+) (: null $($rest:tt)*) $copy:tt) => {
        $crate::plist_internal!(@object $object [$($key)+] ($crate::plist_internal!(null)) $($rest)*);
    };

    // Next value is `true`.
    (@object $object:ident ($($key:tt)+) (: true $($rest:tt)*) $copy:tt) => {
        $crate::plist_internal!(@object $object [$($key)+] ($crate::plist_internal!(true)) $($rest)*);
    };

    // Next value is `false`.
    (@object $object:ident ($($key:tt)+) (: false $($rest:tt)*) $copy:tt) => {
        $crate::plist_internal!(@object $object [$($key)+] ($crate::plist_internal!(false)) $($rest)*);
    };

    // Next value is an array.
    (@object $object:ident ($($key:tt)+) (: [$($array:tt)*] $($rest:tt)*) $copy:tt) => {
        $crate::plist_internal!(@object $object [$($key)+] ($crate::plist_internal!([$($array)*])) $($rest)*);
    };

    // Next value is a map.
    (@object $object:ident ($($key:tt)+) (: {$($map:tt)*} $($rest:tt)*) $copy:tt) => {
        $crate::plist_internal!(@object $object [$($key)+] ($crate::plist_internal!({$($map)*})) $($rest)*);
    };

    // Optional insert with trailing comma: key?: expr,
    (@object $object:ident ($($key:tt)+) (:? $value:expr , $($rest:tt)*) $copy:tt) => {
        if let Some(__v) = $crate::plist_macro::plist_maybe($value) {
            let _ = $object.insert(($($key)+).into(), __v);
        }
        $crate::plist_internal!(@object $object () ($($rest)*) ($($rest)*));
    };

    // Optional insert, last entry: key?: expr
    (@object $object:ident ($($key:tt)+) (:? $value:expr) $copy:tt) => {
        if let Some(__v) = $crate::plist_macro::plist_maybe($value) {
            let _ = $object.insert(($($key)+).into(), __v);
        }
    };

    (@object $object:ident () ( :< $value:expr , $($rest:tt)*) $copy:tt) => {
        {
            let __v = $crate::plist_internal!($value);
            let __dict = $crate::plist_macro::IntoPlistDict::into_plist_dict(__v);
            for (__k, __val) in __dict {
                let _ = $object.insert(__k, __val);
            }
        }
        $crate::plist_internal!(@object $object () ($($rest)*) ($($rest)*));
    };

    // Merge: last entry `:< expr`
    (@object $object:ident () ( :< $value:expr ) $copy:tt) => {
        {
            let __v = $crate::plist_internal!($value);
            let __dict = $crate::plist_macro::IntoPlistDict::into_plist_dict(__v);
            for (__k, __val) in __dict {
                let _ = $object.insert(__k, __val);
            }
        }
    };

    // Optional merge: `:< ? expr,` — only merge if Some(...)
    (@object $object:ident () ( :< ? $value:expr , $($rest:tt)*) $copy:tt) => {
        if let Some(__dict) = $crate::plist_macro::maybe_into_dict($value) {
            for (__k, __val) in __dict {
                let _ = $object.insert(__k, __val);
            }
        }
        $crate::plist_internal!(@object $object () ($($rest)*) ($($rest)*));
    };

    // Optional merge: last entry `:< ? expr`
    (@object $object:ident () ( :< ? $value:expr ) $copy:tt) => {
        if let Some(__dict) = $crate::plist_macro::maybe_into_dict($value) {
            for (__k, __val) in __dict {
                let _ = $object.insert(__k, __val);
            }
        }
    };

    // Next value is an expression followed by comma.
    (@object $object:ident ($($key:tt)+) (: $value:expr , $($rest:tt)*) $copy:tt) => {
        $crate::plist_internal!(@object $object [$($key)+] ($crate::plist_internal!($value)) , $($rest)*);
    };

    // Last value is an expression with no trailing comma.
    (@object $object:ident ($($key:tt)+) (: $value:expr) $copy:tt) => {
        $crate::plist_internal!(@object $object [$($key)+] ($crate::plist_internal!($value)));
    };

    // Missing value for last entry. Trigger a reasonable error message.
    (@object $object:ident ($($key:tt)+) (:) $copy:tt) => {
        // "unexpected end of macro invocation"
        $crate::plist_internal!();
    };

    // Missing colon and value for last entry. Trigger a reasonable error
    // message.
    (@object $object:ident ($($key:tt)+) () $copy:tt) => {
        // "unexpected end of macro invocation"
        $crate::plist_internal!();
    };

    // Misplaced colon. Trigger a reasonable error message.
    (@object $object:ident () (: $($rest:tt)*) ($colon:tt $($copy:tt)*)) => {
        // Takes no arguments so "no rules expected the token `:`".
        $crate::plist_unexpected!($colon);
    };

    // Found a comma inside a key. Trigger a reasonable error message.
    (@object $object:ident ($($key:tt)*) (, $($rest:tt)*) ($comma:tt $($copy:tt)*)) => {
        // Takes no arguments so "no rules expected the token `,`".
        $crate::plist_unexpected!($comma);
    };

    // Key is fully parenthesized. This avoids clippy double_parens false
    // positives because the parenthesization may be necessary here.
    (@object $object:ident () (($key:expr) : $($rest:tt)*) $copy:tt) => {
        $crate::plist_internal!(@object $object ($key) (: $($rest)*) (: $($rest)*));
    };

    // Refuse to absorb colon token into key expression.
    (@object $object:ident ($($key:tt)*) (: $($unexpected:tt)+) $copy:tt) => {
        $crate::plist_expect_expr_comma!($($unexpected)+);
    };

    // Munch a token into the current key.
    (@object $object:ident ($($key:tt)*) ($tt:tt $($rest:tt)*) $copy:tt) => {
        $crate::plist_internal!(@object $object ($($key)* $tt) ($($rest)*) ($($rest)*));
    };

    //////////////////////////////////////////////////////////////////////////
    // The main implementation.
    //
    // Must be invoked as: plist_internal!($($plist)+)
    //////////////////////////////////////////////////////////////////////////

    (null) => {
        plist::Value::String("".to_string()) // plist doesn't have null, use empty string or consider other representation
    };

    (true) => {
        plist::Value::Boolean(true)
    };

    (false) => {
        plist::Value::Boolean(false)
    };

    ([]) => {
        plist::Value::Array(vec![])
    };

    ([ $($tt:tt)+ ]) => {
        plist::Value::Array($crate::plist_internal!(@array [] $($tt)+))
    };

    ({}) => {
        plist::Value::Dictionary(plist::Dictionary::new())
    };

    ({ $($tt:tt)+ }) => {
        plist::Value::Dictionary({
            let mut object = plist::Dictionary::new();
            $crate::plist_internal!(@object object () ($($tt)+) ($($tt)+));
            object
        })
    };

    // Any type that can be converted to plist::Value: numbers, strings, variables etc.
    // Must be below every other rule.
    ($other:expr) => {
        $crate::plist_macro::plist_to_value($other)
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! plist_unexpected {
    () => {};
}

#[macro_export]
#[doc(hidden)]
macro_rules! plist_expect_expr_comma {
    ($e:expr , $($tt:tt)*) => {};
}

// Helper function to convert various types to plist::Value
#[doc(hidden)]
pub fn plist_to_value<T: PlistConvertible>(value: T) -> plist::Value {
    value.to_plist_value()
}

// Trait for types that can be converted to plist::Value
pub trait PlistConvertible {
    fn to_plist_value(self) -> plist::Value;
}

// Implementations for common types
impl PlistConvertible for plist::Value {
    fn to_plist_value(self) -> plist::Value {
        self
    }
}

impl PlistConvertible for String {
    fn to_plist_value(self) -> plist::Value {
        plist::Value::String(self)
    }
}

impl PlistConvertible for &str {
    fn to_plist_value(self) -> plist::Value {
        plist::Value::String(self.to_string())
    }
}

impl PlistConvertible for i16 {
    fn to_plist_value(self) -> plist::Value {
        plist::Value::Integer(self.into())
    }
}

impl PlistConvertible for i32 {
    fn to_plist_value(self) -> plist::Value {
        plist::Value::Integer(self.into())
    }
}

impl PlistConvertible for i64 {
    fn to_plist_value(self) -> plist::Value {
        plist::Value::Integer(self.into())
    }
}

impl PlistConvertible for u16 {
    fn to_plist_value(self) -> plist::Value {
        plist::Value::Integer((self as i64).into())
    }
}

impl PlistConvertible for u32 {
    fn to_plist_value(self) -> plist::Value {
        plist::Value::Integer((self as i64).into())
    }
}

impl PlistConvertible for u64 {
    fn to_plist_value(self) -> plist::Value {
        plist::Value::Integer((self as i64).into())
    }
}

impl PlistConvertible for f32 {
    fn to_plist_value(self) -> plist::Value {
        plist::Value::Real(self as f64)
    }
}

impl PlistConvertible for f64 {
    fn to_plist_value(self) -> plist::Value {
        plist::Value::Real(self)
    }
}

impl PlistConvertible for bool {
    fn to_plist_value(self) -> plist::Value {
        plist::Value::Boolean(self)
    }
}

impl<'a> PlistConvertible for std::borrow::Cow<'a, str> {
    fn to_plist_value(self) -> plist::Value {
        plist::Value::String(self.into_owned())
    }
}
impl PlistConvertible for Vec<u8> {
    fn to_plist_value(self) -> plist::Value {
        plist::Value::Data(self)
    }
}
impl PlistConvertible for &[u8] {
    fn to_plist_value(self) -> plist::Value {
        plist::Value::Data(self.to_vec())
    }
}
impl PlistConvertible for std::time::SystemTime {
    fn to_plist_value(self) -> plist::Value {
        plist::Value::Date(self.into())
    }
}

impl<T: PlistConvertible> PlistConvertible for Vec<T> {
    fn to_plist_value(self) -> plist::Value {
        plist::Value::Array(self.into_iter().map(|item| item.to_plist_value()).collect())
    }
}

impl<T: PlistConvertible + Clone> PlistConvertible for &[T] {
    fn to_plist_value(self) -> plist::Value {
        plist::Value::Array(
            self.iter()
                .map(|item| item.clone().to_plist_value())
                .collect(),
        )
    }
}

impl<T: PlistConvertible + Clone, const N: usize> PlistConvertible for [T; N] {
    fn to_plist_value(self) -> plist::Value {
        plist::Value::Array(self.into_iter().map(|item| item.to_plist_value()).collect())
    }
}

impl<T: PlistConvertible + Clone, const N: usize> PlistConvertible for &[T; N] {
    fn to_plist_value(self) -> plist::Value {
        plist::Value::Array(
            self.iter()
                .map(|item| item.clone().to_plist_value())
                .collect(),
        )
    }
}

impl PlistConvertible for plist::Dictionary {
    fn to_plist_value(self) -> plist::Value {
        plist::Value::Dictionary(self)
    }
}

impl<K, V> PlistConvertible for std::collections::HashMap<K, V>
where
    K: Into<String>,
    V: PlistConvertible,
{
    fn to_plist_value(self) -> plist::Value {
        let mut dict = plist::Dictionary::new();
        for (key, value) in self {
            dict.insert(key.into(), value.to_plist_value());
        }
        plist::Value::Dictionary(dict)
    }
}

impl<K, V> PlistConvertible for std::collections::BTreeMap<K, V>
where
    K: Into<String>,
    V: PlistConvertible,
{
    fn to_plist_value(self) -> plist::Value {
        let mut dict = plist::Dictionary::new();
        for (key, value) in self {
            dict.insert(key.into(), value.to_plist_value());
        }
        plist::Value::Dictionary(dict)
    }
}

// Treat plain T as Some(T) and Option<T> as-is.
pub trait MaybePlist {
    fn into_option_value(self) -> Option<plist::Value>;
}

impl<T: PlistConvertible> MaybePlist for T {
    fn into_option_value(self) -> Option<plist::Value> {
        Some(self.to_plist_value())
    }
}

impl<T: PlistConvertible> MaybePlist for Option<T> {
    fn into_option_value(self) -> Option<plist::Value> {
        self.map(|v| v.to_plist_value())
    }
}

#[doc(hidden)]
pub fn plist_maybe<T: MaybePlist>(v: T) -> Option<plist::Value> {
    v.into_option_value()
}

// Convert things into a Dictionary we can merge.
pub trait IntoPlistDict {
    fn into_plist_dict(self) -> plist::Dictionary;
}

impl IntoPlistDict for plist::Dictionary {
    fn into_plist_dict(self) -> plist::Dictionary {
        self
    }
}

impl IntoPlistDict for plist::Value {
    fn into_plist_dict(self) -> plist::Dictionary {
        match self {
            plist::Value::Dictionary(d) => d,
            other => panic!("plist :< expects a dictionary, got {other:?}"),
        }
    }
}

impl<K, V> IntoPlistDict for std::collections::HashMap<K, V>
where
    K: Into<String>,
    V: PlistConvertible,
{
    fn into_plist_dict(self) -> plist::Dictionary {
        let mut d = plist::Dictionary::new();
        for (k, v) in self {
            d.insert(k.into(), v.to_plist_value());
        }
        d
    }
}

impl<K, V> IntoPlistDict for std::collections::BTreeMap<K, V>
where
    K: Into<String>,
    V: PlistConvertible,
{
    fn into_plist_dict(self) -> plist::Dictionary {
        let mut d = plist::Dictionary::new();
        for (k, v) in self {
            d.insert(k.into(), v.to_plist_value());
        }
        d
    }
}

// Optional version: T or Option<T>.
pub trait MaybeIntoPlistDict {
    fn into_option_plist_dict(self) -> Option<plist::Dictionary>;
}
impl<T: IntoPlistDict> MaybeIntoPlistDict for T {
    fn into_option_plist_dict(self) -> Option<plist::Dictionary> {
        Some(self.into_plist_dict())
    }
}
impl<T: IntoPlistDict> MaybeIntoPlistDict for Option<T> {
    fn into_option_plist_dict(self) -> Option<plist::Dictionary> {
        self.map(|t| t.into_plist_dict())
    }
}

#[doc(hidden)]
pub fn maybe_into_dict<T: MaybeIntoPlistDict>(v: T) -> Option<plist::Dictionary> {
    v.into_option_plist_dict()
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_plist_macro_basic() {
        let value = plist!({
            "name": "test",
            "count": 42,
            "active": true,
            "items": ["a", ?"b", "c"]
        });

        if let plist::Value::Dictionary(dict) = value {
            assert_eq!(
                dict.get("name"),
                Some(&plist::Value::String("test".to_string()))
            );
            assert_eq!(dict.get("count"), Some(&plist::Value::Integer(42.into())));
            assert_eq!(dict.get("active"), Some(&plist::Value::Boolean(true)));
        } else {
            panic!("Expected dictionary");
        }
    }

    #[test]
    fn test_plist_macro_with_variables() {
        let name = "dynamic";
        let count = 100;
        let items = vec!["x", "y"];
        let none: Option<u64> = None;

        let to_merge = plist!({
            "reee": "cool beans"
        });
        let maybe_merge = Some(plist!({
            "yeppers": "what did I say about yeppers",
            "replace me": 2,
        }));
        let value = plist!({
            "name": name,
            "count": count,
            "items": items,
            "omit me":? none,
            "keep me":? Some(123),
            "replace me": 1,
            :< to_merge,
            :<? maybe_merge
        });

        if let plist::Value::Dictionary(dict) = value {
            assert_eq!(
                dict.get("name"),
                Some(&plist::Value::String("dynamic".to_string()))
            );
            assert_eq!(dict.get("count"), Some(&plist::Value::Integer(100.into())));
            assert!(dict.get("omit me").is_none());
            assert_eq!(
                dict.get("keep me"),
                Some(&plist::Value::Integer(123.into()))
            );
            assert_eq!(
                dict.get("reee"),
                Some(&plist::Value::String("cool beans".to_string()))
            );
            assert_eq!(
                dict.get("yeppers"),
                Some(&plist::Value::String(
                    "what did I say about yeppers".to_string()
                ))
            );
            assert_eq!(
                dict.get("replace me"),
                Some(&plist::Value::Integer(2.into()))
            );
        } else {
            panic!("Expected dictionary");
        }
    }
}
