//! H.264: lettura delle intestazioni (SPS, PPS, fetta) e tenuta dei fotogrammi di riferimento.
//!
//! Serve alla decodifica con la scheda video (`vadecode`): la scheda decodifica i pezzi di immagine, ma
//! chi la guida deve dire quali fotogrammi fanno da riferimento, in che ordine e quando mostrare i
//! fotogrammi. Qui c'è solo quello che serve ai video progressivi a 8 bit 4:2:0 (telefoni, fotocamere,
//! schermate registrate, YouTube). Tutto il resto (interlacciato, 10 bit, matrici di scala, gruppi di
//! fette, buchi nei numeri dei fotogrammi) si rifiuta con un errore, e il video si decodifica come prima
//! con OpenH264.

use std::collections::HashMap;

/// Legge i bit di un NAL già privato dei byte di protezione (00 00 03 → 00 00).
struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8]) -> Self {
        Bits { data, pos: 0 }
    }

    fn bit(&mut self) -> Result<u32, String> {
        let byte = self.data.get(self.pos / 8).ok_or("H.264: intestazione troppo corta")?;
        let bit = (byte >> (7 - self.pos % 8)) & 1;
        self.pos += 1;
        Ok(u32::from(bit))
    }

    fn flag(&mut self) -> Result<bool, String> {
        Ok(self.bit()? == 1)
    }

    fn bits(&mut self, n: u32) -> Result<u32, String> {
        let mut value = 0u32;
        for _ in 0..n {
            value = (value << 1) | self.bit()?;
        }
        Ok(value)
    }

    fn skip(&mut self, n: usize) {
        self.pos += n;
    }

    fn ue(&mut self) -> Result<u32, String> {
        let mut zeros = 0;
        while self.bit()? == 0 {
            zeros += 1;
            if zeros > 32 {
                return Err("H.264: numero non valido".into());
            }
        }
        if zeros == 0 {
            return Ok(0);
        }
        let rest = self.bits(zeros)?;
        Ok(((1u64 << zeros) - 1 + u64::from(rest)).min(u64::from(u32::MAX)) as u32)
    }

    fn se(&mut self) -> Result<i32, String> {
        let k = i64::from(self.ue()?);
        Ok(if k % 2 == 1 { (k + 1) / 2 } else { -(k / 2) } as i32)
    }

    /// Vero se dopo la posizione c'è ancora altro che il bit di chiusura.
    fn more_data(&self) -> bool {
        let Some(last) = self.data.iter().rposition(|&b| b != 0) else {
            return false;
        };
        let stop = last * 8 + 7 - self.data[last].trailing_zeros() as usize;
        self.pos < stop
    }
}

/// Toglie i byte di protezione. Restituisce i dati e, per ognuno tolto, la posizione nei dati puliti
/// del byte che lo seguiva (serve a rimettere i bit al loro posto nel NAL originale).
fn unescape(nal: &[u8]) -> (Vec<u8>, Vec<usize>) {
    let mut out = Vec::with_capacity(nal.len());
    let mut removed = Vec::new();
    let mut zeros = 0;
    for &b in nal {
        if zeros >= 2 && b == 3 {
            removed.push(out.len());
            zeros = 0;
            continue;
        }
        zeros = if b == 0 { zeros + 1 } else { 0 };
        out.push(b);
    }
    (out, removed)
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Sps {
    pub profile_idc: u8,
    pub constraint_flags: u8,
    pub level_idc: u8,
    pub id: u32,
    pub log2_max_frame_num: u32,
    pub poc_type: u32,
    pub log2_max_poc_lsb: u32,
    pub delta_pic_order_always_zero: bool,
    pub offset_for_non_ref_pic: i32,
    pub offset_for_top_to_bottom: i32,
    pub offsets_for_ref_frame: Vec<i32>,
    pub max_num_ref_frames: u32,
    pub gaps_allowed: bool,
    pub mb_width: u32,
    pub mb_height: u32,
    pub direct_8x8_inference: bool,
    /// Ritaglio in pixel: sinistra, destra, sopra, sotto.
    pub crop: [u32; 4],
    pub num_reorder_frames: Option<u32>,
    pub max_dec_frame_buffering: Option<u32>,
}

impl Sps {
    pub(crate) fn width(&self) -> u32 {
        self.mb_width * 16 - self.crop[0] - self.crop[1]
    }

    pub(crate) fn height(&self) -> u32 {
        self.mb_height * 16 - self.crop[2] - self.crop[3]
    }

    fn max_frame_num(&self) -> i64 {
        1i64 << self.log2_max_frame_num
    }

    /// Quanti fotogrammi sta nella memoria del decodificatore (livello e dimensioni).
    fn dpb_frames(&self) -> u32 {
        let mbs: u32 = match self.level_idc {
            9 | 10 => 396,
            11 if self.constraint_flags & 0x10 != 0 => 396,
            11 => 900,
            12 | 13 | 20 => 2376,
            21 => 4752,
            22 | 30 => 8100,
            31 => 18000,
            32 => 20480,
            40 | 41 => 32768,
            42 => 34816,
            50 => 110_400,
            51 | 52 => 184_320,
            _ => 184_320,
        };
        (mbs / (self.mb_width * self.mb_height).max(1)).clamp(1, 16)
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Pps {
    pub id: u32,
    pub sps_id: u32,
    pub cabac: bool,
    pub bottom_field_pic_order_present: bool,
    pub num_ref_idx_default: [u32; 2],
    pub weighted_pred: bool,
    pub weighted_bipred_idc: u32,
    pub pic_init_qp_minus26: i32,
    pub pic_init_qs_minus26: i32,
    pub chroma_qp_index_offset: i32,
    pub second_chroma_qp_index_offset: i32,
    pub deblocking_filter_control_present: bool,
    pub constrained_intra_pred: bool,
    pub redundant_pic_cnt_present: bool,
    pub transform_8x8_mode: bool,
}

fn parse_vui(r: &mut Bits, sps: &mut Sps) -> Result<(), String> {
    if r.flag()? {
        // aspect_ratio_info_present_flag
        if r.bits(8)? == 255 {
            r.skip(32);
        }
    }
    if r.flag()? {
        r.skip(1); // overscan_appropriate_flag
    }
    if r.flag()? {
        // video_signal_type_present_flag
        r.skip(4);
        if r.flag()? {
            r.skip(24);
        }
    }
    if r.flag()? {
        r.ue()?;
        r.ue()?;
    }
    if r.flag()? {
        // timing_info_present_flag
        r.skip(65);
    }
    let nal_hrd = r.flag()?;
    if nal_hrd {
        skip_hrd(r)?;
    }
    let vcl_hrd = r.flag()?;
    if vcl_hrd {
        skip_hrd(r)?;
    }
    if nal_hrd || vcl_hrd {
        r.skip(1); // low_delay_hrd_flag
    }
    r.skip(1); // pic_struct_present_flag
    if r.flag()? {
        // bitstream_restriction_flag
        r.skip(1);
        r.ue()?;
        r.ue()?;
        r.ue()?;
        r.ue()?;
        sps.num_reorder_frames = Some(r.ue()?);
        sps.max_dec_frame_buffering = Some(r.ue()?);
    }
    Ok(())
}

fn skip_hrd(r: &mut Bits) -> Result<(), String> {
    let count = r.ue()? + 1;
    r.skip(8);
    for _ in 0..count.min(32) {
        r.ue()?;
        r.ue()?;
        r.skip(1);
    }
    r.skip(20);
    Ok(())
}

/// `nal` comincia dal byte di intestazione del NAL (tipo 7).
pub(crate) fn parse_sps(nal: &[u8]) -> Result<Sps, String> {
    let (rbsp, _) = unescape(nal.get(1..).ok_or("H.264: SPS vuoto")?);
    let mut r = Bits::new(&rbsp);
    let mut sps = Sps { profile_idc: r.bits(8)? as u8, constraint_flags: r.bits(8)? as u8, level_idc: r.bits(8)? as u8, ..Default::default() };
    sps.id = r.ue()?;
    let mut chroma_format = 1;
    if [100, 110, 122, 244, 44, 83, 86, 118, 128, 138, 139, 134, 135].contains(&sps.profile_idc) {
        chroma_format = r.ue()?;
        if chroma_format == 3 {
            return Err("H.264: 4:4:4 non gestito dalla scheda".into());
        }
        let (luma, chroma) = (r.ue()?, r.ue()?);
        if luma != 0 || chroma != 0 {
            return Err("H.264: più di 8 bit non gestito dalla scheda".into());
        }
        r.skip(1); // qpprime_y_zero_transform_bypass_flag
        if r.flag()? {
            return Err("H.264: matrici di scala non gestite dalla scheda".into());
        }
    }
    if chroma_format != 1 {
        return Err("H.264: solo 4:2:0 gestito dalla scheda".into());
    }
    sps.log2_max_frame_num = r.ue()? + 4;
    sps.poc_type = r.ue()?;
    match sps.poc_type {
        0 => sps.log2_max_poc_lsb = r.ue()? + 4,
        1 => {
            sps.delta_pic_order_always_zero = r.flag()?;
            sps.offset_for_non_ref_pic = r.se()?;
            sps.offset_for_top_to_bottom = r.se()?;
            let n = r.ue()?;
            if n > 255 {
                return Err("H.264: SPS non valido".into());
            }
            for _ in 0..n {
                sps.offsets_for_ref_frame.push(r.se()?);
            }
        }
        2 => {}
        _ => return Err("H.264: SPS non valido".into()),
    }
    sps.max_num_ref_frames = r.ue()?;
    sps.gaps_allowed = r.flag()?;
    sps.mb_width = r.ue()? + 1;
    let map_units = r.ue()? + 1;
    if !r.flag()? {
        return Err("H.264: interlacciato non gestito dalla scheda".into());
    }
    sps.mb_height = map_units;
    sps.direct_8x8_inference = r.flag()?;
    if r.flag()? {
        // frame_cropping_flag: unità di 2 pixel per 4:2:0 progressivo
        for c in sps.crop.iter_mut() {
            *c = r.ue()?.saturating_mul(2);
        }
    }
    if sps.mb_width > 256 || sps.mb_height > 256 || sps.crop[0] + sps.crop[1] >= sps.mb_width * 16 || sps.crop[2] + sps.crop[3] >= sps.mb_height * 16 {
        return Err("H.264: dimensioni non valide".into());
    }
    if sps.max_num_ref_frames > 16 {
        return Err("H.264: SPS non valido".into());
    }
    if r.flag()? {
        // I campi del VUI dopo questo sono facoltativi: un errore qui non rende inutilizzabile l'SPS.
        let mut with_vui = sps.clone();
        if parse_vui(&mut r, &mut with_vui).is_ok() {
            sps = with_vui;
        }
    }
    Ok(sps)
}

pub(crate) fn parse_pps(nal: &[u8], chroma_format_idc: u32) -> Result<Pps, String> {
    let _ = chroma_format_idc;
    let (rbsp, _) = unescape(nal.get(1..).ok_or("H.264: PPS vuoto")?);
    let mut r = Bits::new(&rbsp);
    let mut pps = Pps { id: r.ue()?, sps_id: r.ue()?, cabac: r.flag()?, bottom_field_pic_order_present: r.flag()?, ..Default::default() };
    if r.ue()? != 0 {
        return Err("H.264: gruppi di fette non gestiti dalla scheda".into());
    }
    pps.num_ref_idx_default = [r.ue()? + 1, r.ue()? + 1];
    pps.weighted_pred = r.flag()?;
    pps.weighted_bipred_idc = r.bits(2)?;
    pps.pic_init_qp_minus26 = r.se()?;
    pps.pic_init_qs_minus26 = r.se()?;
    pps.chroma_qp_index_offset = r.se()?;
    pps.second_chroma_qp_index_offset = pps.chroma_qp_index_offset;
    pps.deblocking_filter_control_present = r.flag()?;
    pps.constrained_intra_pred = r.flag()?;
    pps.redundant_pic_cnt_present = r.flag()?;
    if r.more_data() {
        pps.transform_8x8_mode = r.flag()?;
        if r.flag()? {
            return Err("H.264: matrici di scala non gestite dalla scheda".into());
        }
        pps.second_chroma_qp_index_offset = r.se()?;
    }
    Ok(pps)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SliceType {
    P,
    B,
    I,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct WeightTable {
    pub luma_log2_denom: u8,
    pub chroma_log2_denom: u8,
    /// Per lista (0, 1) e per riferimento: flag, peso, scarto.
    pub luma: [Vec<(bool, i16, i16)>; 2],
    pub chroma: [Vec<(bool, [i16; 2], [i16; 2])>; 2],
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Mmco {
    ShortUnused(u32),
    LongUnused(u32),
    ShortToLong(u32, u32),
    MaxLong(u32),
    ClearAll,
    CurrentLong(u32),
}

#[derive(Clone, Debug)]
pub(crate) struct SliceHeader {
    pub nal_ref_idc: u8,
    pub idr: bool,
    pub first_mb: u32,
    pub slice_type: SliceType,
    pub pps_id: u32,
    pub frame_num: u32,
    pub idr_pic_id: u32,
    pub poc_lsb: u32,
    pub delta_poc_bottom: i32,
    pub delta_poc: [i32; 2],
    pub redundant_pic_cnt: u32,
    pub direct_spatial: bool,
    pub num_ref_idx_override: bool,
    pub num_ref_idx_active: [u32; 2],
    /// Per lista: (comando, valore) di riordino.
    pub reorder: [Vec<(u32, u32)>; 2],
    pub weights: Option<WeightTable>,
    pub no_output_of_prior_pics: bool,
    pub long_term_reference: bool,
    pub mmco: Vec<Mmco>,
    pub adaptive_marking: bool,
    pub cabac_init_idc: u32,
    pub slice_qp_delta: i32,
    pub disable_deblocking_filter_idc: u32,
    pub alpha_c0_offset_div2: i32,
    pub beta_offset_div2: i32,
    /// Bit dall'inizio del NAL (byte di intestazione compreso, byte di protezione compresi) al primo dato.
    pub data_bit_offset: u32,
}

fn parse_weights(r: &mut Bits, slice_type: SliceType, active: [u32; 2]) -> Result<WeightTable, String> {
    let mut table = WeightTable { luma_log2_denom: r.ue()?.min(7) as u8, ..Default::default() };
    table.chroma_log2_denom = r.ue()?.min(7) as u8;
    let lists = if slice_type == SliceType::B { 2 } else { 1 };
    for list in 0..lists {
        for _ in 0..active[list].min(32) {
            let (mut lflag, mut lw, mut lo) = (false, 1i16 << table.luma_log2_denom, 0i16);
            if r.flag()? {
                lflag = true;
                lw = r.se()? as i16;
                lo = r.se()? as i16;
            }
            table.luma[list].push((lflag, lw, lo));
            let (mut cflag, mut cw, mut co) = (false, [1i16 << table.chroma_log2_denom; 2], [0i16; 2]);
            if r.flag()? {
                cflag = true;
                for j in 0..2 {
                    cw[j] = r.se()? as i16;
                    co[j] = r.se()? as i16;
                }
            }
            table.chroma[list].push((cflag, cw, co));
        }
    }
    Ok(table)
}

/// `nal` comincia dal byte di intestazione del NAL (tipo 1 o 5).
fn parse_slice(nal: &[u8], spss: &HashMap<u32, Sps>, ppss: &HashMap<u32, Pps>) -> Result<SliceHeader, String> {
    let header = *nal.first().ok_or("H.264: fetta vuota")?;
    let nal_type = header & 0x1f;
    let (rbsp, removed) = unescape(&nal[1..]);
    let mut r = Bits::new(&rbsp);
    let first_mb = r.ue()?;
    let kind = r.ue()? % 5;
    let slice_type = match kind {
        0 => SliceType::P,
        1 => SliceType::B,
        2 => SliceType::I,
        _ => return Err("H.264: fette SP/SI non gestite dalla scheda".into()),
    };
    let pps_id = r.ue()?;
    let pps = ppss.get(&pps_id).ok_or("H.264: PPS mancante")?;
    let sps = spss.get(&pps.sps_id).ok_or("H.264: SPS mancante")?;
    let frame_num = r.bits(sps.log2_max_frame_num)?;
    let idr = nal_type == 5;
    let nal_ref_idc = (header >> 5) & 3;
    let mut h = SliceHeader {
        nal_ref_idc,
        idr,
        first_mb,
        slice_type,
        pps_id,
        frame_num,
        idr_pic_id: 0,
        poc_lsb: 0,
        delta_poc_bottom: 0,
        delta_poc: [0; 2],
        redundant_pic_cnt: 0,
        direct_spatial: false,
        num_ref_idx_override: false,
        num_ref_idx_active: pps.num_ref_idx_default,
        reorder: [Vec::new(), Vec::new()],
        weights: None,
        no_output_of_prior_pics: false,
        long_term_reference: false,
        mmco: Vec::new(),
        adaptive_marking: false,
        cabac_init_idc: 0,
        slice_qp_delta: 0,
        disable_deblocking_filter_idc: 0,
        alpha_c0_offset_div2: 0,
        beta_offset_div2: 0,
        data_bit_offset: 0,
    };
    if idr {
        h.idr_pic_id = r.ue()?;
    }
    if sps.poc_type == 0 {
        h.poc_lsb = r.bits(sps.log2_max_poc_lsb)?;
        if pps.bottom_field_pic_order_present {
            h.delta_poc_bottom = r.se()?;
        }
    }
    if sps.poc_type == 1 && !sps.delta_pic_order_always_zero {
        h.delta_poc[0] = r.se()?;
        if pps.bottom_field_pic_order_present {
            h.delta_poc[1] = r.se()?;
        }
    }
    if pps.redundant_pic_cnt_present {
        h.redundant_pic_cnt = r.ue()?;
    }
    if slice_type == SliceType::B {
        h.direct_spatial = r.flag()?;
    }
    if slice_type != SliceType::I {
        h.num_ref_idx_override = r.flag()?;
        if h.num_ref_idx_override {
            h.num_ref_idx_active[0] = r.ue()? + 1;
            if slice_type == SliceType::B {
                h.num_ref_idx_active[1] = r.ue()? + 1;
            }
        }
        if slice_type != SliceType::B {
            h.num_ref_idx_active[1] = 0;
        }
        if h.num_ref_idx_active[0] > 32 || h.num_ref_idx_active[1] > 32 {
            return Err("H.264: troppi riferimenti".into());
        }
        let lists = if slice_type == SliceType::B { 2 } else { 1 };
        for list in 0..lists {
            if r.flag()? {
                loop {
                    let idc = r.ue()?;
                    if idc == 3 {
                        break;
                    }
                    if idc > 3 || h.reorder[list].len() > 64 {
                        return Err("H.264: riordino non valido".into());
                    }
                    h.reorder[list].push((idc, r.ue()?));
                }
            }
        }
    } else {
        h.num_ref_idx_active = [0, 0];
    }
    if (pps.weighted_pred && slice_type == SliceType::P) || (pps.weighted_bipred_idc == 1 && slice_type == SliceType::B) {
        h.weights = Some(parse_weights(&mut r, slice_type, h.num_ref_idx_active)?);
    }
    if nal_ref_idc != 0 {
        if idr {
            h.no_output_of_prior_pics = r.flag()?;
            h.long_term_reference = r.flag()?;
        } else {
            h.adaptive_marking = r.flag()?;
            if h.adaptive_marking {
                loop {
                    let op = r.ue()?;
                    match op {
                        0 => break,
                        1 => h.mmco.push(Mmco::ShortUnused(r.ue()?)),
                        2 => h.mmco.push(Mmco::LongUnused(r.ue()?)),
                        3 => {
                            let diff = r.ue()?;
                            h.mmco.push(Mmco::ShortToLong(diff, r.ue()?));
                        }
                        4 => h.mmco.push(Mmco::MaxLong(r.ue()?)),
                        5 => h.mmco.push(Mmco::ClearAll),
                        6 => h.mmco.push(Mmco::CurrentLong(r.ue()?)),
                        _ => return Err("H.264: comando di riferimento non valido".into()),
                    }
                    if h.mmco.len() > 64 {
                        return Err("H.264: troppi comandi di riferimento".into());
                    }
                }
            }
        }
    }
    if pps.cabac && slice_type != SliceType::I {
        h.cabac_init_idc = r.ue()?;
    }
    h.slice_qp_delta = r.se()?;
    if pps.deblocking_filter_control_present {
        h.disable_deblocking_filter_idc = r.ue()?;
        if h.disable_deblocking_filter_idc != 1 {
            h.alpha_c0_offset_div2 = r.se()?;
            h.beta_offset_div2 = r.se()?;
        }
    }
    // Dalla fine dell'intestazione alla posizione nel NAL originale: il byte di intestazione e i byte di
    // protezione incontrati prima contano.
    let byte = r.pos / 8;
    let escapes = removed.iter().filter(|&&at| at <= byte).count();
    h.data_bit_offset = (r.pos + 8 + 8 * escapes) as u32;
    Ok(h)
}

/// Un fotogramma di riferimento, o in attesa di essere mostrato, e la superficie della scheda che lo tiene.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RefPic {
    pub slot: usize,
    pub frame_num: u32,
    /// Dall'indice a lungo termine se c'è.
    pub long_idx: Option<u32>,
    pub poc: [i32; 2],
}

#[derive(Clone, Debug)]
pub(crate) struct SliceJob {
    /// Il NAL com'è nel file, byte di intestazione compreso.
    pub nal: Vec<u8>,
    pub header: SliceHeader,
    pub list0: Vec<Option<RefPic>>,
    pub list1: Vec<Option<RefPic>>,
}

/// Un fotogramma da decodificare, con tutto quello che serve alla scheda.
#[derive(Clone, Debug)]
pub(crate) struct Picture {
    pub sps: Sps,
    pub pps: Pps,
    pub slot: usize,
    pub frame_num: u32,
    pub poc: [i32; 2],
    pub is_ref: bool,
    pub idr: bool,
    /// I riferimenti nella memoria prima di questo fotogramma.
    pub refs: Vec<RefPic>,
    pub slices: Vec<SliceJob>,
    poc_msb: i32,
    frame_num_offset: i64,
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    pic: RefPic,
    short_ref: bool,
    waiting: bool,
}

impl Entry {
    fn is_ref(&self) -> bool {
        self.short_ref || self.pic.long_idx.is_some()
    }

    fn poc(&self) -> i32 {
        self.pic.poc[0].min(self.pic.poc[1])
    }
}

/// Lo stato della lettura di un flusso: parametri, memoria dei fotogrammi, ordine di uscita.
pub(crate) struct Stream {
    spss: HashMap<u32, Sps>,
    ppss: HashMap<u32, Pps>,
    active: Option<Sps>,
    entries: Vec<Entry>,
    slots: usize,
    reorder_limit: usize,
    prev_ref_frame_num: u32,
    prev_frame_num: u32,
    prev_frame_num_offset: i64,
    prev_poc_msb: i32,
    prev_poc_lsb: i32,
    max_long_idx: i64,
    seen_idr: bool,
    output: Vec<RefPic>,
}

impl Stream {
    pub(crate) fn new() -> Self {
        Stream {
            spss: HashMap::new(),
            ppss: HashMap::new(),
            active: None,
            entries: Vec::new(),
            slots: 0,
            reorder_limit: 0,
            prev_ref_frame_num: 0,
            prev_frame_num: 0,
            prev_frame_num_offset: 0,
            prev_poc_msb: 0,
            prev_poc_lsb: 0,
            max_long_idx: -1,
            seen_idr: false,
            output: Vec::new(),
        }
    }

    /// Gli SPS e PPS del file (avcC) e quelli che arrivano dentro il flusso.
    pub(crate) fn add_parameter_set(&mut self, nal: &[u8]) -> Result<(), String> {
        match nal.first().map(|b| b & 0x1f) {
            Some(7) => {
                let sps = parse_sps(nal)?;
                self.spss.insert(sps.id, sps);
            }
            Some(8) => {
                let pps = parse_pps(nal, 1)?;
                self.ppss.insert(pps.id, pps);
            }
            _ => {}
        }
        Ok(())
    }

    /// Sceglie lo SPS del primo PPS noto: serve per aprire la scheda prima del primo fotogramma.
    pub(crate) fn activate_first(&mut self) -> Result<Sps, String> {
        let pps = self.ppss.values().min_by_key(|p| p.id).ok_or("H.264: PPS mancante")?;
        let sps = self.spss.get(&pps.sps_id).ok_or("H.264: SPS mancante")?.clone();
        self.set_active(&sps);
        Ok(sps)
    }

    fn set_active(&mut self, sps: &Sps) {
        self.reorder_limit = sps.num_reorder_frames.unwrap_or_else(|| sps.dpb_frames()).min(16) as usize;
        self.slots = sps.max_num_ref_frames.max(1) as usize + self.reorder_limit + 2;
        self.active = Some(sps.clone());
    }

    /// Quanti buffer della scheda servono.
    pub(crate) fn slots(&self) -> usize {
        self.slots
    }

    /// I fotogrammi pronti da mostrare, in ordine. Si leggono dalla scheda prima di decodificare altro.
    pub(crate) fn take_output(&mut self) -> Vec<RefPic> {
        std::mem::take(&mut self.output)
    }

    fn free_slot(&self) -> Result<usize, String> {
        (0..self.slots)
            .find(|slot| !self.entries.iter().any(|e| e.pic.slot == *slot) && !self.output.iter().any(|o| o.slot == *slot))
            .ok_or_else(|| "H.264: nessun buffer libero nella scheda".to_string())
    }

    /// Legge un campione (formato Annex B). `None` se non contiene fette da decodificare.
    pub(crate) fn begin(&mut self, annex: &[u8]) -> Result<Option<Picture>, String> {
        let mut jobs: Vec<(SliceHeader, Vec<u8>)> = Vec::new();
        for nal in crate::nal::annex_nals(annex) {
            if nal.is_empty() {
                continue;
            }
            match nal[0] & 0x1f {
                7 | 8 => self.add_parameter_set(nal)?,
                1 | 5 => {
                    let header = parse_slice(nal, &self.spss, &self.ppss)?;
                    if header.redundant_pic_cnt > 0 {
                        continue;
                    }
                    if let Some((first, _)) = jobs.first() {
                        // Un'altra immagine nello stesso campione: non è il caso dei file MP4 comuni.
                        if header.frame_num != first.frame_num || header.idr != first.idr || header.pps_id != first.pps_id {
                            return Err("H.264: più fotogrammi in un campione".into());
                        }
                    }
                    jobs.push((header, nal.to_vec()));
                }
                2..=4 => return Err("H.264: partizioni dei dati non gestite dalla scheda".into()),
                _ => {}
            }
        }
        let Some((first, _)) = jobs.first().cloned() else {
            return Ok(None);
        };
        let pps = self.ppss.get(&first.pps_id).ok_or("H.264: PPS mancante")?.clone();
        let sps = self.spss.get(&pps.sps_id).ok_or("H.264: SPS mancante")?.clone();
        match &self.active {
            None => self.set_active(&sps),
            Some(active) if active.mb_width != sps.mb_width || active.mb_height != sps.mb_height || active.crop != sps.crop => {
                return Err("H.264: il formato cambia nel video".into());
            }
            Some(_) => {}
        }
        if first.idr {
            // Prima di ogni IDR escono tutti i fotogrammi in attesa: sono di prima del taglio.
            self.flush_all();
            self.entries.clear();
            self.seen_idr = true;
            self.max_long_idx = -1;
        } else if !self.seen_idr {
            return Err("H.264: il video non comincia con un fotogramma IDR".into());
        } else {
            let max = sps.max_frame_num() as u32;
            let prev = self.prev_ref_frame_num;
            if first.frame_num != prev && first.frame_num != (prev + 1) % max {
                if sps.gaps_allowed {
                    return Err("H.264: buchi nei numeri dei fotogrammi non gestiti dalla scheda".into());
                }
                return Err("H.264: fotogramma perso".into());
            }
        }
        let (poc, poc_msb, frame_num_offset) = self.picture_order(&sps, &first);
        let slot = self.free_slot()?;
        let refs: Vec<RefPic> = self.entries.iter().filter(|e| e.is_ref()).map(|e| e.pic).collect();
        let current = RefPic { slot, frame_num: first.frame_num, long_idx: None, poc };
        let mut slices = Vec::new();
        for (header, nal) in jobs {
            let (list0, list1) = self.ref_lists(&sps, &header, &current)?;
            slices.push(SliceJob { nal, header, list0, list1 });
        }
        Ok(Some(Picture {
            sps,
            pps,
            slot,
            frame_num: first.frame_num,
            poc,
            is_ref: first.nal_ref_idc != 0,
            idr: first.idr,
            refs,
            slices,
            poc_msb,
            frame_num_offset,
        }))
    }

    /// Il numero d'ordine di visualizzazione (8.2.1).
    fn picture_order(&self, sps: &Sps, h: &SliceHeader) -> ([i32; 2], i32, i64) {
        let max_frame = sps.max_frame_num();
        let frame_num_offset = if h.idr {
            0
        } else if i64::from(self.prev_frame_num) > i64::from(h.frame_num) {
            self.prev_frame_num_offset + max_frame
        } else {
            self.prev_frame_num_offset
        };
        match sps.poc_type {
            0 => {
                let (prev_msb, prev_lsb) = if h.idr { (0, 0) } else { (self.prev_poc_msb, self.prev_poc_lsb) };
                let max_lsb = 1i32 << sps.log2_max_poc_lsb;
                let lsb = h.poc_lsb as i32;
                let msb = if lsb < prev_lsb && prev_lsb - lsb >= max_lsb / 2 {
                    prev_msb + max_lsb
                } else if lsb > prev_lsb && lsb - prev_lsb > max_lsb / 2 {
                    prev_msb - max_lsb
                } else {
                    prev_msb
                };
                let top = msb + lsb;
                ([top, top + h.delta_poc_bottom], msb, frame_num_offset)
            }
            1 => {
                let cycle = sps.offsets_for_ref_frame.len() as i64;
                let mut abs = if cycle != 0 { frame_num_offset + i64::from(h.frame_num) } else { 0 };
                if h.nal_ref_idc == 0 && abs > 0 {
                    abs -= 1;
                }
                let mut expected = 0i64;
                if cycle != 0 && abs > 0 {
                    let cycles = (abs - 1) / cycle;
                    let in_cycle = ((abs - 1) % cycle) as usize;
                    let per_cycle: i64 = sps.offsets_for_ref_frame.iter().map(|&o| i64::from(o)).sum();
                    expected = cycles * per_cycle + sps.offsets_for_ref_frame[..=in_cycle].iter().map(|&o| i64::from(o)).sum::<i64>();
                }
                if h.nal_ref_idc == 0 {
                    expected += i64::from(sps.offset_for_non_ref_pic);
                }
                let top = expected as i32 + h.delta_poc[0];
                let bottom = top + sps.offset_for_top_to_bottom + h.delta_poc[1];
                ([top, bottom], 0, frame_num_offset)
            }
            _ => {
                let temp = if h.idr {
                    0
                } else if h.nal_ref_idc == 0 {
                    2 * (frame_num_offset + i64::from(h.frame_num)) as i32 - 1
                } else {
                    2 * (frame_num_offset + i64::from(h.frame_num)) as i32
                };
                ([temp, temp], 0, frame_num_offset)
            }
        }
    }

    fn frame_num_wrap(&self, entry: &Entry, current: u32, max_frame: i64) -> i64 {
        let n = i64::from(entry.pic.frame_num);
        if n > i64::from(current) {
            n - max_frame
        } else {
            n
        }
    }

    /// Le liste di riferimento di una fetta (8.2.4), come le vuole la scheda: con vuoti dove manca un fotogramma.
    #[allow(clippy::type_complexity)]
    fn ref_lists(&self, sps: &Sps, h: &SliceHeader, current: &RefPic) -> Result<(Vec<Option<RefPic>>, Vec<Option<RefPic>>), String> {
        if h.slice_type == SliceType::I {
            return Ok((Vec::new(), Vec::new()));
        }
        let max_frame = sps.max_frame_num();
        let cur_poc = current.poc[0].min(current.poc[1]);
        let mut short: Vec<(&Entry, i64)> = self
            .entries
            .iter()
            .filter(|e| e.short_ref && e.pic.long_idx.is_none())
            .map(|e| (e, self.frame_num_wrap(e, h.frame_num, max_frame)))
            .collect();
        let mut long: Vec<&Entry> = self.entries.iter().filter(|e| e.pic.long_idx.is_some()).collect();
        long.sort_by_key(|e| e.pic.long_idx);
        let mut lists: [Vec<RefPic>; 2] = [Vec::new(), Vec::new()];
        if h.slice_type == SliceType::P {
            short.sort_by_key(|&(_, wrap)| std::cmp::Reverse(wrap));
            lists[0].extend(short.iter().map(|(e, _)| e.pic));
            lists[0].extend(long.iter().map(|e| e.pic));
        } else {
            let mut before: Vec<&Entry> = short.iter().map(|(e, _)| *e).filter(|e| e.poc() < cur_poc).collect();
            let mut after: Vec<&Entry> = short.iter().map(|(e, _)| *e).filter(|e| e.poc() >= cur_poc).collect();
            before.sort_by_key(|e| std::cmp::Reverse(e.poc()));
            after.sort_by_key(|e| e.poc());
            lists[0].extend(before.iter().chain(after.iter()).map(|e| e.pic));
            lists[0].extend(long.iter().map(|e| e.pic));
            lists[1].extend(after.iter().chain(before.iter()).map(|e| e.pic));
            lists[1].extend(long.iter().map(|e| e.pic));
            if lists[1].len() > 1 && lists[0] == lists[1] {
                lists[1].swap(0, 1);
            }
        }
        let lists_used = if h.slice_type == SliceType::B { 2 } else { 1 };
        let mut out: [Vec<Option<RefPic>>; 2] = [Vec::new(), Vec::new()];
        for list in 0..lists_used {
            let active = h.num_ref_idx_active[list] as usize;
            let mut work: Vec<Option<RefPic>> = lists[list].iter().copied().map(Some).collect();
            work.truncate(active);
            work.resize(active + 1, None);
            let mut index = 0usize;
            let mut pred = i64::from(h.frame_num);
            for &(idc, value) in &h.reorder[list] {
                if index >= active {
                    break;
                }
                let found = if idc == 2 {
                    long.iter().find(|e| e.pic.long_idx == Some(value)).map(|e| e.pic)
                } else {
                    let diff = i64::from(value) + 1;
                    let mut no_wrap = if idc == 0 { pred - diff } else { pred + diff };
                    if idc == 0 && no_wrap < 0 {
                        no_wrap += max_frame;
                    } else if idc == 1 && no_wrap >= max_frame {
                        no_wrap -= max_frame;
                    }
                    pred = no_wrap;
                    let pic_num = if no_wrap > i64::from(h.frame_num) { no_wrap - max_frame } else { no_wrap };
                    short.iter().find(|&&(_, wrap)| wrap == pic_num).map(|(e, _)| e.pic)
                };
                let Some(pic) = found else {
                    return Err("H.264: riordino su un fotogramma che non c'è".into());
                };
                for c in ((index + 1)..=active).rev() {
                    work[c] = work[c - 1];
                }
                work[index] = Some(pic);
                index += 1;
                let mut n = index;
                for c in index..=active {
                    if work[c] != Some(pic) {
                        work[n] = work[c];
                        n += 1;
                    }
                }
            }
            work.truncate(active);
            out[list] = work;
        }
        let [l0, l1] = out;
        Ok((l0, l1))
    }

    /// A fotogramma decodificato: segna i riferimenti, mette il fotogramma tra quelli da mostrare e fa uscire
    /// quelli che si possono mostrare.
    pub(crate) fn finish(&mut self, pic: &Picture) -> Result<(), String> {
        let first = &pic.slices[0].header;
        let max_frame = pic.sps.max_frame_num();
        let mut current = RefPic { slot: pic.slot, frame_num: pic.frame_num, long_idx: None, poc: pic.poc };
        let mut cleared = false;
        let mut current_short = pic.is_ref;
        if pic.is_ref {
            if pic.idr {
                if first.long_term_reference {
                    current.long_idx = Some(0);
                    current_short = false;
                    self.max_long_idx = 0;
                }
            } else if first.adaptive_marking {
                for op in &first.mmco {
                    match *op {
                        Mmco::ShortUnused(diff) => {
                            let pic_num = i64::from(pic.frame_num) - i64::from(diff) - 1;
                            let at = self.entries.iter().position(|e| {
                                e.short_ref && e.pic.long_idx.is_none() && self.frame_num_wrap(e, pic.frame_num, max_frame) == pic_num
                            });
                            if let Some(at) = at {
                                self.entries[at].short_ref = false;
                            }
                        }
                        Mmco::LongUnused(num) => {
                            if let Some(e) = self.entries.iter_mut().find(|e| e.pic.long_idx == Some(num)) {
                                e.pic.long_idx = None;
                            }
                        }
                        Mmco::ShortToLong(diff, idx) => {
                            let pic_num = i64::from(pic.frame_num) - i64::from(diff) - 1;
                            for e in self.entries.iter_mut().filter(|e| e.pic.long_idx == Some(idx)) {
                                e.pic.long_idx = None;
                            }
                            let at = self.entries.iter().position(|e| {
                                e.short_ref && e.pic.long_idx.is_none() && self.frame_num_wrap(e, pic.frame_num, max_frame) == pic_num
                            });
                            if let Some(at) = at {
                                self.entries[at].short_ref = false;
                                self.entries[at].pic.long_idx = Some(idx);
                            }
                        }
                        Mmco::MaxLong(plus1) => {
                            self.max_long_idx = i64::from(plus1) - 1;
                            let max = self.max_long_idx;
                            for e in self.entries.iter_mut() {
                                if e.pic.long_idx.is_some_and(|i| i64::from(i) > max) {
                                    e.pic.long_idx = None;
                                }
                            }
                        }
                        Mmco::ClearAll => {
                            self.flush_waiting_for_clear();
                            for e in self.entries.iter_mut() {
                                e.short_ref = false;
                                e.pic.long_idx = None;
                            }
                            self.max_long_idx = -1;
                            cleared = true;
                        }
                        Mmco::CurrentLong(idx) => {
                            for e in self.entries.iter_mut().filter(|e| e.pic.long_idx == Some(idx)) {
                                e.pic.long_idx = None;
                            }
                            current.long_idx = Some(idx);
                            current_short = false;
                        }
                    }
                }
            } else {
                let limit = pic.sps.max_num_ref_frames.max(1) as usize;
                let held = self.entries.iter().filter(|e| e.is_ref()).count();
                if held >= limit {
                    let oldest = self
                        .entries
                        .iter()
                        .enumerate()
                        .filter(|(_, e)| e.short_ref && e.pic.long_idx.is_none())
                        .min_by_key(|(_, e)| self.frame_num_wrap(e, pic.frame_num, max_frame))
                        .map(|(i, _)| i);
                    if let Some(i) = oldest {
                        self.entries[i].short_ref = false;
                    }
                }
            }
        }
        if cleared {
            // MMCO 5: il fotogramma riparte da zero, come dopo un IDR.
            let low = current.poc[0].min(current.poc[1]);
            current.poc = [current.poc[0] - low, current.poc[1] - low];
            current.frame_num = 0;
        }
        self.entries.retain(|e| e.is_ref() || e.waiting);
        self.entries.push(Entry { pic: current, short_ref: current_short, waiting: true });
        // Aggiorna la memoria dell'ordine dei numeri.
        if pic.is_ref {
            self.prev_ref_frame_num = if cleared { 0 } else { pic.frame_num };
            if cleared {
                self.prev_poc_msb = 0;
                self.prev_poc_lsb = current.poc[0];
            } else {
                self.prev_poc_msb = pic.poc_msb;
                self.prev_poc_lsb = first.poc_lsb as i32;
            }
        }
        self.prev_frame_num = if cleared { 0 } else { pic.frame_num };
        self.prev_frame_num_offset = if cleared { 0 } else { pic.frame_num_offset };
        while self.entries.iter().filter(|e| e.waiting).count() > self.reorder_limit {
            self.bump();
        }
        Ok(())
    }

    /// Fa uscire il fotogramma col numero d'ordine più basso.
    fn bump(&mut self) {
        let Some(at) = self.entries.iter().enumerate().filter(|(_, e)| e.waiting).min_by_key(|(_, e)| e.poc()).map(|(i, _)| i) else {
            return;
        };
        self.entries[at].waiting = false;
        self.output.push(self.entries[at].pic);
        if !self.entries[at].is_ref() {
            self.entries.remove(at);
        }
    }

    /// Prima di un MMCO 5 escono tutti i fotogrammi in attesa (tranne quello che si sta decodificando).
    fn flush_waiting_for_clear(&mut self) {
        while self.entries.iter().any(|e| e.waiting) {
            self.bump();
        }
    }

    fn flush_all(&mut self) {
        while self.entries.iter().any(|e| e.waiting) {
            self.bump();
        }
    }

    /// Fine del video: escono tutti i fotogrammi rimasti.
    pub(crate) fn flush(&mut self) -> Vec<RefPic> {
        self.flush_all();
        self.take_output()
    }
}

#[cfg(test)]
/// Divide un flusso Annex B in campioni: una fetta con first_mb = 0 comincia un fotogramma, e i NAL di
/// parametri che la precedono vanno con lei.
pub(crate) fn split_samples(stream: &[u8]) -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = Vec::new();
    let mut current: Vec<u8> = Vec::new();
    let mut has_slice = false;
    let mut pending: Vec<u8> = Vec::new();
    for nal in crate::nal::annex_nals(stream) {
        let kind = nal[0] & 0x1f;
        let is_slice = kind == 1 || kind == 5;
        let first = is_slice && Bits::new(&unescape(&nal[1..]).0).ue().unwrap() == 0;
        let mut bytes = vec![0, 0, 1];
        bytes.extend_from_slice(nal);
        if first {
            if has_slice {
                out.push(std::mem::take(&mut current));
            }
            current = std::mem::take(&mut pending);
            current.extend_from_slice(&bytes);
            has_slice = true;
        } else if is_slice {
            current.extend_from_slice(&bytes);
        } else if has_slice {
            pending.extend_from_slice(&bytes);
        } else {
            pending.extend_from_slice(&bytes);
        }
    }
    if has_slice {
        out.push(current);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const BFRAMES: &[u8] = include_bytes!("../tests/data/bframes-96x64.h264");
    const BASELINE: &[u8] = include_bytes!("../tests/data/baseline-64x48.h264");

    /// Ordine di uscita dei fotogrammi del video con i B (indice di codifica), come lo dà ffmpeg.
    const BFRAMES_ORDER: [usize; 48] = [
        0, 3, 2, 4, 1, 6, 7, 5, 9, 10, 8, 13, 12, 14, 11, 15, 16, 19, 18, 20, 17, 22, 23, 21, 26, 25, 27, 24, 30, 29, 31, 28, 32, 34, 33, 36, 37,
        35, 38, 41, 40, 42, 39, 43, 44, 45, 46, 47,
    ];

    /// L'ordine in cui escono i fotogrammi, come indice di decodifica.
    fn run(stream: &[u8]) -> Vec<usize> {
        let mut parser = Stream::new();
        let mut shown = Vec::new();
        let mut decoded_as = std::collections::HashMap::new();
        let mut index = 0usize;
        for sample in split_samples(stream) {
            let Some(picture) = parser.begin(&sample).unwrap() else { continue };
            // I buffer in uscita si leggono prima di decodificare: qui si registra a chi appartenevano.
            for out in parser.take_output() {
                shown.push(decoded_as[&out.slot]);
            }
            decoded_as.insert(picture.slot, index);
            parser.finish(&picture).unwrap();
            for out in parser.take_output() {
                shown.push(decoded_as[&out.slot]);
            }
            index += 1;
        }
        for out in parser.flush() {
            shown.push(decoded_as[&out.slot]);
        }
        shown
    }

    #[test]
    fn parametri_del_video_con_i_b() {
        let mut stream = Stream::new();
        for nal in crate::nal::annex_nals(BFRAMES) {
            if matches!(nal[0] & 0x1f, 7 | 8) {
                stream.add_parameter_set(nal).unwrap();
            }
        }
        let sps = stream.activate_first().unwrap();
        assert_eq!((sps.width(), sps.height()), (96, 64));
        assert_eq!(sps.profile_idc, 100);
        assert!(sps.max_num_ref_frames >= 3);
        assert!(sps.num_reorder_frames.is_some());
    }

    #[test]
    fn i_b_escono_nell_ordine_giusto() {
        assert_eq!(run(BFRAMES), BFRAMES_ORDER);
    }

    #[test]
    fn base_senza_b_esce_in_ordine() {
        assert_eq!(run(BASELINE), (0..24).collect::<Vec<_>>());
    }

    #[test]
    fn bit_e_protezione() {
        let (clean, removed) = unescape(&[1, 0, 0, 3, 1, 0, 0, 3, 0]);
        assert_eq!(clean, vec![1, 0, 0, 1, 0, 0, 0]);
        assert_eq!(removed, vec![3, 6]);
        let mut r = Bits::new(&[0b1010_0110, 0b0100_0000]);
        assert_eq!(r.ue().unwrap(), 0);
        assert_eq!(r.ue().unwrap(), 1);
        assert_eq!(r.ue().unwrap(), 2);
    }
}
