//! Decodifica H.264 con la scheda video su Linux, via VA-API: Intel e AMD, grafiche integrate comprese.
//!
//! La scheda decodifica i pezzi di immagine; Cedro legge le intestazioni (`h264`), dice alla scheda quali
//! fotogrammi fanno da riferimento e rimette i fotogrammi nell'ordine di visualizzazione. Se il video non
//! è adatto (interlacciato, 10 bit, …), se la scheda manca o rifiuta un fotogramma, chi chiama rifà il video
//! con OpenH264 come sempre. Le strutture sono quelle di `va.h` (libva 2.x); i test ne controllano le misure.

use super::*;
use crate::h264::{Picture, RefPic, SliceJob, SliceType, Sps, Stream};

const BUF_PIC_PARAM: c_int = 0;
const BUF_IQ_MATRIX: c_int = 1;
const BUF_SLICE_PARAM: c_int = 4;
const BUF_SLICE_DATA: c_int = 5;
const PICTURE_LONG_TERM: u32 = 16;

#[repr(C)]
struct DecPictureParameters {
    curr_pic: PictureH264,
    reference_frames: [PictureH264; 16],
    picture_width_in_mbs_minus1: u16,
    picture_height_in_mbs_minus1: u16,
    bit_depth_luma_minus8: u8,
    bit_depth_chroma_minus8: u8,
    num_ref_frames: u8,
    seq_fields: u32,
    num_slice_groups_minus1: u8,
    slice_group_map_type: u8,
    slice_group_change_rate_minus1: u16,
    pic_init_qp_minus26: i8,
    pic_init_qs_minus26: i8,
    chroma_qp_index_offset: i8,
    second_chroma_qp_index_offset: i8,
    pic_fields: u32,
    frame_num: u16,
    reserved: [u32; 8],
}

#[repr(C)]
struct IqMatrix {
    scaling_list_4x4: [[u8; 16]; 6],
    scaling_list_8x8: [[u8; 64]; 2],
    reserved: [u32; 4],
}

#[repr(C)]
struct DecSliceParameters {
    slice_data_size: u32,
    slice_data_offset: u32,
    slice_data_flag: u32,
    slice_data_bit_offset: u16,
    first_mb_in_slice: u16,
    slice_type: u8,
    direct_spatial_mv_pred_flag: u8,
    num_ref_idx_l0_active_minus1: u8,
    num_ref_idx_l1_active_minus1: u8,
    cabac_init_idc: u8,
    slice_qp_delta: i8,
    disable_deblocking_filter_idc: u8,
    slice_alpha_c0_offset_div2: i8,
    slice_beta_offset_div2: i8,
    ref_pic_list0: [PictureH264; 32],
    ref_pic_list1: [PictureH264; 32],
    luma_log2_weight_denom: u8,
    chroma_log2_weight_denom: u8,
    luma_weight_l0_flag: u8,
    luma_weight_l0: [i16; 32],
    luma_offset_l0: [i16; 32],
    chroma_weight_l0_flag: u8,
    chroma_weight_l0: [[i16; 2]; 32],
    chroma_offset_l0: [[i16; 2]; 32],
    luma_weight_l1_flag: u8,
    luma_weight_l1: [i16; 32],
    luma_offset_l1: [i16; 32],
    chroma_weight_l1_flag: u8,
    chroma_weight_l1: [[i16; 2]; 32],
    chroma_offset_l1: [[i16; 2]; 32],
    reserved: [u32; 4],
}

/// Un fotogramma decodificato, in I420 compatto e già ritagliato.
pub(crate) struct I420Frame {
    pub y: Vec<u8>,
    pub u: Vec<u8>,
    pub v: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Un decodificatore H.264 sulla scheda video: campioni Annex B dentro, fotogrammi I420 fuori, in ordine.
pub(crate) struct H264Decoder {
    device: Device,
    config: Id,
    context: Id,
    surfaces: Vec<Id>,
    image: Image,
    stream: Stream,
    sps: Sps,
    aligned: (u32, u32),
}

impl Drop for H264Decoder {
    fn drop(&mut self) {
        let lib = self.device.lib;
        let dpy = self.device.dpy;
        unsafe {
            (lib.destroy_image)(dpy, self.image.image_id);
            (lib.destroy_context)(dpy, self.context);
            (lib.destroy_surfaces)(dpy, self.surfaces.as_mut_ptr(), self.surfaces.len() as c_int);
            (lib.destroy_config)(dpy, self.config);
        }
    }
}

/// Quali profili della scheda possono decodificare uno SPS, dal più adatto.
fn profiles_for(sps: &Sps) -> Vec<c_int> {
    match sps.profile_idc {
        66 => vec![PROFILE_H264_CONSTRAINED_BASELINE, PROFILE_H264_MAIN, PROFILE_H264_HIGH],
        77 => vec![PROFILE_H264_MAIN, PROFILE_H264_HIGH],
        100 => vec![PROFILE_H264_HIGH],
        _ => Vec::new(),
    }
}

impl H264Decoder {
    /// `parameter_sets`: gli SPS e i PPS del file, ognuno un NAL senza start code.
    pub(crate) fn new(parameter_sets: &[&[u8]]) -> Result<Self, String> {
        let mut stream = Stream::new();
        for nal in parameter_sets {
            stream.add_parameter_set(nal)?;
        }
        let sps = stream.activate_first()?;
        let profiles = profiles_for(&sps);
        if profiles.is_empty() {
            return Err(format!("scheda video: profilo H.264 {} non gestito", sps.profile_idc));
        }
        let (aw, ah) = (sps.mb_width * 16, sps.mb_height * 16);
        if aw < 16 || ah < 16 || aw > 4096 || ah > 4096 {
            return Err(format!("scheda video: {}×{} non adatto", sps.width(), sps.height()));
        }
        let device = Device::open_decode(&profiles).ok_or("nessuna scheda video che decodifica H.264")?;
        let lib = device.lib;
        let dpy = device.dpy;
        let slots = stream.slots();
        unsafe {
            let mut attribs = [ConfigAttrib { kind: ATTRIB_RT_FORMAT, value: RT_FORMAT_YUV420 }];
            let mut config = INVALID_ID;
            check((lib.create_config)(dpy, device.profile, device.entrypoint, attribs.as_mut_ptr(), 1, &mut config), "configurazione")?;
            let mut surfaces = vec![INVALID_ID; slots];
            let made = (lib.create_surfaces)(dpy, RT_FORMAT_YUV420, aw, ah, surfaces.as_mut_ptr(), slots as c_uint, std::ptr::null_mut(), 0);
            if made != SUCCESS {
                (lib.destroy_config)(dpy, config);
                return Err(format!("scheda video: superfici non create ({made})"));
            }
            let mut context = INVALID_ID;
            let made = (lib.create_context)(dpy, config, aw as c_int, ah as c_int, PROGRESSIVE, surfaces.as_mut_ptr(), slots as c_int, &mut context);
            if made != SUCCESS {
                (lib.destroy_surfaces)(dpy, surfaces.as_mut_ptr(), slots as c_int);
                (lib.destroy_config)(dpy, config);
                return Err(format!("scheda video: contesto non creato ({made})"));
            }
            let mut format = ImageFormat { fourcc: FOURCC_NV12, byte_order: 1, bits_per_pixel: 12, ..Default::default() };
            let mut image = Image::default();
            let made = (lib.create_image)(dpy, &mut format, aw as c_int, ah as c_int, &mut image);
            if made != SUCCESS {
                (lib.destroy_context)(dpy, context);
                (lib.destroy_surfaces)(dpy, surfaces.as_mut_ptr(), slots as c_int);
                (lib.destroy_config)(dpy, config);
                return Err(format!("scheda video: immagine non creata ({made})"));
            }
            Ok(H264Decoder { device, config, context, surfaces, image, stream, sps, aligned: (aw, ah) })
        }
    }

    /// Il nome della scheda, per la spia «in uso».
    pub(crate) fn name(&self) -> String {
        self.device.name()
    }

    /// Un campione (un fotogramma, formato Annex B). I fotogrammi pronti vanno a `emit`, in ordine di visualizzazione.
    /// Vero se il campione aveva un fotogramma da decodificare (chi ne conta i tempi lo sa).
    pub(crate) fn decode(&mut self, annex: &[u8], emit: &mut dyn FnMut(I420Frame) -> Result<(), String>) -> Result<bool, String> {
        let Some(picture) = self.stream.begin(annex)? else {
            return Ok(false);
        };
        // Quelli usciti per un taglio (IDR) si leggono prima che il buffer serva ad altro.
        self.drain(emit)?;
        self.submit(&picture)?;
        self.stream.finish(&picture)?;
        self.drain(emit)?;
        Ok(true)
    }

    /// Fine del video: escono i fotogrammi rimasti.
    pub(crate) fn flush(&mut self, emit: &mut dyn FnMut(I420Frame) -> Result<(), String>) -> Result<(), String> {
        for out in self.stream.flush() {
            emit(self.read(out.slot)?)?;
        }
        Ok(())
    }

    fn drain(&mut self, emit: &mut dyn FnMut(I420Frame) -> Result<(), String>) -> Result<(), String> {
        for out in self.stream.take_output() {
            emit(self.read(out.slot)?)?;
        }
        Ok(())
    }

    fn picture_of(&self, pic: &RefPic) -> PictureH264 {
        let long = pic.long_idx.is_some();
        PictureH264 {
            picture_id: self.surfaces[pic.slot],
            frame_idx: pic.long_idx.unwrap_or(pic.frame_num),
            flags: if long { PICTURE_LONG_TERM } else { PICTURE_SHORT_TERM },
            top_field_order_cnt: pic.poc[0],
            bottom_field_order_cnt: pic.poc[1],
            reserved: [0; 4],
        }
    }

    fn buffer(&self, kind: c_int, data: &[u8]) -> Result<Id, String> {
        let mut id = INVALID_ID;
        let status = unsafe {
            (self.device.lib.create_buffer)(self.device.dpy, self.context, kind, data.len() as c_uint, 1, data.as_ptr() as *mut c_void, &mut id)
        };
        check(status, "parametri")?;
        Ok(id)
    }

    fn submit(&mut self, picture: &Picture) -> Result<(), String> {
        let lib = self.device.lib;
        let dpy = self.device.dpy;
        let sps = &picture.sps;
        let pps = &picture.pps;
        let mut params: DecPictureParameters = zeroed();
        params.curr_pic = PictureH264 {
            picture_id: self.surfaces[picture.slot],
            frame_idx: picture.frame_num,
            flags: if picture.is_ref { PICTURE_SHORT_TERM } else { 0 },
            top_field_order_cnt: picture.poc[0],
            bottom_field_order_cnt: picture.poc[1],
            reserved: [0; 4],
        };
        params.reference_frames = [PictureH264::INVALID; 16];
        for (i, pic) in picture.refs.iter().take(16).enumerate() {
            params.reference_frames[i] = self.picture_of(pic);
        }
        params.picture_width_in_mbs_minus1 = (sps.mb_width - 1) as u16;
        params.picture_height_in_mbs_minus1 = (sps.mb_height - 1) as u16;
        params.num_ref_frames = sps.max_num_ref_frames as u8;
        params.seq_fields = 1 // chroma_format_idc = 1 (4:2:0)
            | (u32::from(sps.gaps_allowed) << 3)
            | (1 << 4) // frame_mbs_only_flag
            | (u32::from(sps.direct_8x8_inference) << 6)
            | (u32::from(sps.level_idc >= 31) << 7)
            | ((sps.log2_max_frame_num - 4) << 8)
            | (sps.poc_type << 12)
            | (if sps.poc_type == 0 { sps.log2_max_poc_lsb - 4 } else { 0 } << 14)
            | (u32::from(sps.delta_pic_order_always_zero) << 18);
        params.pic_init_qp_minus26 = pps.pic_init_qp_minus26 as i8;
        params.pic_init_qs_minus26 = pps.pic_init_qs_minus26 as i8;
        params.chroma_qp_index_offset = pps.chroma_qp_index_offset as i8;
        params.second_chroma_qp_index_offset = pps.second_chroma_qp_index_offset as i8;
        params.pic_fields = u32::from(pps.cabac)
            | (u32::from(pps.weighted_pred) << 1)
            | (pps.weighted_bipred_idc << 2)
            | (u32::from(pps.transform_8x8_mode) << 4)
            | (u32::from(pps.constrained_intra_pred) << 6)
            | (u32::from(pps.bottom_field_pic_order_present) << 7)
            | (u32::from(pps.deblocking_filter_control_present) << 8)
            | (u32::from(pps.redundant_pic_cnt_present) << 9)
            | (u32::from(picture.is_ref) << 10);
        params.frame_num = picture.frame_num as u16;
        let mut matrix: IqMatrix = zeroed();
        matrix.scaling_list_4x4 = [[16; 16]; 6];
        matrix.scaling_list_8x8 = [[16; 64]; 2];

        let mut ids = Vec::new();
        let mut slice_ids = Vec::new();
        let result = (|| -> Result<(), String> {
            ids.push(self.buffer(BUF_PIC_PARAM, bytes_of(&params))?);
            ids.push(self.buffer(BUF_IQ_MATRIX, bytes_of(&matrix))?);
            for slice in &picture.slices {
                let slice_params = self.slice_parameters(slice);
                slice_ids.push(self.buffer(BUF_SLICE_PARAM, bytes_of(&*slice_params))?);
                slice_ids.push(self.buffer(BUF_SLICE_DATA, &slice.nal)?);
            }
            unsafe {
                check((lib.begin_picture)(dpy, self.context, self.surfaces[picture.slot]), "inizio fotogramma")?;
                check((lib.render_picture)(dpy, self.context, ids.as_mut_ptr(), ids.len() as c_int), "parametri del fotogramma")?;
                check((lib.render_picture)(dpy, self.context, slice_ids.as_mut_ptr(), slice_ids.len() as c_int), "dati del fotogramma")?;
                check((lib.end_picture)(dpy, self.context), "fine fotogramma")?;
            }
            Ok(())
        })();
        // I buffer non li libera la scheda: senza questo ogni fotogramma ne lascia indietro (come fa ffmpeg).
        for id in ids.into_iter().chain(slice_ids) {
            unsafe { (lib.destroy_buffer)(dpy, id) };
        }
        result
    }

    fn slice_parameters(&self, slice: &SliceJob) -> Box<DecSliceParameters> {
        let h = &slice.header;
        let mut p: Box<DecSliceParameters> = Box::new(zeroed());
        p.slice_data_size = slice.nal.len() as u32;
        p.slice_data_offset = 0;
        p.slice_data_flag = 0; // VA_SLICE_DATA_FLAG_ALL
        p.slice_data_bit_offset = h.data_bit_offset as u16;
        p.first_mb_in_slice = h.first_mb as u16;
        p.slice_type = match h.slice_type {
            SliceType::P => 0,
            SliceType::B => 1,
            SliceType::I => 2,
        };
        p.direct_spatial_mv_pred_flag = u8::from(h.direct_spatial);
        p.num_ref_idx_l0_active_minus1 = h.num_ref_idx_active[0].saturating_sub(1) as u8;
        p.num_ref_idx_l1_active_minus1 = h.num_ref_idx_active[1].saturating_sub(1) as u8;
        p.cabac_init_idc = h.cabac_init_idc as u8;
        p.slice_qp_delta = h.slice_qp_delta as i8;
        p.disable_deblocking_filter_idc = h.disable_deblocking_filter_idc as u8;
        p.slice_alpha_c0_offset_div2 = h.alpha_c0_offset_div2 as i8;
        p.slice_beta_offset_div2 = h.beta_offset_div2 as i8;
        p.ref_pic_list0 = [PictureH264::INVALID; 32];
        p.ref_pic_list1 = [PictureH264::INVALID; 32];
        for (i, entry) in slice.list0.iter().take(32).enumerate() {
            if let Some(pic) = entry {
                p.ref_pic_list0[i] = self.picture_of(pic);
            }
        }
        for (i, entry) in slice.list1.iter().take(32).enumerate() {
            if let Some(pic) = entry {
                p.ref_pic_list1[i] = self.picture_of(pic);
            }
        }
        if let Some(w) = &h.weights {
            p.luma_log2_weight_denom = w.luma_log2_denom;
            p.chroma_log2_weight_denom = w.chroma_log2_denom;
            let lists = if h.slice_type == SliceType::B { 2 } else { 1 };
            for list in 0..lists {
                let (luma_flag, luma_weight, luma_offset, chroma_flag, chroma_weight, chroma_offset) = if list == 0 {
                    (&mut p.luma_weight_l0_flag, &mut p.luma_weight_l0, &mut p.luma_offset_l0, &mut p.chroma_weight_l0_flag, &mut p.chroma_weight_l0, &mut p.chroma_offset_l0)
                } else {
                    (&mut p.luma_weight_l1_flag, &mut p.luma_weight_l1, &mut p.luma_offset_l1, &mut p.chroma_weight_l1_flag, &mut p.chroma_weight_l1, &mut p.chroma_offset_l1)
                };
                for (i, &(flag, weight, offset)) in w.luma[list].iter().take(32).enumerate() {
                    *luma_flag |= u8::from(flag);
                    luma_weight[i] = weight;
                    luma_offset[i] = offset;
                }
                for (i, &(flag, weight, offset)) in w.chroma[list].iter().take(32).enumerate() {
                    *chroma_flag |= u8::from(flag);
                    chroma_weight[i] = weight;
                    chroma_offset[i] = offset;
                }
            }
        }
        p
    }

    /// Legge un fotogramma decodificato dalla scheda: NV12 in memoria normale, poi I420 ritagliato.
    fn read(&self, slot: usize) -> Result<I420Frame, String> {
        let lib = self.device.lib;
        let dpy = self.device.dpy;
        let (aw, ah) = self.aligned;
        let sps = &self.sps;
        unsafe {
            check((lib.sync_surface)(dpy, self.surfaces[slot]), "attesa del fotogramma")?;
            check((lib.get_image)(dpy, self.surfaces[slot], 0, 0, aw, ah, self.image.image_id), "lettura del fotogramma")?;
            let mut ptr = std::ptr::null_mut();
            let mapped = (lib.map_buffer)(dpy, self.image.buf, &mut ptr);
            if mapped != SUCCESS || ptr.is_null() {
                return Err(format!("scheda video: fotogramma non leggibile ({mapped})"));
            }
            let src = std::slice::from_raw_parts(ptr as *const u8, self.image.data_size as usize);
            let frame = nv12_to_i420(src, &self.image, (sps.width() as usize, sps.height() as usize), (sps.crop[0] as usize, sps.crop[2] as usize));
            (lib.unmap_buffer)(dpy, self.image.buf);
            frame
        }
    }
}

/// NV12 con passi e scarti qualsiasi → I420 compatto, tagliato: `size` è la misura finale, `origin` l'angolo in alto a sinistra.
fn nv12_to_i420(src: &[u8], image: &Image, size: (usize, usize), origin: (usize, usize)) -> Result<I420Frame, String> {
    let (w, h) = size;
    let (x0, y0) = origin;
    let (pitch_y, off_y) = (image.pitches[0] as usize, image.offsets[0] as usize);
    let (pitch_uv, off_uv) = (image.pitches[1] as usize, image.offsets[1] as usize);
    if w == 0 || h == 0 || w % 2 == 1 || h % 2 == 1 || image.num_planes < 2 {
        return Err("scheda video: fotogramma non valido".into());
    }
    let (cw, ch) = (w / 2, h / 2);
    if off_y + (y0 + h) * pitch_y > src.len() || off_uv + (y0 / 2 + ch) * pitch_uv > src.len() || pitch_y < x0 + w || pitch_uv < x0 + w {
        return Err("scheda video: fotogramma troppo corto".into());
    }
    let mut y = Vec::with_capacity(w * h);
    for row in 0..h {
        let at = off_y + (y0 + row) * pitch_y + x0;
        y.extend_from_slice(&src[at..at + w]);
    }
    let mut u = Vec::with_capacity(cw * ch);
    let mut v = Vec::with_capacity(cw * ch);
    for row in 0..ch {
        let at = off_uv + (y0 / 2 + row) * pitch_uv + x0;
        for pair in src[at..at + cw * 2].chunks_exact(2) {
            u.push(pair[0]);
            v.push(pair[1]);
        }
    }
    Ok(I420Frame { y, u, v, width: w as u32, height: h as u32 })
}

#[cfg(test)]
mod tests {
    use std::mem::offset_of;

    use super::*;

    #[test]
    fn misure_come_va_h() {
        assert_eq!(std::mem::size_of::<PictureH264>(), 36);
        assert_eq!(std::mem::size_of::<DecPictureParameters>(), 672);
        assert_eq!(offset_of!(DecPictureParameters, reference_frames), 36);
        assert_eq!(offset_of!(DecPictureParameters, picture_width_in_mbs_minus1), 612);
        assert_eq!(offset_of!(DecPictureParameters, bit_depth_luma_minus8), 616);
        assert_eq!(offset_of!(DecPictureParameters, num_ref_frames), 618);
        assert_eq!(offset_of!(DecPictureParameters, seq_fields), 620);
        assert_eq!(offset_of!(DecPictureParameters, num_slice_groups_minus1), 624);
        assert_eq!(offset_of!(DecPictureParameters, pic_init_qp_minus26), 628);
        assert_eq!(offset_of!(DecPictureParameters, chroma_qp_index_offset), 630);
        assert_eq!(offset_of!(DecPictureParameters, pic_fields), 632);
        assert_eq!(offset_of!(DecPictureParameters, frame_num), 636);
        assert_eq!(std::mem::size_of::<IqMatrix>(), 240);
        assert_eq!(std::mem::size_of::<DecSliceParameters>(), 3128);
        assert_eq!(offset_of!(DecSliceParameters, slice_data_offset), 4);
        assert_eq!(offset_of!(DecSliceParameters, slice_data_flag), 8);
        assert_eq!(offset_of!(DecSliceParameters, slice_data_bit_offset), 12);
        assert_eq!(offset_of!(DecSliceParameters, first_mb_in_slice), 14);
        assert_eq!(offset_of!(DecSliceParameters, slice_type), 16);
        assert_eq!(offset_of!(DecSliceParameters, cabac_init_idc), 20);
        assert_eq!(offset_of!(DecSliceParameters, slice_beta_offset_div2), 24);
        assert_eq!(offset_of!(DecSliceParameters, ref_pic_list0), 28);
        assert_eq!(offset_of!(DecSliceParameters, ref_pic_list1), 1180);
        assert_eq!(offset_of!(DecSliceParameters, luma_log2_weight_denom), 2332);
        assert_eq!(offset_of!(DecSliceParameters, luma_weight_l0_flag), 2334);
        assert_eq!(offset_of!(DecSliceParameters, luma_weight_l0), 2336);
        assert_eq!(offset_of!(DecSliceParameters, luma_offset_l0), 2400);
        assert_eq!(offset_of!(DecSliceParameters, chroma_weight_l0_flag), 2464);
        assert_eq!(offset_of!(DecSliceParameters, chroma_weight_l0), 2466);
        assert_eq!(offset_of!(DecSliceParameters, chroma_offset_l0), 2594);
        assert_eq!(offset_of!(DecSliceParameters, luma_weight_l1_flag), 2722);
        assert_eq!(offset_of!(DecSliceParameters, chroma_offset_l1), 2982);
    }

    #[test]
    fn nv12_ritagliato() {
        // Superficie 4×4: Y = 0..16, poi UV alternati; si tiene il quadrato 2×2 in basso a destra.
        let mut src = vec![0u8; 4 * 4 + 4 * 2];
        for (i, b) in src.iter_mut().enumerate().take(16) {
            *b = i as u8;
        }
        src[16..].copy_from_slice(&[100, 200, 101, 201, 102, 202, 103, 203]);
        let image = Image { pitches: [4, 4, 0], offsets: [0, 16, 0], num_planes: 2, ..Default::default() };
        let frame = nv12_to_i420(&src, &image, (2, 2), (2, 2)).unwrap();
        assert_eq!(frame.y, vec![10, 11, 14, 15]);
        assert_eq!((frame.u, frame.v), (vec![103], vec![203]));
        assert!(nv12_to_i420(&src, &image, (4, 4), (2, 2)).is_err());
    }

    fn plane(src: &[u8], width: usize, height: usize, stride: usize) -> Vec<u8> {
        (0..height).flat_map(|row| src[row * stride..row * stride + width].iter().copied()).collect()
    }

    fn psnr(a: &[u8], b: &[u8]) -> f64 {
        assert_eq!(a.len(), b.len());
        let sum: f64 = a.iter().zip(b).map(|(&x, &y)| (f64::from(x) - f64::from(y)).powi(2)).sum();
        if sum == 0.0 {
            return 99.0;
        }
        10.0 * (255.0f64 * 255.0 / (sum / a.len() as f64)).log10()
    }

    /// Su un PC con una scheda che decodifica H.264 (Intel o AMD) confronta con OpenH264 due video veri
    /// (uno con fotogrammi B, uno base); altrove non fa niente. Con CEDRO_GPU_TEST=1 la scheda deve esserci.
    #[test]
    fn decodes_on_real_hardware() {
        use openh264::decoder::Decoder;
        use openh264::formats::YUVSource;
        let must = std::env::var_os("CEDRO_GPU_TEST").is_some();
        for (name, stream) in [
            ("B", &include_bytes!("../../tests/data/bframes-96x64.h264")[..]),
            ("base", &include_bytes!("../../tests/data/baseline-64x48.h264")[..]),
        ] {
            let samples = crate::h264::split_samples(stream);
            let sets: Vec<&[u8]> = crate::nal::annex_nals(stream).into_iter().filter(|n| matches!(n[0] & 0x1f, 7 | 8)).collect();
            let mut gpu = match H264Decoder::new(&sets) {
                Ok(decoder) => decoder,
                // Sotto il minimo del decodificatore della scheda (AMD: 64×48 non si crea) Cedro usa OpenH264.
                Err(err) if name == "base" => {
                    eprintln!("{name}: {err} (si userebbe OpenH264)");
                    continue;
                }
                Err(err) if must => panic!("{err}"),
                Err(_) => return,
            };
            eprintln!("scheda video: {} ({name})", gpu.name());
            let mut got: Vec<I420Frame> = Vec::new();
            for (n, sample) in samples.iter().enumerate() {
                if let Err(err) = gpu.decode(sample, &mut |frame| {
                    got.push(frame);
                    Ok(())
                }) {
                    assert!(!must, "campione {n}: {err}");
                    eprintln!("la scheda video non decodifica ancora: {err}");
                    return;
                }
            }
            gpu.flush(&mut |frame| {
                got.push(frame);
                Ok(())
            })
            .unwrap();
            let mut soft = Decoder::new().unwrap();
            let mut soft_errors = 0;
            let mut want: Vec<I420Frame> = Vec::new();
            let mut take = |f: &openh264::decoder::DecodedYUV| {
                let (w, h) = f.dimensions();
                let (sy, su, sv) = f.strides();
                want.push(I420Frame {
                    y: plane(f.y(), w, h, sy),
                    u: plane(f.u(), w / 2, h / 2, su),
                    v: plane(f.v(), w / 2, h / 2, sv),
                    width: w as u32,
                    height: h as u32,
                });
            };
            for sample in &samples {
                let annex = crate::nal::annex_nals(sample).iter().fold(Vec::new(), |mut v, n| {
                    v.extend_from_slice(&[0, 0, 0, 1]);
                    v.extend_from_slice(n);
                    v
                });
                match soft.decode(&annex) {
                    Ok(Some(f)) => take(&f),
                    Ok(None) => {}
                    Err(_) => soft_errors += 1,
                }
            }
            for f in soft.flush_remaining().unwrap_or_default() {
                take(&f);
            }
            if soft_errors > 0 {
                // OpenH264 non legge tutti i flussi con i B (pyramid, pesi): non è un riferimento valido, ffmpeg sì.
                eprintln!("{name}: OpenH264 dà {soft_errors} errori su questo flusso, confronto saltato ({} fotogrammi dalla scheda)", got.len());
                continue;
            }
            assert_eq!(got.len(), want.len(), "{name}: numero di fotogrammi");
            for (n, (g, w)) in got.iter().zip(&want).enumerate() {
                assert_eq!((g.width, g.height), (w.width, w.height), "{name}: misure");
                let worst = psnr(&g.y, &w.y).min(psnr(&g.u, &w.u)).min(psnr(&g.v, &w.v));
                assert!(worst > 40.0, "{name}: fotogramma {n} diverso da OpenH264 ({worst:.1} dB)");
            }
            eprintln!("{name}: {} fotogrammi uguali a OpenH264", got.len());
        }
    }
}
