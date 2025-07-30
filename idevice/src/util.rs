//! Utility Functions
//!
//! Provides helper functions for working with Apple's Property List (PLIST) format,
//! including serialization and pretty-printing utilities.
#![allow(dead_code)] // functions might not be used by all features

use plist::Value;

/// Converts a PLIST dictionary to XML-formatted bytes
///
/// # Arguments
/// * `p` - The PLIST dictionary to serialize
///
/// # Returns
/// A byte vector containing the XML representation
///
/// # Panics
/// Will panic if serialization fails (should only happen with invalid data)
///
/// # Example
/// ```rust
/// let mut dict = plist::Dictionary::new();
/// dict.insert("key".into(), "value".into());
/// let xml_bytes = plist_to_xml_bytes(&dict);
/// ```
pub fn plist_to_xml_bytes(p: &plist::Dictionary) -> Vec<u8> {
    let buf = Vec::new();
    let mut writer = std::io::BufWriter::new(buf);
    plist::to_writer_xml(&mut writer, &p).unwrap();

    writer.into_inner().unwrap()
}

/// Pretty-prints a PLIST value with indentation
///
/// # Arguments
/// * `p` - The PLIST value to format
///
/// # Returns
/// A formatted string representation
pub fn pretty_print_plist(p: &Value) -> String {
    print_plist(p, 0)
}

/// Pretty-prints a PLIST dictionary with key-value pairs
///
/// # Arguments
/// * `dict` - The dictionary to format
///
/// # Returns
/// A formatted string representation with newlines and indentation
///
/// # Example
/// ```rust
/// let mut dict = plist::Dictionary::new();
/// dict.insert("name".into(), "John".into());
/// dict.insert("age".into(), 30.into());
/// println!("{}", pretty_print_dictionary(&dict));
/// ```
pub fn pretty_print_dictionary(dict: &plist::Dictionary) -> String {
    let items: Vec<String> = dict
        .iter()
        .map(|(k, v)| format!("{}: {}", k, print_plist(v, 2)))
        .collect();
    format!("{{\n{}\n}}", items.join(",\n"))
}

/// Internal recursive function for printing PLIST values with indentation
///
/// # Arguments
/// * `p` - The PLIST value to format
/// * `indentation` - Current indentation level
///
/// # Returns
/// Formatted string representation
fn print_plist(p: &Value, indentation: usize) -> String {
    let indent = " ".repeat(indentation);
    match p {
        Value::Array(vec) => {
            let items: Vec<String> = vec
                .iter()
                .map(|v| {
                    format!(
                        "{}{}",
                        " ".repeat(indentation + 2),
                        print_plist(v, indentation + 2)
                    )
                })
                .collect();
            format!("[\n{}\n{}]", items.join(",\n"), indent)
        }
        Value::Dictionary(dict) => {
            let items: Vec<String> = dict
                .iter()
                .map(|(k, v)| {
                    format!(
                        "{}{}: {}",
                        " ".repeat(indentation + 2),
                        k,
                        print_plist(v, indentation + 2)
                    )
                })
                .collect();
            format!("{{\n{}\n{}}}", items.join(",\n"), indent)
        }
        Value::Boolean(b) => format!("{b}"),
        Value::Data(vec) => {
            let len = vec.len();
            let preview: String = vec
                .iter()
                .take(20)
                .map(|b| format!("{b:02X}"))
                .collect::<Vec<String>>()
                .join(" ");
            if len > 20 {
                format!("Data({preview}... Len: {len})")
            } else {
                format!("Data({preview} Len: {len})")
            }
        }
        Value::Date(date) => format!("Date({})", date.to_xml_format()),
        Value::Real(f) => format!("{f}"),
        Value::Integer(i) => format!("{i}"),
        Value::String(s) => format!("\"{s}\""),
        Value::Uid(_uid) => "Uid(?)".to_string(),
        _ => "Unknown".to_string(),
    }
}

#[macro_export]
macro_rules! obf {
    ($lit:literal) => {{
        #[cfg(feature = "obfuscate")]
        {
            std::borrow::Cow::Owned(obfstr::obfstr!($lit).to_string())
        }
        #[cfg(not(feature = "obfuscate"))]
        {
            std::borrow::Cow::Borrowed($lit)
        }
    }};
}
