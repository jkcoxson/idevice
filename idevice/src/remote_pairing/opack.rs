// Jackson Coxson

use plist::Value;

pub fn plist_to_opack(value: &Value) -> Vec<u8> {
    let mut buf = Vec::new();
    plist_to_opack_inner(value, &mut buf);

    buf
}

fn plist_to_opack_inner(node: &Value, buf: &mut Vec<u8>) {
    match node {
        Value::Dictionary(dict) => {
            let count = dict.len() as u32;
            let blen = if count < 15 {
                (count as u8).wrapping_sub(32)
            } else {
                0xEF
            };
            buf.push(blen);

            for (key, val) in dict {
                plist_to_opack_inner(&Value::String(key.clone()), buf);
                plist_to_opack_inner(val, buf);
            }

            if count > 14 {
                buf.push(0x03);
            }
        }
        Value::Array(array) => {
            let count = array.len() as u32;
            let blen = if count < 15 {
                (count as u8).wrapping_sub(48)
            } else {
                0xDF
            };
            buf.push(blen);

            for val in array {
                plist_to_opack_inner(val, buf);
            }

            if count > 14 {
                buf.push(0x03); // Terminator
            }
        }
        Value::Boolean(b) => {
            let bval = if *b { 1u8 } else { 2u8 };
            buf.push(bval);
        }
        Value::Integer(integer) => {
            let u64val = integer.as_unsigned().unwrap_or(0);

            if u64val <= u8::MAX as u64 {
                let u8val = u64val as u8;
                if u8val > 0x27 {
                    buf.push(0x30);
                    buf.push(u8val);
                } else {
                    buf.push(u8val + 8);
                }
            } else if u64val <= u32::MAX as u64 {
                buf.push(0x32);
                buf.extend_from_slice(&(u64val as u32).to_le_bytes());
            } else {
                buf.push(0x33);
                buf.extend_from_slice(&u64val.to_le_bytes());
            }
        }
        Value::Real(real) => {
            let dval = *real;
            let fval = dval as f32;

            if fval as f64 == dval {
                buf.push(0x35);
                buf.extend_from_slice(&fval.to_bits().swap_bytes().to_ne_bytes());
            } else {
                buf.push(0x36);
                buf.extend_from_slice(&dval.to_bits().swap_bytes().to_ne_bytes());
            }
        }
        Value::String(s) => {
            let bytes = s.as_bytes();
            let len = bytes.len();

            if len > 0x20 {
                if len <= 0xFF {
                    buf.push(0x61);
                    buf.push(len as u8);
                } else if len <= 0xFFFF {
                    buf.push(0x62);
                    buf.extend_from_slice(&(len as u16).to_le_bytes());
                } else if len <= 0xFFFFFFFF {
                    buf.push(0x63);
                    buf.extend_from_slice(&(len as u32).to_le_bytes());
                } else {
                    buf.push(0x64);
                    buf.extend_from_slice(&(len as u64).to_le_bytes());
                }
            } else {
                buf.push(0x40 + len as u8);
            }
            buf.extend_from_slice(bytes);
        }
        Value::Data(data) => {
            let len = data.len();
            if len > 0x20 {
                if len <= 0xFF {
                    buf.push(0x91);
                    buf.push(len as u8);
                } else if len <= 0xFFFF {
                    buf.push(0x92);
                    buf.extend_from_slice(&(len as u16).to_le_bytes());
                } else if len <= 0xFFFFFFFF {
                    buf.push(0x93);
                    buf.extend_from_slice(&(len as u32).to_le_bytes());
                } else {
                    buf.push(0x94);
                    buf.extend_from_slice(&(len as u64).to_le_bytes());
                }
            } else {
                buf.push(0x70 + len as u8);
            }
            buf.extend_from_slice(data);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn t1() {
        let v = crate::plist!({
            "altIRK": b"\xe9\xe8-\xc0jIykVoT\x00\x19\xb1\xc7{".to_vec(),
            "btAddr": "11:22:33:44:55:66",
            "mac": b"\x11\x22\x33\x44\x55\x66".to_vec(),
            "remotepairing_serial_number": "AAAAAAAAAAAA",
            "accountID": "lolsssss",
            "model": "computer-model",
            "name": "reeeee",
        });

        let res = super::plist_to_opack(&v);

        let expected = [
            0xe7, 0x46, 0x61, 0x6c, 0x74, 0x49, 0x52, 0x4b, 0x80, 0xe9, 0xe8, 0x2d, 0xc0, 0x6a,
            0x49, 0x79, 0x6b, 0x56, 0x6f, 0x54, 0x00, 0x19, 0xb1, 0xc7, 0x7b, 0x46, 0x62, 0x74,
            0x41, 0x64, 0x64, 0x72, 0x51, 0x31, 0x31, 0x3a, 0x32, 0x32, 0x3a, 0x33, 0x33, 0x3a,
            0x34, 0x34, 0x3a, 0x35, 0x35, 0x3a, 0x36, 0x36, 0x43, 0x6d, 0x61, 0x63, 0x76, 0x11,
            0x22, 0x33, 0x44, 0x55, 0x66, 0x5b, 0x72, 0x65, 0x6d, 0x6f, 0x74, 0x65, 0x70, 0x61,
            0x69, 0x72, 0x69, 0x6e, 0x67, 0x5f, 0x73, 0x65, 0x72, 0x69, 0x61, 0x6c, 0x5f, 0x6e,
            0x75, 0x6d, 0x62, 0x65, 0x72, 0x4c, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41,
            0x41, 0x41, 0x41, 0x41, 0x49, 0x61, 0x63, 0x63, 0x6f, 0x75, 0x6e, 0x74, 0x49, 0x44,
            0x48, 0x6c, 0x6f, 0x6c, 0x73, 0x73, 0x73, 0x73, 0x73, 0x45, 0x6d, 0x6f, 0x64, 0x65,
            0x6c, 0x4e, 0x63, 0x6f, 0x6d, 0x70, 0x75, 0x74, 0x65, 0x72, 0x2d, 0x6d, 0x6f, 0x64,
            0x65, 0x6c, 0x44, 0x6e, 0x61, 0x6d, 0x65, 0x46, 0x72, 0x65, 0x65, 0x65, 0x65, 0x65,
        ];

        println!("{res:02X?}");
        assert_eq!(res, expected);
    }
}
