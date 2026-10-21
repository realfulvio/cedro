fn crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut n = 0u32;
    while n < 256 {
        let mut c = n;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 == 1 { 0xedb88320 ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        table[n as usize] = c;
        n += 1;
    }
    table
}

pub fn crc32(data: &[u8]) -> u32 {
    let table = crc_table();
    let mut c = 0xffff_ffffu32;
    for &b in data {
        c = table[((c ^ b as u32) & 0xff) as usize] ^ (c >> 8);
    }
    c ^ 0xffff_ffff
}

fn u16(n: u16) -> [u8; 2] {
    n.to_le_bytes()
}
fn u32(n: u32) -> [u8; 4] {
    n.to_le_bytes()
}

pub fn zip_store(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut locals = Vec::new();
    let mut centrals = Vec::new();
    let mut offset = 0u32;
    for (name, data) in entries {
        let base = name.rsplit(['/', '\\']).next().unwrap_or("file");
        let name_b = base.as_bytes();
        let crc = crc32(data);
        let size = data.len() as u32;
        let mut local = Vec::new();
        local.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04]);
        local.extend_from_slice(&u16(20));
        local.extend_from_slice(&u16(0x0800));
        local.extend_from_slice(&u16(0));
        local.extend_from_slice(&u16(0));
        local.extend_from_slice(&u16(0));
        local.extend_from_slice(&u32(crc));
        local.extend_from_slice(&u32(size));
        local.extend_from_slice(&u32(size));
        local.extend_from_slice(&u16(name_b.len() as u16));
        local.extend_from_slice(&u16(0));
        local.extend_from_slice(name_b);
        local.extend_from_slice(data);
        let mut central = Vec::new();
        central.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02]);
        central.extend_from_slice(&u16(20));
        central.extend_from_slice(&u16(20));
        central.extend_from_slice(&u16(0x0800));
        central.extend_from_slice(&u16(0));
        central.extend_from_slice(&u16(0));
        central.extend_from_slice(&u16(0));
        central.extend_from_slice(&u32(crc));
        central.extend_from_slice(&u32(size));
        central.extend_from_slice(&u32(size));
        central.extend_from_slice(&u16(name_b.len() as u16));
        central.extend_from_slice(&u16(0));
        central.extend_from_slice(&u16(0));
        central.extend_from_slice(&u16(0));
        central.extend_from_slice(&u16(0));
        central.extend_from_slice(&u32(0));
        central.extend_from_slice(&u32(offset));
        central.extend_from_slice(name_b);
        offset += local.len() as u32;
        locals.extend_from_slice(&local);
        centrals.extend_from_slice(&central);
    }
    let mut out = locals;
    let central_start = out.len() as u32;
    out.extend_from_slice(&centrals);
    let central_size = (out.len() as u32) - central_start;
    out.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06]);
    out.extend_from_slice(&u16(0));
    out.extend_from_slice(&u16(0));
    out.extend_from_slice(&u16(entries.len() as u16));
    out.extend_from_slice(&u16(entries.len() as u16));
    out.extend_from_slice(&u32(central_size));
    out.extend_from_slice(&u32(central_start));
    out.extend_from_slice(&u16(0));
    out
}
