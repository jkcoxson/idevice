// Jackson Coxson

pub fn plist_to_bytes(p: &plist::Dictionary) -> Vec<u8> {
    let buf = Vec::new();
    let mut writer = std::io::BufWriter::new(buf);
    plist::to_writer_xml(&mut writer, &p).unwrap();

    writer.into_inner().unwrap()
}
