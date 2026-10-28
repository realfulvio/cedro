//! H.264 con la scheda video su Linux, via VA-API: Intel (Quick Sync) e AMD, grafiche integrate comprese.
//!
//! libva si apre al momento con dlopen, così Cedro parte anche dove manca. Se la libreria non c'è, se la
//! scheda non codifica H.264 o se rifiuta un fotogramma, il video si fa con OpenH264 come sempre.
//! AMD (radeonsi) non scrive le intestazioni da sé: dove il driver le accetta, SPS, PPS e intestazione
//! della fetta li scrive Cedro in bit (`BitWriter`), come fa ffmpeg con `h264_vaapi`.
//! Le strutture sono quelle di `va.h` e `va_enc_h264.h` (libva 2.x); i test ne controllano le misure.

use std::ffi::{c_char, c_int, c_uint, c_void, CStr};
use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::sync::OnceLock;

type Display = *mut c_void;
type Status = c_int;
type Id = c_uint;

const SUCCESS: Status = 0;
const INVALID_ID: Id = 0xffff_ffff;

const PROFILE_H264_MAIN: c_int = 6;
const PROFILE_H264_HIGH: c_int = 7;
const PROFILE_H264_CONSTRAINED_BASELINE: c_int = 13;
const ENTRY_ENC_SLICE: c_int = 6;
const ENTRY_ENC_SLICE_LP: c_int = 8;

const ATTRIB_RT_FORMAT: c_int = 0;
const ATTRIB_RATE_CONTROL: c_int = 5;
const ATTRIB_ENC_PACKED_HEADERS: c_int = 10;
const PACKED_SEQUENCE: u32 = 1;
const PACKED_SLICE: u32 = 4;
const ATTRIB_NOT_SUPPORTED: u32 = 0x8000_0000;
const RT_FORMAT_YUV420: u32 = 1;
const RC_CBR: u32 = 2;
const RC_VBR: u32 = 4;
const RC_CQP: u32 = 16;

const BUF_CODED: c_int = 21;
const BUF_SEQUENCE: c_int = 22;
const BUF_PICTURE: c_int = 23;
const BUF_SLICE: c_int = 24;
const BUF_PACKED_HEADER_PARAM: c_int = 25;
const BUF_PACKED_HEADER_DATA: c_int = 26;
const HEADER_SEQUENCE: u32 = 1;
const HEADER_SLICE: u32 = 3;
const BUF_MISC: c_int = 27;
const MISC_FRAME_RATE: u32 = 0;
const MISC_RATE_CONTROL: u32 = 1;

const PICTURE_INVALID: u32 = 1;
const PICTURE_SHORT_TERM: u32 = 8;
const FOURCC_NV12: u32 = 0x3231_564E;
const PROGRESSIVE: c_int = 1;

#[repr(C)]
#[derive(Clone, Copy)]
struct PictureH264 {
    picture_id: Id,
    frame_idx: u32,
    flags: u32,
    top_field_order_cnt: i32,
    bottom_field_order_cnt: i32,
    reserved: [u32; 4],
}

impl PictureH264 {
    const INVALID: Self = PictureH264 {
        picture_id: INVALID_ID,
        frame_idx: 0,
        flags: PICTURE_INVALID,
        top_field_order_cnt: 0,
        bottom_field_order_cnt: 0,
        reserved: [0; 4],
    };
}

#[repr(C)]
struct SequenceParameters {
    seq_parameter_set_id: u8,
    level_idc: u8,
    intra_period: u32,
    intra_idr_period: u32,
    ip_period: u32,
    bits_per_second: u32,
    max_num_ref_frames: u32,
    picture_width_in_mbs: u16,
    picture_height_in_mbs: u16,
    seq_fields: u32,
    bit_depth_luma_minus8: u8,
    bit_depth_chroma_minus8: u8,
    num_ref_frames_in_pic_order_cnt_cycle: u8,
    offset_for_non_ref_pic: i32,
    offset_for_top_to_bottom_field: i32,
    offset_for_ref_frame: [i32; 256],
    frame_cropping_flag: u8,
    frame_crop_left_offset: u32,
    frame_crop_right_offset: u32,
    frame_crop_top_offset: u32,
    frame_crop_bottom_offset: u32,
    vui_parameters_present_flag: u8,
    vui_fields: u32,
    aspect_ratio_idc: u8,
    sar_width: u32,
    sar_height: u32,
    num_units_in_tick: u32,
    time_scale: u32,
    reserved: [u32; 4],
}

#[repr(C)]
struct PictureParameters {
    curr_pic: PictureH264,
    reference_frames: [PictureH264; 16],
    coded_buf: Id,
    pic_parameter_set_id: u8,
    seq_parameter_set_id: u8,
    last_picture: u8,
    frame_num: u16,
    pic_init_qp: u8,
    num_ref_idx_l0_active_minus1: u8,
    num_ref_idx_l1_active_minus1: u8,
    chroma_qp_index_offset: i8,
    second_chroma_qp_index_offset: i8,
    pic_fields: u32,
    reserved: [u32; 4],
}

#[repr(C)]
struct SliceParameters {
    macroblock_address: u32,
    num_macroblocks: u32,
    macroblock_info: Id,
    slice_type: u8,
    pic_parameter_set_id: u8,
    idr_pic_id: u16,
    pic_order_cnt_lsb: u16,
    delta_pic_order_cnt_bottom: i32,
    delta_pic_order_cnt: [i32; 2],
    direct_spatial_mv_pred_flag: u8,
    num_ref_idx_active_override_flag: u8,
    num_ref_idx_l0_active_minus1: u8,
    num_ref_idx_l1_active_minus1: u8,
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
    cabac_init_idc: u8,
    slice_qp_delta: i8,
    disable_deblocking_filter_idc: u8,
    slice_alpha_c0_offset_div2: i8,
    slice_beta_offset_div2: i8,
    reserved: [u32; 4],
}

#[repr(C)]
struct RateControl {
    bits_per_second: u32,
    target_percentage: u32,
    window_size: u32,
    initial_qp: u32,
    min_qp: u32,
    basic_unit_size: u32,
    rc_flags: u32,
    icq_quality_factor: u32,
    max_qp: u32,
    quality_factor: u32,
    target_frame_size: u32,
    reserved: [u32; 4],
}

/// `VAEncPackedHeaderParameterBuffer`: che intestazione sta per arrivare e quanto è lunga, in bit.
#[repr(C)]
struct PackedHeader {
    kind: u32,
    bit_length: u32,
    has_emulation_bytes: u8,
    reserved: [u32; 4],
}

#[repr(C)]
struct FrameRate {
    framerate: u32,
    framerate_flags: u32,
    reserved: [u32; 4],
}

#[repr(C)]
struct CodedSegment {
    size: u32,
    bit_offset: u32,
    status: u32,
    reserved: u32,
    buf: *mut c_void,
    next: *mut c_void,
    va_reserved: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ImageFormat {
    fourcc: u32,
    byte_order: u32,
    bits_per_pixel: u32,
    depth: u32,
    red_mask: u32,
    green_mask: u32,
    blue_mask: u32,
    alpha_mask: u32,
    reserved: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Image {
    image_id: Id,
    format: ImageFormat,
    buf: Id,
    width: u16,
    height: u16,
    data_size: u32,
    num_planes: u32,
    pitches: [u32; 3],
    offsets: [u32; 3],
    num_palette_entries: i32,
    entry_bytes: i32,
    component_order: [i8; 4],
    reserved: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ConfigAttrib {
    kind: c_int,
    value: u32,
}

/// Una struttura da passare a libva come byte.
fn bytes_of<T>(value: &T) -> &[u8] {
    unsafe { std::slice::from_raw_parts(value as *const T as *const u8, std::mem::size_of::<T>()) }
}

/// Una struttura tutta a zero: per quelle di libva vuol dire «campo non usato».
fn zeroed<T>() -> T {
    unsafe { std::mem::zeroed() }
}

type MessageCallback = extern "C" fn(*mut c_void, *const c_char);

extern "C" fn quiet(_: *mut c_void, _: *const c_char) {}

/// Le funzioni di libva che servono, prese con dlopen.
struct Lib {
    get_display_drm: unsafe extern "C" fn(c_int) -> Display,
    initialize: unsafe extern "C" fn(Display, *mut c_int, *mut c_int) -> Status,
    terminate: unsafe extern "C" fn(Display) -> Status,
    query_vendor_string: unsafe extern "C" fn(Display) -> *const c_char,
    set_error_callback: Option<unsafe extern "C" fn(Display, MessageCallback, *mut c_void) -> *mut c_void>,
    set_info_callback: Option<unsafe extern "C" fn(Display, MessageCallback, *mut c_void) -> *mut c_void>,
    max_num_profiles: unsafe extern "C" fn(Display) -> c_int,
    max_num_entrypoints: unsafe extern "C" fn(Display) -> c_int,
    query_config_profiles: unsafe extern "C" fn(Display, *mut c_int, *mut c_int) -> Status,
    query_config_entrypoints: unsafe extern "C" fn(Display, c_int, *mut c_int, *mut c_int) -> Status,
    get_config_attributes: unsafe extern "C" fn(Display, c_int, c_int, *mut ConfigAttrib, c_int) -> Status,
    create_config: unsafe extern "C" fn(Display, c_int, c_int, *mut ConfigAttrib, c_int, *mut Id) -> Status,
    destroy_config: unsafe extern "C" fn(Display, Id) -> Status,
    create_surfaces: unsafe extern "C" fn(Display, c_uint, c_uint, c_uint, *mut Id, c_uint, *mut c_void, c_uint) -> Status,
    destroy_surfaces: unsafe extern "C" fn(Display, *mut Id, c_int) -> Status,
    create_context: unsafe extern "C" fn(Display, Id, c_int, c_int, c_int, *mut Id, c_int, *mut Id) -> Status,
    destroy_context: unsafe extern "C" fn(Display, Id) -> Status,
    create_buffer: unsafe extern "C" fn(Display, Id, c_int, c_uint, c_uint, *mut c_void, *mut Id) -> Status,
    destroy_buffer: unsafe extern "C" fn(Display, Id) -> Status,
    map_buffer: unsafe extern "C" fn(Display, Id, *mut *mut c_void) -> Status,
    unmap_buffer: unsafe extern "C" fn(Display, Id) -> Status,
    begin_picture: unsafe extern "C" fn(Display, Id, Id) -> Status,
    render_picture: unsafe extern "C" fn(Display, Id, *mut Id, c_int) -> Status,
    end_picture: unsafe extern "C" fn(Display, Id) -> Status,
    sync_surface: unsafe extern "C" fn(Display, Id) -> Status,
    derive_image: unsafe extern "C" fn(Display, Id, *mut Image) -> Status,
    create_image: unsafe extern "C" fn(Display, *mut ImageFormat, c_int, c_int, *mut Image) -> Status,
    put_image:
        unsafe extern "C" fn(Display, Id, Id, c_int, c_int, c_uint, c_uint, c_int, c_int, c_uint, c_uint) -> Status,
    destroy_image: unsafe extern "C" fn(Display, Id) -> Status,
}

fn lib() -> Option<&'static Lib> {
    static LIB: OnceLock<Option<Lib>> = OnceLock::new();
    LIB.get_or_init(|| unsafe { load() }).as_ref()
}

unsafe fn open_so(names: &[&CStr]) -> Option<*mut c_void> {
    names.iter().find_map(|name| {
        let handle = unsafe { libc::dlopen(name.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        (!handle.is_null()).then_some(handle)
    })
}

unsafe fn sym<T: Copy>(handle: *mut c_void, name: &CStr) -> Option<T> {
    debug_assert_eq!(std::mem::size_of::<T>(), std::mem::size_of::<*mut c_void>());
    let ptr = unsafe { libc::dlsym(handle, name.as_ptr()) };
    (!ptr.is_null()).then(|| unsafe { std::mem::transmute_copy::<*mut c_void, T>(&ptr) })
}

unsafe fn load() -> Option<Lib> {
    // Le librerie restano aperte fino all'uscita: le funzioni prese qui servono a ogni video.
    let va = unsafe { open_so(&[c"libva.so.2", c"libva.so"])? };
    let drm = unsafe { open_so(&[c"libva-drm.so.2", c"libva-drm.so"])? };
    unsafe {
        Some(Lib {
            get_display_drm: sym(drm, c"vaGetDisplayDRM")?,
            initialize: sym(va, c"vaInitialize")?,
            terminate: sym(va, c"vaTerminate")?,
            query_vendor_string: sym(va, c"vaQueryVendorString")?,
            set_error_callback: sym(va, c"vaSetErrorCallback"),
            set_info_callback: sym(va, c"vaSetInfoCallback"),
            max_num_profiles: sym(va, c"vaMaxNumProfiles")?,
            max_num_entrypoints: sym(va, c"vaMaxNumEntrypoints")?,
            query_config_profiles: sym(va, c"vaQueryConfigProfiles")?,
            query_config_entrypoints: sym(va, c"vaQueryConfigEntrypoints")?,
            get_config_attributes: sym(va, c"vaGetConfigAttributes")?,
            create_config: sym(va, c"vaCreateConfig")?,
            destroy_config: sym(va, c"vaDestroyConfig")?,
            create_surfaces: sym(va, c"vaCreateSurfaces")?,
            destroy_surfaces: sym(va, c"vaDestroySurfaces")?,
            create_context: sym(va, c"vaCreateContext")?,
            destroy_context: sym(va, c"vaDestroyContext")?,
            create_buffer: sym(va, c"vaCreateBuffer")?,
            destroy_buffer: sym(va, c"vaDestroyBuffer")?,
            map_buffer: sym(va, c"vaMapBuffer")?,
            unmap_buffer: sym(va, c"vaUnmapBuffer")?,
            begin_picture: sym(va, c"vaBeginPicture")?,
            render_picture: sym(va, c"vaRenderPicture")?,
            end_picture: sym(va, c"vaEndPicture")?,
            sync_surface: sym(va, c"vaSyncSurface")?,
            derive_image: sym(va, c"vaDeriveImage")?,
            create_image: sym(va, c"vaCreateImage")?,
            put_image: sym(va, c"vaPutImage")?,
            destroy_image: sym(va, c"vaDestroyImage")?,
        })
    }
}

fn check(status: Status, what: &str) -> Result<(), String> {
    if status == SUCCESS {
        Ok(())
    } else {
        Err(format!("scheda video: {what} non riuscito ({status})"))
    }
}

/// Una scheda video aperta, con il profilo H.264 e il modo di codifica che sa usare.
struct Device {
    lib: &'static Lib,
    dpy: Display,
    profile: c_int,
    entrypoint: c_int,
    _node: File,
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe { (self.lib.terminate)(self.dpy) };
    }
}

impl Device {
    /// La prima scheda (/dev/dri/renderD128 e seguenti) che codifica H.264 a 8 bit 4:2:0.
    fn open() -> Option<Device> {
        let lib = lib()?;
        (128..136).find_map(|n| Self::open_node(lib, &format!("/dev/dri/renderD{n}")))
    }

    fn open_node(lib: &'static Lib, path: &str) -> Option<Device> {
        let node = OpenOptions::new().read(true).write(true).open(path).ok()?;
        unsafe {
            let dpy = (lib.get_display_drm)(node.as_raw_fd());
            if dpy.is_null() {
                return None;
            }
            // libva scrive i suoi messaggi su stderr: qui non servono.
            if let Some(set) = lib.set_error_callback {
                set(dpy, quiet, std::ptr::null_mut());
            }
            if let Some(set) = lib.set_info_callback {
                set(dpy, quiet, std::ptr::null_mut());
            }
            let (mut major, mut minor) = (0, 0);
            if (lib.initialize)(dpy, &mut major, &mut minor) != SUCCESS {
                (lib.terminate)(dpy);
                return None;
            }
            let mut device = Device { lib, dpy, profile: 0, entrypoint: 0, _node: node };
            let (profile, entrypoint) = device.find_h264()?;
            device.profile = profile;
            device.entrypoint = entrypoint;
            Some(device)
        }
    }

    fn find_h264(&self) -> Option<(c_int, c_int)> {
        unsafe {
            let mut profiles = vec![0; (self.lib.max_num_profiles)(self.dpy).max(1) as usize];
            let mut count = 0;
            if (self.lib.query_config_profiles)(self.dpy, profiles.as_mut_ptr(), &mut count) != SUCCESS {
                return None;
            }
            profiles.truncate(count.max(0) as usize);
            for profile in [PROFILE_H264_CONSTRAINED_BASELINE, PROFILE_H264_MAIN, PROFILE_H264_HIGH] {
                if !profiles.contains(&profile) {
                    continue;
                }
                let mut entries = vec![0; (self.lib.max_num_entrypoints)(self.dpy).max(1) as usize];
                let mut n = 0;
                if (self.lib.query_config_entrypoints)(self.dpy, profile, entries.as_mut_ptr(), &mut n) != SUCCESS {
                    continue;
                }
                entries.truncate(n.max(0) as usize);
                for entry in [ENTRY_ENC_SLICE, ENTRY_ENC_SLICE_LP] {
                    if entries.contains(&entry) && self.attribute(profile, entry, ATTRIB_RT_FORMAT) & RT_FORMAT_YUV420 != 0 {
                        return Some((profile, entry));
                    }
                }
            }
            None
        }
    }

    fn attribute(&self, profile: c_int, entrypoint: c_int, kind: c_int) -> u32 {
        let mut attrib = ConfigAttrib { kind, value: 0 };
        let status = unsafe { (self.lib.get_config_attributes)(self.dpy, profile, entrypoint, &mut attrib, 1) };
        if status != SUCCESS || attrib.value == ATTRIB_NOT_SUPPORTED {
            0
        } else {
            attrib.value
        }
    }

    /// Il nome della scheda, dalla descrizione del driver.
    fn name(&self) -> String {
        let vendor = unsafe {
            let raw = (self.lib.query_vendor_string)(self.dpy);
            if raw.is_null() { String::new() } else { CStr::from_ptr(raw).to_string_lossy().into_owned() }
        };
        let name = card_name(&vendor);
        if name.is_empty() { "VA-API".to_string() } else { format!("{name} · VA-API") }
    }
}

/// «Mesa Gallium driver 24.0 for AMD Radeon Graphics (radeonsi, renoir, …)» → «AMD Radeon Graphics»;
/// «Intel iHD driver for Intel(R) Gen Graphics - 24.1.0 ()» → «Intel Gen Graphics».
fn card_name(vendor: &str) -> String {
    let after = vendor.split_once(" for ").map(|(_, rest)| rest).unwrap_or(vendor);
    let cut = after.find(" (").into_iter().chain(after.find(" - ")).min().unwrap_or(after.len());
    after[..cut].replace("(R)", "").replace("(TM)", "").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// La scheda video che codifica H.264, per nome; `None` se non c'è.
pub(crate) fn probe() -> Option<String> {
    Device::open().map(|device| device.name())
}

/// Un codificatore H.264 sulla scheda video: un fotogramma I420 dentro, un pezzo di flusso Annex B fuori.
/// Solo fotogrammi I e P, un riferimento: come OpenH264 in Cedro, va bene per qualsiasi lettore.
pub(crate) struct H264Encoder {
    device: Device,
    config: Id,
    context: Id,
    surfaces: [Id; 3],
    coded: Id,
    width: u32,
    height: u32,
    mbw: u32,
    mbh: u32,
    rate_control: u32,
    bitrate: u32,
    qp: u8,
    cabac: bool,
    /// Quali intestazioni scrive Cedro (PACKED_SEQUENCE, PACKED_SLICE): radeonsi non le fa da sé.
    packed: u32,
    framerate: u32,
    level: u8,
    gop: u64,
    count: u64,
    frame_num: u16,
    idr_id: u16,
    recon: usize,
    reference: Option<(u16, i32)>,
    upload: Option<Image>,
}

impl Drop for H264Encoder {
    fn drop(&mut self) {
        let lib = self.device.lib;
        let dpy = self.device.dpy;
        unsafe {
            if let Some(image) = self.upload.take() {
                (lib.destroy_image)(dpy, image.image_id);
            }
            (lib.destroy_buffer)(dpy, self.coded);
            (lib.destroy_context)(dpy, self.context);
            (lib.destroy_surfaces)(dpy, self.surfaces.as_mut_ptr(), self.surfaces.len() as c_int);
            (lib.destroy_config)(dpy, self.config);
        }
    }
}

impl H264Encoder {
    /// `width`×`height` pari; `fps` fotogrammi al secondo; `bitrate` in bit al secondo; `quality` 40-95.
    pub(crate) fn new(width: u32, height: u32, fps: f32, bitrate: u32, quality: u8) -> Result<Self, String> {
        if width < 16 || height < 16 || width % 2 == 1 || height % 2 == 1 || width > 4096 || height > 4096 {
            return Err(format!("scheda video: {width}×{height} non adatto"));
        }
        let device = Device::open().ok_or("nessuna scheda video che codifica H.264")?;
        let lib = device.lib;
        let dpy = device.dpy;
        let (mbw, mbh) = (width.div_ceil(16), height.div_ceil(16));
        let rc_modes = device.attribute(device.profile, device.entrypoint, ATTRIB_RATE_CONTROL);
        let rate_control = [RC_VBR, RC_CBR, RC_CQP].into_iter().find(|mode| rc_modes & mode != 0).unwrap_or(0);
        let mut attribs = vec![ConfigAttrib { kind: ATTRIB_RT_FORMAT, value: RT_FORMAT_YUV420 }];
        if rate_control != 0 {
            attribs.push(ConfigAttrib { kind: ATTRIB_RATE_CONTROL, value: rate_control });
        }
        // Come ffmpeg: dove il driver le accetta, SPS, PPS e intestazione della fetta le scrive il programma.
        let packed = device.attribute(device.profile, device.entrypoint, ATTRIB_ENC_PACKED_HEADERS) & (PACKED_SEQUENCE | PACKED_SLICE);
        if packed != 0 {
            attribs.push(ConfigAttrib { kind: ATTRIB_ENC_PACKED_HEADERS, value: packed });
        }
        let fps = if fps.is_finite() && fps > 0.0 { fps.clamp(1.0, 240.0) } else { 30.0 };
        unsafe {
            let mut config = INVALID_ID;
            check(
                (lib.create_config)(dpy, device.profile, device.entrypoint, attribs.as_mut_ptr(), attribs.len() as c_int, &mut config),
                "configurazione",
            )?;
            let mut surfaces = [INVALID_ID; 3];
            let made = (lib.create_surfaces)(
                dpy,
                RT_FORMAT_YUV420,
                mbw * 16,
                mbh * 16,
                surfaces.as_mut_ptr(),
                surfaces.len() as c_uint,
                std::ptr::null_mut(),
                0,
            );
            if made != SUCCESS {
                (lib.destroy_config)(dpy, config);
                return Err(format!("scheda video: superfici non create ({made})"));
            }
            let mut context = INVALID_ID;
            let made = (lib.create_context)(
                dpy,
                config,
                (mbw * 16) as c_int,
                (mbh * 16) as c_int,
                PROGRESSIVE,
                surfaces.as_mut_ptr(),
                surfaces.len() as c_int,
                &mut context,
            );
            if made != SUCCESS {
                (lib.destroy_surfaces)(dpy, surfaces.as_mut_ptr(), surfaces.len() as c_int);
                (lib.destroy_config)(dpy, config);
                return Err(format!("scheda video: contesto non creato ({made})"));
            }
            // Un fotogramma codificato non supera quello grezzo: con il margine per le intestazioni basta.
            let coded_size = (mbw * mbh * 16 * 16 * 3 / 2).max(1 << 20) + (1 << 16);
            let mut coded = INVALID_ID;
            let made = (lib.create_buffer)(dpy, context, BUF_CODED, coded_size, 1, std::ptr::null_mut(), &mut coded);
            if made != SUCCESS {
                (lib.destroy_context)(dpy, context);
                (lib.destroy_surfaces)(dpy, surfaces.as_mut_ptr(), surfaces.len() as c_int);
                (lib.destroy_config)(dpy, config);
                return Err(format!("scheda video: memoria per il video non creata ({made})"));
            }
            Ok(H264Encoder {
                cabac: device.profile != PROFILE_H264_CONSTRAINED_BASELINE,
                packed,
                level: level_for(mbw * mbh, fps),
                device,
                config,
                context,
                surfaces,
                coded,
                width,
                height,
                mbw,
                mbh,
                rate_control,
                bitrate: bitrate.max(100_000),
                qp: qp_for(quality),
                framerate: framerate_for(fps),
                // Un fotogramma chiave ogni due secondi circa: si salta bene nel video.
                gop: ((fps * 2.0).round() as u64).clamp(1, 250),
                count: 0,
                frame_num: 0,
                idr_id: 0,
                recon: 1,
                reference: None,
                upload: None,
            })
        }
    }

    /// Codifica un fotogramma I420 compatto (Y, poi U, poi V) e ne restituisce il flusso e se è un IDR.
    pub(crate) fn encode(&mut self, i420: &[u8]) -> Result<(Vec<u8>, bool), String> {
        let (w, h) = (self.width as usize, self.height as usize);
        if i420.len() < w * h * 3 / 2 {
            return Err("scheda video: fotogramma incompleto".into());
        }
        self.upload(i420)?;
        let idr = self.count.is_multiple_of(self.gop);
        if idr {
            self.frame_num = 0;
            self.reference = None;
        }
        let poc = (2 * (self.count % self.gop)) as i32;
        let mut buffers = Vec::new();
        let result = self.submit(idr, poc, &mut buffers);
        let lib = self.device.lib;
        for buffer in buffers {
            unsafe { (lib.destroy_buffer)(self.device.dpy, buffer) };
        }
        result?;
        let stream = self.read_coded()?;
        if idr && !(has_nal(&stream, 7) && has_nal(&stream, 8)) {
            return Err("scheda video: il flusso non ha le intestazioni H.264".into());
        }
        if !(1..=5).any(|kind| has_nal(&stream, kind)) {
            return Err("scheda video: fotogramma vuoto".into());
        }
        self.reference = Some((self.frame_num, poc));
        self.recon = 3 - self.recon;
        self.frame_num = (self.frame_num + 1) % 256;
        if idr {
            self.idr_id = self.idr_id.wrapping_add(1);
        }
        self.count += 1;
        Ok((stream, idr))
    }

    fn submit(&mut self, idr: bool, poc: i32, buffers: &mut Vec<Id>) -> Result<(), String> {
        let lib = self.device.lib;
        let dpy = self.device.dpy;
        if idr {
            let seq = self.sequence();
            buffers.push(self.buffer(BUF_SEQUENCE, bytes_of(&seq))?);
            if self.rate_control == RC_VBR || self.rate_control == RC_CBR {
                let vbr = self.rate_control == RC_VBR;
                let rc = RateControl {
                    bits_per_second: if vbr { self.bitrate.saturating_mul(3) / 2 } else { self.bitrate },
                    target_percentage: if vbr { 66 } else { 100 },
                    window_size: 1000,
                    // disable_frame_skip: ogni fotogramma resta nel video.
                    rc_flags: 1 << 1,
                    ..zeroed()
                };
                buffers.push(self.buffer(BUF_MISC, &misc(MISC_RATE_CONTROL, bytes_of(&rc)))?);
            }
            let rate = FrameRate { framerate: self.framerate, ..zeroed() };
            buffers.push(self.buffer(BUF_MISC, &misc(MISC_FRAME_RATE, bytes_of(&rate)))?);
            if self.packed & PACKED_SEQUENCE != 0 {
                let mut headers = sps(self.profile(), self.level, self.mbw, self.mbh, self.width, self.height);
                headers.extend(pps(self.cabac, self.init_qp()));
                self.packed_header(HEADER_SEQUENCE, &headers, buffers)?;
            }
        }
        let current = PictureH264 {
            picture_id: self.surfaces[self.recon],
            frame_idx: self.frame_num as u32,
            flags: 0,
            top_field_order_cnt: poc,
            bottom_field_order_cnt: poc,
            reserved: [0; 4],
        };
        let reference = self.reference.map(|(frame_num, ref_poc)| PictureH264 {
            picture_id: self.surfaces[3 - self.recon],
            frame_idx: frame_num as u32,
            flags: PICTURE_SHORT_TERM,
            top_field_order_cnt: ref_poc,
            bottom_field_order_cnt: ref_poc,
            reserved: [0; 4],
        });
        let mut picture: PictureParameters = zeroed();
        picture.curr_pic = current;
        picture.reference_frames = [PictureH264::INVALID; 16];
        if let Some(reference) = reference {
            picture.reference_frames[0] = reference;
        }
        picture.coded_buf = self.coded;
        picture.frame_num = self.frame_num;
        picture.pic_init_qp = self.init_qp();
        // idr_pic_flag, reference_pic_flag = 1, entropy_coding_mode_flag, deblocking_filter_control_present_flag
        picture.pic_fields = idr as u32 | 1 << 1 | (self.cabac as u32) << 3 | 1 << 9;
        buffers.push(self.buffer(BUF_PICTURE, bytes_of(&picture))?);

        let mut slice: Box<SliceParameters> = Box::new(zeroed());
        slice.num_macroblocks = self.mbw * self.mbh;
        slice.macroblock_info = INVALID_ID;
        slice.slice_type = if idr { 2 } else { 0 };
        slice.idr_pic_id = self.idr_id;
        slice.pic_order_cnt_lsb = (poc & 0xff) as u16;
        slice.ref_pic_list0 = [PictureH264::INVALID; 32];
        slice.ref_pic_list1 = [PictureH264::INVALID; 32];
        if let (false, Some(reference)) = (idr, reference) {
            slice.ref_pic_list0[0] = reference;
        }
        if self.packed & PACKED_SLICE != 0 {
            let header = slice_header(&SliceHeader {
                idr,
                cabac: self.cabac,
                frame_num: self.frame_num,
                idr_id: self.idr_id,
                poc_lsb: (poc & 0xff) as u16,
                qp_delta: 0,
            });
            self.packed_header(HEADER_SLICE, &header, buffers)?;
        }
        buffers.push(self.buffer(BUF_SLICE, bytes_of(slice.as_ref()))?);

        unsafe {
            check((lib.begin_picture)(dpy, self.context, self.surfaces[0]), "inizio fotogramma")?;
            check((lib.render_picture)(dpy, self.context, buffers.as_mut_ptr(), buffers.len() as c_int), "fotogramma")?;
            check((lib.end_picture)(dpy, self.context), "fine fotogramma")?;
            check((lib.sync_surface)(dpy, self.surfaces[0]), "attesa della scheda")
        }
    }

    /// Il quantizzatore di partenza di ogni fotogramma, scritto anche nel PPS.
    fn init_qp(&self) -> u8 {
        if self.rate_control == RC_CQP || self.rate_control == 0 { self.qp } else { 26 }
    }

    fn profile(&self) -> Profile {
        match self.device.profile {
            PROFILE_H264_CONSTRAINED_BASELINE => Profile::ConstrainedBaseline,
            PROFILE_H264_MAIN => Profile::Main,
            _ => Profile::High,
        }
    }

    /// Un'intestazione già scritta in bit: prima il parametro con la lunghezza, poi i byte.
    fn packed_header(&self, kind: u32, data: &PackedBits, buffers: &mut Vec<Id>) -> Result<(), String> {
        let param = PackedHeader { kind, bit_length: data.bits, has_emulation_bytes: 1, reserved: [0; 4] };
        buffers.push(self.buffer(BUF_PACKED_HEADER_PARAM, bytes_of(&param))?);
        buffers.push(self.buffer(BUF_PACKED_HEADER_DATA, &data.bytes)?);
        Ok(())
    }

    fn sequence(&self) -> Box<SequenceParameters> {
        let mut seq: Box<SequenceParameters> = Box::new(zeroed());
        seq.level_idc = self.level;
        seq.intra_period = self.gop as u32;
        seq.intra_idr_period = self.gop as u32;
        seq.ip_period = 1;
        seq.bits_per_second = if self.rate_control == RC_CQP { 0 } else { self.bitrate };
        seq.max_num_ref_frames = 1;
        seq.picture_width_in_mbs = self.mbw as u16;
        seq.picture_height_in_mbs = self.mbh as u16;
        // chroma 4:2:0, solo fotogrammi, direct_8x8, frame_num e POC a 8 bit (POC tipo 0).
        seq.seq_fields = 1 | 1 << 2 | 1 << 5 | 4 << 6 | 4 << 12;
        let (crop_w, crop_h) = (self.mbw * 16 - self.width, self.mbh * 16 - self.height);
        if crop_w > 0 || crop_h > 0 {
            seq.frame_cropping_flag = 1;
            seq.frame_crop_right_offset = crop_w / 2;
            seq.frame_crop_bottom_offset = crop_h / 2;
        }
        seq
    }

    fn buffer(&self, kind: c_int, data: &[u8]) -> Result<Id, String> {
        let mut id = INVALID_ID;
        let status = unsafe {
            (self.device.lib.create_buffer)(
                self.device.dpy,
                self.context,
                kind,
                data.len() as c_uint,
                1,
                data.as_ptr() as *mut c_void,
                &mut id,
            )
        };
        check(status, "parametri")?;
        Ok(id)
    }

    /// Copia il fotogramma nella superficie d'ingresso, in NV12, ripetendo l'ultima riga e colonna
    /// fino ai multipli di 16 (la parte in più si taglia nel flusso).
    fn upload(&mut self, i420: &[u8]) -> Result<(), String> {
        let lib = self.device.lib;
        let dpy = self.device.dpy;
        let (aw, ah) = (self.mbw * 16, self.mbh * 16);
        unsafe {
            let mut derived = Image::default();
            let direct = (lib.derive_image)(dpy, self.surfaces[0], &mut derived) == SUCCESS;
            if direct && derived.format.fourcc != FOURCC_NV12 {
                (lib.destroy_image)(dpy, derived.image_id);
            }
            let direct = direct && derived.format.fourcc == FOURCC_NV12;
            let image = if direct {
                derived
            } else {
                match self.upload {
                    Some(image) => image,
                    None => {
                        let mut format = ImageFormat { fourcc: FOURCC_NV12, byte_order: 1, bits_per_pixel: 12, ..Default::default() };
                        let mut image = Image::default();
                        check((lib.create_image)(dpy, &mut format, aw as c_int, ah as c_int, &mut image), "immagine")?;
                        self.upload = Some(image);
                        image
                    }
                }
            };
            let mut ptr = std::ptr::null_mut();
            let mapped = (lib.map_buffer)(dpy, image.buf, &mut ptr);
            if mapped != SUCCESS || ptr.is_null() {
                if direct {
                    (lib.destroy_image)(dpy, image.image_id);
                }
                return Err(format!("scheda video: immagine non scrivibile ({mapped})"));
            }
            let dst = std::slice::from_raw_parts_mut(ptr as *mut u8, image.data_size as usize);
            let filled = write_nv12(dst, &image, i420, self.width as usize, self.height as usize, aw as usize, ah as usize);
            (lib.unmap_buffer)(dpy, image.buf);
            let put = if !direct && filled.is_ok() {
                (lib.put_image)(dpy, self.surfaces[0], image.image_id, 0, 0, aw, ah, 0, 0, aw, ah)
            } else {
                SUCCESS
            };
            if direct {
                (lib.destroy_image)(dpy, image.image_id);
            }
            filled?;
            check(put, "copia del fotogramma")
        }
    }

    fn read_coded(&self) -> Result<Vec<u8>, String> {
        let lib = self.device.lib;
        let dpy = self.device.dpy;
        let mut out = Vec::new();
        unsafe {
            let mut ptr = std::ptr::null_mut();
            check((lib.map_buffer)(dpy, self.coded, &mut ptr), "lettura del video")?;
            let mut segment = ptr as *const CodedSegment;
            while !segment.is_null() {
                let seg = &*segment;
                if !seg.buf.is_null() && seg.size > 0 {
                    out.extend_from_slice(std::slice::from_raw_parts(seg.buf as *const u8, seg.size as usize));
                }
                segment = seg.next as *const CodedSegment;
            }
            (lib.unmap_buffer)(dpy, self.coded);
        }
        Ok(out)
    }
}

fn write_nv12(dst: &mut [u8], image: &Image, i420: &[u8], w: usize, h: usize, aw: usize, ah: usize) -> Result<(), String> {
    let (y_pitch, uv_pitch) = (image.pitches[0] as usize, image.pitches[1] as usize);
    let (y_off, uv_off) = (image.offsets[0] as usize, image.offsets[1] as usize);
    if y_pitch < aw || uv_pitch < aw || y_off + y_pitch * (ah - 1) + aw > dst.len() || uv_off + uv_pitch * (ah / 2 - 1) + aw > dst.len() {
        return Err("scheda video: immagine troppo piccola".into());
    }
    let (cw, ch) = (w / 2, h / 2);
    let (u_plane, v_plane) = (&i420[w * h..w * h + cw * ch], &i420[w * h + cw * ch..w * h + 2 * cw * ch]);
    for row in 0..ah {
        let src = &i420[row.min(h - 1) * w..][..w];
        let line = &mut dst[y_off + row * y_pitch..][..aw];
        line[..w].copy_from_slice(src);
        line[w..].fill(src[w - 1]);
    }
    for row in 0..ah / 2 {
        let r = row.min(ch - 1);
        let (u, v) = (&u_plane[r * cw..][..cw], &v_plane[r * cw..][..cw]);
        let line = &mut dst[uv_off + row * uv_pitch..][..aw];
        for (x, pair) in line.chunks_exact_mut(2).enumerate() {
            let c = x.min(cw - 1);
            pair[0] = u[c];
            pair[1] = v[c];
        }
    }
    Ok(())
}

/// Byte già pronti per libva e quanti bit contano davvero (l'ultimo byte può essere pieno a metà).
struct PackedBits {
    bytes: Vec<u8>,
    bits: u32,
}

impl PackedBits {
    /// Unisce un'altra intestazione dopo questa: serve per mettere il PPS dietro l'SPS.
    fn extend(&mut self, other: PackedBits) {
        debug_assert!(self.bits.is_multiple_of(8));
        self.bytes.extend(other.bytes);
        self.bits += other.bits;
    }
}

/// Scrive bit per bit, con gli interi Exp-Golomb dell'H.264.
#[derive(Default)]
struct BitWriter {
    bytes: Vec<u8>,
    /// Bit già usati nell'ultimo byte (0 vuol dire che è pieno o che non c'è).
    used: u8,
}

impl BitWriter {
    fn bit(&mut self, on: bool) {
        if self.used == 0 {
            self.bytes.push(0);
        }
        if on {
            *self.bytes.last_mut().expect("byte appena aggiunto") |= 0x80 >> self.used;
        }
        self.used = (self.used + 1) % 8;
    }

    /// `n` bit (fino a 32), dal più alto.
    fn bits(&mut self, n: u32, value: u32) {
        for i in (0..n).rev() {
            self.bit(value >> i & 1 == 1);
        }
    }

    /// ue(v): intero senza segno, `value + 1` in binario preceduto da altrettanti zeri meno uno.
    fn ue(&mut self, value: u32) {
        let v = value as u64 + 1;
        let len = 64 - v.leading_zeros();
        for _ in 1..len {
            self.bit(false);
        }
        for i in (0..len).rev() {
            self.bit(v >> i & 1 == 1);
        }
    }

    /// se(v): intero con segno, 1 → 1, -1 → 2, 2 → 3, -2 → 4…
    fn se(&mut self, value: i32) {
        self.ue(if value > 0 { value as u32 * 2 - 1 } else { value.unsigned_abs() * 2 });
    }

    /// rbsp_trailing_bits: un bit a uno e poi zeri fino al byte intero.
    fn trailing(&mut self) {
        self.bit(true);
        while self.used != 0 {
            self.bit(false);
        }
    }

    fn bit_count(&self) -> u32 {
        (self.bytes.len() as u32 * 8) - if self.used == 0 { 0 } else { 8 - self.used as u32 }
    }
}

/// Un'unità NAL con il codice d'inizio, l'intestazione e i byte di protezione (00 00 0x → 00 00 03 0x).
fn nal(ref_idc: u8, kind: u8, body: &BitWriter) -> PackedBits {
    let mut bytes = vec![0, 0, 0, 1, ref_idc << 5 | kind];
    let mut zeros = 0;
    for &byte in &body.bytes {
        if zeros >= 2 && byte <= 3 {
            bytes.push(3);
            zeros = 0;
        }
        bytes.push(byte);
        zeros = if byte == 0 { zeros + 1 } else { 0 };
    }
    // L'ultimo byte di un'intestazione di fetta può essere a metà: contano solo i bit scritti.
    let bits = bytes.len() as u32 * 8 - (body.bytes.len() as u32 * 8 - body.bit_count());
    PackedBits { bytes, bits }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Profile {
    ConstrainedBaseline,
    Main,
    High,
}

/// L'SPS: 8 bit per il numero del fotogramma e per il POC (come `sequence()`), un solo riferimento.
fn sps(profile: Profile, level: u8, mbw: u32, mbh: u32, width: u32, height: u32) -> PackedBits {
    let mut w = BitWriter::default();
    let (idc, constraints) = match profile {
        Profile::ConstrainedBaseline => (66, 0x40),
        Profile::Main => (77, 0),
        Profile::High => (100, 0),
    };
    w.bits(8, idc);
    w.bits(8, constraints);
    w.bits(8, level as u32);
    w.ue(0); // seq_parameter_set_id
    if profile == Profile::High {
        w.ue(1); // chroma_format_idc: 4:2:0
        w.ue(0); // bit_depth_luma_minus8
        w.ue(0); // bit_depth_chroma_minus8
        w.bit(false); // qpprime_y_zero_transform_bypass_flag
        w.bit(false); // seq_scaling_matrix_present_flag
    }
    w.ue(4); // log2_max_frame_num_minus4
    w.ue(0); // pic_order_cnt_type
    w.ue(4); // log2_max_pic_order_cnt_lsb_minus4
    w.ue(1); // max_num_ref_frames
    w.bit(false); // gaps_in_frame_num_value_allowed_flag
    w.ue(mbw - 1);
    w.ue(mbh - 1);
    w.bit(true); // frame_mbs_only_flag
    w.bit(true); // direct_8x8_inference_flag
    let (crop_w, crop_h) = (mbw * 16 - width, mbh * 16 - height);
    if crop_w > 0 || crop_h > 0 {
        w.bit(true);
        w.ue(0);
        w.ue(crop_w / 2);
        w.ue(0);
        w.ue(crop_h / 2);
    } else {
        w.bit(false);
    }
    w.bit(false); // vui_parameters_present_flag
    w.trailing();
    nal(3, 7, &w)
}

/// Il PPS: una sola fetta per fotogramma, un riferimento, filtro anti-blocchi regolabile dalla fetta.
fn pps(cabac: bool, init_qp: u8) -> PackedBits {
    let mut w = BitWriter::default();
    w.ue(0); // pic_parameter_set_id
    w.ue(0); // seq_parameter_set_id
    w.bit(cabac); // entropy_coding_mode_flag
    w.bit(false); // bottom_field_pic_order_in_frame_present_flag
    w.ue(0); // num_slice_groups_minus1
    w.ue(0); // num_ref_idx_l0_default_active_minus1
    w.ue(0); // num_ref_idx_l1_default_active_minus1
    w.bit(false); // weighted_pred_flag
    w.bits(2, 0); // weighted_bipred_idc
    w.se(init_qp as i32 - 26); // pic_init_qp_minus26
    w.se(0); // pic_init_qs_minus26
    w.se(0); // chroma_qp_index_offset
    w.bit(true); // deblocking_filter_control_present_flag
    w.bit(false); // constrained_intra_pred_flag
    w.bit(false); // redundant_pic_cnt_present_flag
    w.trailing();
    nal(3, 8, &w)
}

struct SliceHeader {
    idr: bool,
    cabac: bool,
    frame_num: u16,
    idr_id: u16,
    poc_lsb: u16,
    qp_delta: i32,
}

/// L'intestazione della fetta (fotogramma intero), senza i dati: quelli li aggiunge la scheda.
fn slice_header(h: &SliceHeader) -> PackedBits {
    let mut w = BitWriter::default();
    w.ue(0); // first_mb_in_slice
    w.ue(if h.idr { 2 } else { 0 }); // slice_type: I o P
    w.ue(0); // pic_parameter_set_id
    w.bits(8, h.frame_num as u32);
    if h.idr {
        w.ue(h.idr_id as u32);
    }
    w.bits(8, h.poc_lsb as u32);
    if !h.idr {
        w.bit(false); // num_ref_idx_active_override_flag
        w.bit(false); // ref_pic_list_modification_flag_l0
    }
    if h.idr {
        w.bit(false); // no_output_of_prior_pics_flag
        w.bit(false); // long_term_reference_flag
    } else {
        w.bit(false); // adaptive_ref_pic_marking_mode_flag
    }
    if h.cabac && !h.idr {
        w.ue(0); // cabac_init_idc
    }
    w.se(h.qp_delta);
    w.ue(0); // disable_deblocking_filter_idc
    w.se(0); // slice_alpha_c0_offset_div2
    w.se(0); // slice_beta_offset_div2
    if h.cabac {
        // cabac_alignment_one_bit: i dati della fetta partono da un byte intero.
        while w.used != 0 {
            w.bit(true);
        }
    }
    nal(if h.idr { 3 } else { 2 }, if h.idr { 5 } else { 1 }, &w)
}

/// Un parametro «misc»: il tipo e poi la struttura.
fn misc(kind: u32, data: &[u8]) -> Vec<u8> {
    let mut out = kind.to_ne_bytes().to_vec();
    out.extend_from_slice(data);
    out
}

/// Il livello H.264 più basso che regge la dimensione e i fotogrammi al secondo.
fn level_for(mbs: u32, fps: f32) -> u8 {
    let rate = mbs as f32 * fps;
    let table: [(u8, u32, f32); 8] = [
        (30, 1620, 40_500.0),
        (31, 3600, 108_000.0),
        (32, 5120, 216_000.0),
        (40, 8192, 245_760.0),
        (42, 8704, 522_240.0),
        (50, 22_080, 589_824.0),
        (51, 36_864, 983_040.0),
        (52, 36_864, 2_073_600.0),
    ];
    table.iter().find(|(_, frame, per_sec)| mbs <= *frame && rate <= *per_sec).map(|(level, _, _)| *level).unwrap_or(52)
}

/// Qualità 40-95 → quantizzatore: 82 dà 23, come il predefinito di molti codificatori.
fn qp_for(quality: u8) -> u8 {
    (51.0 - quality.clamp(40, 95) as f32 * 0.34).round().clamp(10.0, 45.0) as u8
}

/// Fotogrammi al secondo come li vuole libva: interi, o numeratore e denominatore su 16 bit.
fn framerate_for(fps: f32) -> u32 {
    if (fps - fps.round()).abs() < 0.01 || fps * 1000.0 > 65_535.0 {
        fps.round().max(1.0) as u32
    } else {
        (1000 << 16) | (fps * 1000.0).round() as u32
    }
}

fn has_nal(stream: &[u8], kind: u8) -> bool {
    stream.windows(4).any(|w| w[0] == 0 && w[1] == 0 && w[2] == 1 && w[3] & 0x1f == kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    /// Le misure vengono da `va.h` di libva 2.20 (sizeof e offsetof compilati in C).
    #[test]
    fn layouts_match_libva() {
        assert_eq!(size_of::<PictureH264>(), 36);
        assert_eq!(size_of::<SequenceParameters>(), 1132);
        assert_eq!(std::mem::offset_of!(SequenceParameters, seq_fields), 28);
        assert_eq!(std::mem::offset_of!(SequenceParameters, frame_cropping_flag), 1068);
        assert_eq!(std::mem::offset_of!(SequenceParameters, vui_fields), 1092);
        assert_eq!(std::mem::offset_of!(SequenceParameters, time_scale), 1112);
        assert_eq!(size_of::<PictureParameters>(), 648);
        assert_eq!(std::mem::offset_of!(PictureParameters, coded_buf), 612);
        assert_eq!(std::mem::offset_of!(PictureParameters, frame_num), 620);
        assert_eq!(std::mem::offset_of!(PictureParameters, pic_fields), 628);
        assert_eq!(size_of::<SliceParameters>(), 3140);
        assert_eq!(std::mem::offset_of!(SliceParameters, ref_pic_list0), 36);
        assert_eq!(std::mem::offset_of!(SliceParameters, luma_weight_l0), 2344);
        assert_eq!(std::mem::offset_of!(SliceParameters, chroma_weight_l0), 2474);
        assert_eq!(std::mem::offset_of!(SliceParameters, cabac_init_idc), 3118);
        assert_eq!(size_of::<RateControl>(), 60);
        assert_eq!(size_of::<FrameRate>(), 24);
        assert_eq!(size_of::<PackedHeader>(), 28);
        assert_eq!(size_of::<CodedSegment>(), 48);
        assert_eq!(std::mem::offset_of!(CodedSegment, next), 24);
        assert_eq!(size_of::<ImageFormat>(), 48);
        assert_eq!(size_of::<Image>(), 120);
        assert_eq!(std::mem::offset_of!(Image, pitches), 68);
        assert_eq!(std::mem::offset_of!(Image, offsets), 80);
        assert_eq!(size_of::<ConfigAttrib>(), 8);
    }

    #[test]
    fn card_names_are_short() {
        assert_eq!(
            card_name("Mesa Gallium driver 26.2.5 for AMD Radeon Graphics (radeonsi, renoir, ACO, DRM 3.61)"),
            "AMD Radeon Graphics"
        );
        assert_eq!(card_name("Intel iHD driver for Intel(R) Gen Graphics - 24.1.0 ()"), "Intel Gen Graphics");
    }

    #[test]
    fn levels_and_rates() {
        assert_eq!(level_for(120 * 68, 30.0), 40);
        assert_eq!(level_for(80 * 45, 30.0), 31);
        assert_eq!(level_for(240 * 135, 30.0), 51);
        assert_eq!(qp_for(82), 23);
        assert_eq!(framerate_for(30.0), 30);
        assert_eq!(framerate_for(29.97), (1000 << 16) | 29_970);
    }

    #[test]
    fn nv12_repeats_the_edges() {
        let (w, h, aw, ah) = (4usize, 2usize, 6usize, 4usize);
        let mut i420 = vec![0u8; w * h * 3 / 2];
        for (i, px) in i420[..w * h].iter_mut().enumerate() {
            *px = i as u8;
        }
        i420[w * h..w * h + 2].copy_from_slice(&[100, 101]);
        i420[w * h + 2..].copy_from_slice(&[200, 201]);
        let image = Image { pitches: [8, 8, 0], offsets: [0, 32, 0], ..Default::default() };
        let mut dst = vec![0u8; 48];
        write_nv12(&mut dst, &image, &i420, w, h, aw, ah).unwrap();
        assert_eq!(&dst[..6], &[0, 1, 2, 3, 3, 3]);
        assert_eq!(&dst[24..30], &[4, 5, 6, 7, 7, 7]);
        assert_eq!(&dst[32..38], &[100, 200, 101, 201, 101, 201]);
        assert_eq!(&dst[40..46], &[100, 200, 101, 201, 101, 201]);
    }

    #[test]
    fn exp_golomb() {
        let mut w = BitWriter::default();
        for v in [0, 1, 2, 3, 4] {
            w.ue(v);
        }
        // 1 010 011 00100 00101
        assert_eq!(w.bit_count(), 1 + 3 + 3 + 5 + 5);
        assert_eq!(w.bytes, vec![0b1010_0110, 0b0100_0010, 0b1000_0000]);
        let mut w = BitWriter::default();
        for v in [0, 1, -1, 2, -2] {
            w.se(v);
        }
        // 1 010 011 00100 00101
        assert_eq!(w.bytes, vec![0b1010_0110, 0b0100_0010, 0b1000_0000]);
    }

    #[test]
    fn emulation_prevention() {
        let mut w = BitWriter::default();
        w.bits(8, 0);
        w.bits(8, 0);
        w.bits(8, 1);
        w.bits(8, 0);
        w.bits(8, 0);
        w.bits(8, 0x80);
        let nal = nal(3, 8, &w);
        assert_eq!(nal.bytes, [0, 0, 0, 1, 0x68, 0, 0, 3, 1, 0, 0, 0x80]);
        assert_eq!(nal.bits, 12 * 8);
    }

    /// Legge i campi dell'SPS che Cedro ha scritto: quello che un decodificatore ne ricava.
    #[test]
    fn sps_says_what_we_encode() {
        let found = sps(Profile::ConstrainedBaseline, 31, 11, 9, 176, 144);
        assert_eq!(&found.bytes[..8], &[0, 0, 0, 1, 0x67, 66, 0x40, 31]);
        assert_eq!(found.bits as usize, found.bytes.len() * 8);
        // id 1, frame_num 00101, POC 1, POC lsb 00101, riferimenti 010, vuoti 0, larghezza 0001011, altezza 0001001, 1 1 0 0, chiusura 1
        assert_eq!(&found.bytes[8..], &[0x96, 0x54, 0x16, 0x27, 0x20]);
        // Con 4 pixel di ritaglio a destra c'è un campo in più.
        assert!(sps(Profile::Main, 40, 12, 9, 188, 144).bytes.len() >= found.bytes.len());
    }

    #[test]
    fn headers_are_whole_nals() {
        let p = pps(false, 23);
        assert_eq!(&p.bytes[..5], &[0, 0, 0, 1, 0x68]);
        assert_eq!(p.bits as usize, p.bytes.len() * 8);
        let idr = slice_header(&SliceHeader { idr: true, cabac: false, frame_num: 0, idr_id: 1, poc_lsb: 0, qp_delta: 0 });
        assert_eq!(idr.bytes[4], 0x65);
        assert!(idr.bits as usize <= idr.bytes.len() * 8 && idr.bits as usize > idr.bytes.len() * 8 - 8);
        let cabac = slice_header(&SliceHeader { idr: false, cabac: true, frame_num: 3, idr_id: 0, poc_lsb: 6, qp_delta: -2 });
        assert_eq!(cabac.bytes[4], 0x41);
        assert_eq!(cabac.bits as usize, cabac.bytes.len() * 8);
    }

    /// Su un PC con una scheda che codifica H.264 (Intel o AMD) prova un video vero; altrove non fa niente.
    #[test]
    fn encodes_on_real_hardware() {
        // Con CEDRO_GPU_TEST=1 la scheda deve esserci: serve per le prove sul mini PC.
        let mut encoder = match H264Encoder::new(176, 144, 30.0, 400_000, 82) {
            Ok(encoder) => encoder,
            Err(err) if std::env::var_os("CEDRO_GPU_TEST").is_some() => panic!("{err}"),
            Err(_) => return,
        };
        eprintln!("scheda video: {}", encoder.device.name());
        let mut frame = vec![128u8; 176 * 144 * 3 / 2];
        let mut total = 0;
        // Con CEDRO_GPU_DUMP=<prefisso> salva flusso (.h264) e fotogrammi originali (.yuv) per confrontarli con ffmpeg.
        let dump = std::env::var("CEDRO_GPU_DUMP").ok();
        let mut stream_out = Vec::new();
        let mut source_out = Vec::new();
        for n in 0..65u32 {
            for (i, px) in frame[..176 * 144].iter_mut().enumerate() {
                *px = ((i as u32 + n * 3) % 256) as u8;
            }
            let (stream, idr) = match encoder.encode(&frame) {
                Ok(done) => done,
                // Una scheda che rifiuta il fotogramma: Cedro rifà il video col processore.
                Err(err) if std::env::var_os("CEDRO_GPU_TEST").is_none() => {
                    eprintln!("la scheda video non codifica ancora: {err}");
                    return;
                }
                Err(err) => panic!("fotogramma {n}: {err}"),
            };
            assert_eq!(idr, n % 60 == 0, "fotogramma {n}");
            total += stream.len();
            stream_out.extend_from_slice(&stream);
            source_out.extend_from_slice(&frame);
        }
        assert!(total > 0);
        if let Some(prefix) = dump {
            std::fs::write(format!("{prefix}.h264"), &stream_out).unwrap();
            std::fs::write(format!("{prefix}.yuv"), &source_out).unwrap();
        }
    }
}
