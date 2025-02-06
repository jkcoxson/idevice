// Jackson Coxson

use plist::Value;

pub fn plist_to_bytes(p: &plist::Dictionary) -> Vec<u8> {
    let buf = Vec::new();
    let mut writer = std::io::BufWriter::new(buf);
    plist::to_writer_xml(&mut writer, &p).unwrap();

    writer.into_inner().unwrap()
}

pub fn pretty_print_plist(p: &Value) -> String {
    print_plist(p, 0)
}

pub fn pretty_print_dictionary(dict: &plist::Dictionary) -> String {
    let items: Vec<String> = dict
        .iter()
        .map(|(k, v)| format!("{}: {}", k, print_plist(v, 2)))
        .collect();
    format!("{{\n{}\n}}", items.join(",\n"))
}

fn print_plist(p: &Value, indentation: usize) -> String {
    let indent = " ".repeat(indentation);
    match p {
        Value::Array(vec) => {
            let items: Vec<String> = vec
                .iter()
                .map(|v| print_plist(v, indentation + 2))
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
            format!("{}{{\n{}\n{}}}", indent, items.join(",\n"), indent)
        }
        Value::Boolean(b) => format!("{}", b),
        Value::Data(vec) => {
            let len = vec.len();
            let preview: String = vec
                .iter()
                .take(20)
                .map(|b| format!("{:02X}", b))
                .collect::<Vec<String>>()
                .join(" ");
            format!("Data({}... Len: {})", preview, len)
        }
        Value::Date(date) => format!("Date({})", date.to_xml_format()),
        Value::Real(f) => format!("{}", f),
        Value::Integer(i) => format!("{}", i),
        Value::String(s) => format!("\"{}\"", s),
        Value::Uid(_uid) => "Uid(?)".to_string(),
        _ => "Unknown".to_string(),
    }
}
