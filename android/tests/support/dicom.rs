// Fixture sintetica usata soltanto dai test e dal renderer host; nessun dato clinico.
pub fn crea(path: &std::path::Path) {
    use cedro::dicom::{tag,DicomFile,Element,Value,Syntax,TS_EXPLICIT,PIXEL_DATA};
    let el=|tag,vr,bytes:Vec<u8>|Element{tag,vr,value:Value::Bytes(bytes)};
    let mut body=vec![
        el(tag(0x0008,0x0016),*b"UI",b"1.2.840.10008.5.1.4.1.1.7\0".to_vec()),
        el(tag(0x0008,0x0018),*b"UI",b"2.25.1234567890\0".to_vec()),
        el(tag(0x0028,0x0004),*b"CS",b"MONOCHROME2 ".to_vec()),
        el(tag(0x0028,0x0008),*b"IS",b"2 ".to_vec()),
        el(tag(0x0028,0x1050),*b"DS",b"128 ".to_vec()),
        el(tag(0x0028,0x1051),*b"DS",b"256 ".to_vec()),
    ];
    for (e,v) in [(0x0002,1u16),(0x0010,64),(0x0011,64),(0x0100,16),(0x0101,16),(0x0102,15),(0x0103,0)] {
        body.push(el(tag(0x0028,e),*b"US",v.to_le_bytes().to_vec()));
    }
    let pixels:Vec<u8>=(0..2).flat_map(|frame|(0..64u16).flat_map(move|y|(0..64u16).flat_map(move|x|{
        let value=if frame==0{x*4}else{y*4};value.to_le_bytes()
    }))).collect();
    body.push(el(PIXEL_DATA,*b"OW",pixels));
    let f=DicomFile{meta:vec![],body,syntax:Syntax::ExplicitLE,ts:TS_EXPLICIT.into()};
    std::fs::write(path,f.write()).unwrap();
}
