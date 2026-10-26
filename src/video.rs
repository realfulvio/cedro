use std::path::Path;
use std::sync::OnceLock;

use windows::Win32::Media::MediaFoundation::{
    eAVEncH264VProfile_Base, MFCreateAttributes, MFCreateMediaType, MFCreateMemoryBuffer, MFCreateSample,
    MFCreateSinkWriterFromURL, MFCreateSourceReaderFromURL, MFStartup, MF_MT_ALL_SAMPLES_INDEPENDENT,
    MF_MT_AVG_BITRATE, MF_MT_DEFAULT_STRIDE, MF_MT_FIXED_SIZE_SAMPLES, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE,
    MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE, MF_MT_MPEG2_PROFILE, MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SAMPLE_SIZE,
    MF_MT_SUBTYPE, MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED, MF_SOURCE_READERF_ENDOFSTREAM,
    MF_SOURCE_READERF_STREAMTICK, MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, MF_SOURCE_READER_FIRST_AUDIO_STREAM,
    MF_SOURCE_READER_FIRST_VIDEO_STREAM, MF_VERSION, MFAudioFormat_AAC, MFAudioFormat_PCM, MFMediaType_Audio,
    MFMediaType_Video, MFSTARTUP_NOSOCKET, MFVideoFormat_H264, MFVideoFormat_I420, MFVideoFormat_IYUV,
    MFVideoFormat_NV12, MFVideoFormat_RGB24, MFVideoFormat_RGB32, MFVideoFormat_YUY2, MFVideoFormat_YV12,
    MFVideoInterlace_Progressive,
};
use windows::core::GUID;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED};
use windows::core::PCWSTR;

pub fn to_mp4(input: &Path, output: &Path) -> Result<(), String> {
    let mf = to_mp4_mf(input, output);
    if mf.is_ok() {
        return mf;
    }
    match crate::softvideo::transcode::<EncodedVideo>(input, output) {
        Ok(()) => Ok(()),
        Err(err) if err == crate::softvideo::FOREIGN => mf,
        Err(err) => Err(err),
    }
}

fn create_writer(wide_out: &[u16]) -> Result<windows::Win32::Media::MediaFoundation::IMFSinkWriter, String> {
    unsafe { MFCreateSinkWriterFromURL(PCWSTR(wide_out.as_ptr()), None, None).map_err(|e| format!("MP4 non creato: {e}")) }
}

/// Dice all'encoder H.264 di seguire il bitrate (VBR con picco limitato) invece della sua qualità fissa.
/// Va chiamata dopo `AddStream` e prima di `SetInputMediaType`.
fn set_encoder_bitrate(
    writer: &windows::Win32::Media::MediaFoundation::IMFSinkWriter,
    stream: u32,
    target: &windows::Win32::Media::MediaFoundation::IMFMediaType,
    bitrate: u32,
) {
    use windows::core::Interface;
    use windows::Win32::Media::MediaFoundation::{
        IMFSinkWriterEncoderConfig, CODECAPI_AVEncCommonMeanBitRate, CODECAPI_AVEncCommonRateControlMode,
    };
    unsafe {
        let Ok(config) = writer.cast::<IMFSinkWriterEncoderConfig>() else { return };
        let mut attrs = None;
        if MFCreateAttributes(&mut attrs, 2).is_err() {
            return;
        }
        let Some(attrs) = attrs else { return };
        // 1 = eAVEncCommonRateControlMode_PeakConstrainedVBR
        let _ = attrs.SetUINT32(&CODECAPI_AVEncCommonRateControlMode, 1);
        let _ = attrs.SetUINT32(&CODECAPI_AVEncCommonMeanBitRate, bitrate);
        let _ = config.SetTargetMediaType(stream, target, &attrs);
    }
}

fn to_mp4_mf(input: &Path, output: &Path) -> Result<(), String> {
    ensure_runtime()?;
    let wide_in = wide(input);
    let wide_out = wide(output);
    unsafe {
        let mut attrs = None;
        MFCreateAttributes(&mut attrs, 1).map_err(|e| format!("video non aperto: {e}"))?;
        let attrs = attrs.ok_or("video non aperto")?;
        attrs
            .SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)
            .map_err(|e| format!("video non aperto: {e}"))?;
        let reader = MFCreateSourceReaderFromURL(PCWSTR(wide_in.as_ptr()), &attrs)
            .map_err(|e| format!("video non aperto: {e}"))?;
        let video_stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
        let audio_stream = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
        let native = reader
            .GetNativeMediaType(video_stream, 0)
            .map_err(|_| "il file non ha un video".to_string())?;
        let frame = native.GetUINT64(&MF_MT_FRAME_SIZE).unwrap_or(0);
        let (width, height) = ((frame >> 32) as u32, frame as u32);
        if width < 2 || height < 2 {
            return Err("il video non ha una dimensione".into());
        }
        // Con una risoluzione scelta più bassa i fotogrammi si rimpiccioliscono qui, prima dell'encoder.
        let shrinking = crate::sink::shape().height.is_some_and(|h| h >= 2 && h < height);
        if !shrinking && (width % 2 == 1 || height % 2 == 1) {
            return Err(format!("il video è {width}×{height}: H.264 vuole misure pari"));
        }
        let raw_rate = native.GetUINT64(&MF_MT_FRAME_RATE).unwrap_or(0);
        let rate = if raw_rate >> 32 == 0 || (raw_rate as u32) == 0 {
            (30u64 << 32) | 1
        } else {
            raw_rate
        };
        let frame = ((width as u64) << 32) | (height as u64);
        let out_rate = match crate::sink::shape().fps {
            Some(cap) if cap > 0 && (rate >> 32) / (rate as u32).max(1) as u64 > cap as u64 => ((cap as u64) << 32) | 1,
            _ => rate,
        };

        let mut opened = uncompressed_type(&reader, video_stream, &native, frame, rate)?;
        if shrinking {
            let (out_w, out_h) = crate::sink::target_size(width, height);
            let mut info = opened.convert.take().unwrap_or(FrameInfo {
                kind: Pix::Nv12,
                width,
                height,
                stride: 0,
                enc_w: width,
                enc_h: height,
                scale: None,
            });
            info.scale = Some((out_w, out_h));
            opened.convert = Some(info);
        }
        let (enc_w, enc_h) = match opened.convert.as_ref() {
            Some(f) => f.scale.unwrap_or((f.enc_w, f.enc_h)),
            None => (width, height),
        };
        let frame = ((enc_w as u64) << 32) | (enc_h as u64);

        let mut audio_in = None;
        let has_audio = !crate::sink::shape().mute && reader.GetNativeMediaType(audio_stream, 0).is_ok();
        if has_audio {
            let pcm = MFCreateMediaType().map_err(|e| e.to_string())?;
            pcm.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio).map_err(|e| e.to_string())?;
            pcm.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM).map_err(|e| e.to_string())?;
            if reader.SetCurrentMediaType(audio_stream, None, &pcm).is_ok() {
                audio_in = reader.GetCurrentMediaType(audio_stream).ok();
            }
        }

        let writer = create_writer(&wide_out)?;
        let video_out = MFCreateMediaType().map_err(|e| e.to_string())?;
        video_out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(|e| e.to_string())?;
        video_out.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264).map_err(|e| e.to_string())?;
        video_out.SetUINT64(&MF_MT_FRAME_SIZE, frame).map_err(|e| e.to_string())?;
        video_out.SetUINT64(&MF_MT_FRAME_RATE, out_rate).map_err(|e| e.to_string())?;
        video_out.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1).map_err(|e| e.to_string())?;
        video_out
            .SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
            .map_err(|e| e.to_string())?;
        video_out
            .SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_Base.0 as u32)
            .map_err(|e| e.to_string())?;
        video_out.SetUINT32(&MF_MT_AVG_BITRATE, crate::sink::bitrate(enc_w, enc_h)).map_err(|e| e.to_string())?;
        let video_index = writer
            .AddStream(&video_out)
            .map_err(|e| format!("H.264 non disponibile: {e}"))?;
        set_encoder_bitrate(&writer, video_index, &video_out, crate::sink::bitrate(enc_w, enc_h));
        let nv12 = nv12_type(frame, out_rate)?;
        if let Err(clean_err) = writer.SetInputMediaType(video_index, &nv12, None) {
            writer.SetInputMediaType(video_index, &opened.media, None).map_err(|e| {
                format!("ingresso video ({enc_w}×{enc_h}): {clean_err} / {e}")
            })?;
        }

        let mut audio_index = None;
        if let Some(pcm) = audio_in.as_ref() {
            let channels = pcm.GetUINT32(&windows::Win32::Media::MediaFoundation::MF_MT_AUDIO_NUM_CHANNELS).unwrap_or(2);
            let sample_rate = pcm
                .GetUINT32(&windows::Win32::Media::MediaFoundation::MF_MT_AUDIO_SAMPLES_PER_SECOND)
                .unwrap_or(44100);
            let aac = MFCreateMediaType().map_err(|e| e.to_string())?;
            aac.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio).map_err(|e| e.to_string())?;
            aac.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_AAC).map_err(|e| e.to_string())?;
            aac.SetUINT32(&windows::Win32::Media::MediaFoundation::MF_MT_AUDIO_NUM_CHANNELS, channels)
                .map_err(|e| e.to_string())?;
            aac.SetUINT32(&windows::Win32::Media::MediaFoundation::MF_MT_AUDIO_SAMPLES_PER_SECOND, sample_rate)
                .map_err(|e| e.to_string())?;
            aac.SetUINT32(&windows::Win32::Media::MediaFoundation::MF_MT_AUDIO_BITS_PER_SAMPLE, 16)
                .map_err(|e| e.to_string())?;
            aac.SetUINT32(&MF_MT_AVG_BITRATE, 128_000).map_err(|e| e.to_string())?;
            if let Ok(index) = writer.AddStream(&aac) {
                if writer.SetInputMediaType(index, pcm, None).is_ok() {
                    audio_index = Some(index);
                }
            }
        }

        writer.BeginWriting().map_err(|e| format!("scrittura MP4 non partita: {e}"))?;
        let num = rate >> 32;
        let den = (rate as u32).max(1) as u64;
        let frame_ticks = 10_000_000u64.saturating_mul(den) / num.max(1);
        let mut gate = crate::sink::FrameGate::new();
        let frames = pump(&reader, &writer, video_stream, video_index, Some(frame_ticks), opened.convert.as_ref(), Some(&mut gate))?;
        if frames == 0 {
            return Err("il video non ha fotogrammi".into());
        }
        if let Some(index) = audio_index {
            pump(&reader, &writer, audio_stream, index, None, None, None)?;
        }
        writer.Finalize().map_err(|e| format!("MP4 non chiuso: {e}"))?;
    }
    Ok(())
}

struct OpenedVideo {
    media: windows::Win32::Media::MediaFoundation::IMFMediaType,
    convert: Option<FrameInfo>,
}

struct FrameInfo {
    kind: Pix,
    width: u32,
    height: u32,
    stride: i32,
    enc_w: u32,
    enc_h: u32,
    /// Dimensioni di uscita quando il video si rimpicciolisce.
    scale: Option<(u32, u32)>,
}

#[derive(Clone, Copy, PartialEq)]
enum Pix {
    Nv12,
    Yuy2,
    I420,
    Yv12,
    Rgb24,
    Rgb32,
}

fn uncompressed_type(
    reader: &windows::Win32::Media::MediaFoundation::IMFSourceReader,
    stream: u32,
    native: &windows::Win32::Media::MediaFoundation::IMFMediaType,
    frame: u64,
    rate: u64,
) -> Result<OpenedVideo, String> {
    unsafe {
        let mut last = "questo video non si può decomprimere".to_string();
        let attempts: Vec<windows::Win32::Media::MediaFoundation::IMFMediaType> = {
            let mut list = Vec::new();
            if native.GetGUID(&MF_MT_SUBTYPE).ok().as_ref() == Some(&MFVideoFormat_NV12) {
                list.push(native.clone());
            }
            if let Ok(media) = nv12_type(frame, rate) {
                list.push(media);
            }
            for subtype in [
                &MFVideoFormat_NV12,
                &MFVideoFormat_YUY2,
                &MFVideoFormat_I420,
                &MFVideoFormat_IYUV,
                &MFVideoFormat_YV12,
                &MFVideoFormat_RGB32,
                &MFVideoFormat_RGB24,
            ] {
                if let Ok(media) = lean_type(subtype, frame, rate) {
                    list.push(media);
                }
            }
            if let Some(kind) = native.GetGUID(&MF_MT_SUBTYPE).ok().as_ref().and_then(pix_of) {
                if kind != Pix::Nv12 {
                    list.push(native.clone());
                }
            }
            list
        };
        for media in attempts {
            match reader.SetCurrentMediaType(stream, None, &media) {
                Ok(()) => {
                    let current = reader.GetCurrentMediaType(stream).map_err(|e| e.to_string())?;
                    let convert = frame_plan(&current, frame)?;
                    return Ok(OpenedVideo { media: current, convert });
                }
                Err(e) => last = format!("questo video non si può decomprimere: {e}"),
            }
        }
        Err(last)
    }
}

fn pix_of(subtype: &GUID) -> Option<Pix> {
    if subtype == &MFVideoFormat_NV12 {
        Some(Pix::Nv12)
    } else if subtype == &MFVideoFormat_YUY2 {
        Some(Pix::Yuy2)
    } else if subtype == &MFVideoFormat_I420 || subtype == &MFVideoFormat_IYUV {
        Some(Pix::I420)
    } else if subtype == &MFVideoFormat_YV12 {
        Some(Pix::Yv12)
    } else if subtype == &MFVideoFormat_RGB24 {
        Some(Pix::Rgb24)
    } else if subtype == &MFVideoFormat_RGB32 {
        Some(Pix::Rgb32)
    } else {
        None
    }
}

fn frame_plan(
    media: &windows::Win32::Media::MediaFoundation::IMFMediaType,
    frame: u64,
) -> Result<Option<FrameInfo>, String> {
    unsafe {
        let subtype = media.GetGUID(&MF_MT_SUBTYPE).map_err(|e| e.to_string())?;
        let kind = pix_of(&subtype).ok_or_else(|| "formato video intermedio non letto".to_string())?;
        let width = (frame >> 32) as u32;
        let height = frame as u32;
        let stride = media.GetUINT32(&MF_MT_DEFAULT_STRIDE).map(|v| v as i32).unwrap_or(0);
        let enc_w = width + (width & 1);
        let enc_h = height + (height & 1);
        let packed = kind == Pix::Nv12 && enc_w == width && enc_h == height;
        if packed {
            return Ok(None);
        }
        Ok(Some(FrameInfo { kind, width, height, stride, enc_w, enc_h, scale: None }))
    }
}

fn lean_type(
    subtype: &GUID,
    frame: u64,
    rate: u64,
) -> Result<windows::Win32::Media::MediaFoundation::IMFMediaType, String> {
    unsafe {
        let media = MFCreateMediaType().map_err(|e| e.to_string())?;
        media.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(|e| e.to_string())?;
        media.SetGUID(&MF_MT_SUBTYPE, subtype).map_err(|e| e.to_string())?;
        media.SetUINT64(&MF_MT_FRAME_SIZE, frame).map_err(|e| e.to_string())?;
        media.SetUINT64(&MF_MT_FRAME_RATE, rate).map_err(|e| e.to_string())?;
        media.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1).map_err(|e| e.to_string())?;
        media
            .SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
            .map_err(|e| e.to_string())?;
        Ok(media)
    }
}

fn nv12_type(frame: u64, rate: u64) -> Result<windows::Win32::Media::MediaFoundation::IMFMediaType, String> {
    unsafe {
        let width = (frame >> 32) as u32;
        let height = (frame as u32).max(1);
        let media = MFCreateMediaType().map_err(|e| e.to_string())?;
        media.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(|e| e.to_string())?;
        media.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12).map_err(|e| e.to_string())?;
        media.SetUINT64(&MF_MT_FRAME_SIZE, frame).map_err(|e| e.to_string())?;
        media.SetUINT64(&MF_MT_FRAME_RATE, rate).map_err(|e| e.to_string())?;
        media.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1).map_err(|e| e.to_string())?;
        media
            .SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
            .map_err(|e| e.to_string())?;
        media.SetUINT32(&MF_MT_DEFAULT_STRIDE, width).map_err(|e| e.to_string())?;
        media.SetUINT32(&MF_MT_ALL_SAMPLES_INDEPENDENT, 1).map_err(|e| e.to_string())?;
        media.SetUINT32(&MF_MT_FIXED_SIZE_SAMPLES, 1).map_err(|e| e.to_string())?;
        let sample = width.saturating_mul(height).saturating_mul(3) / 2;
        media.SetUINT32(&MF_MT_SAMPLE_SIZE, sample.max(1)).map_err(|e| e.to_string())?;
        Ok(media)
    }
}

fn pump(
    reader: &windows::Win32::Media::MediaFoundation::IMFSourceReader,
    writer: &windows::Win32::Media::MediaFoundation::IMFSinkWriter,
    source: u32,
    dest: u32,
    frame_ticks: Option<u64>,
    convert: Option<&FrameInfo>,
    mut gate: Option<&mut crate::sink::FrameGate>,
) -> Result<u32, String> {
    let mut written = 0u32;
    let mut ticks = 0u64;
    unsafe {
        loop {
            let mut flags: u32 = 0;
            let mut sample = None;
            reader
                .ReadSample(source, 0, None, Some(&mut flags), None, Some(&mut sample))
                .map_err(|e| format!("lettura video: {e}"))?;
            let eos = flags & (MF_SOURCE_READERF_ENDOFSTREAM.0 as u32) != 0;
            if let Some(sample) = sample {
                if sample.GetSampleTime().is_err() {
                    if let Some(step) = frame_ticks {
                        sample.SetSampleTime(ticks as i64).map_err(|e| format!("tempo del fotogramma: {e}"))?;
                        sample
                            .SetSampleDuration(step as i64)
                            .map_err(|e| format!("tempo del fotogramma: {e}"))?;
                        ticks = ticks.saturating_add(step);
                    }
                }
                if let Some(g) = gate.as_deref_mut() {
                    if let Ok(time) = sample.GetSampleTime() {
                        if !g.pass(time) {
                            if eos {
                                break;
                            }
                            continue;
                        }
                        if g.gap() > 0 {
                            let kept = sample.GetSampleDuration().unwrap_or(0);
                            let _ = sample.SetSampleDuration(kept.max(g.gap()));
                        }
                    }
                }
                if let Some(info) = convert {
                    let packed = repack(&sample, info)?;
                    writer.WriteSample(dest, &packed).map_err(|e| format!("scrittura video: {e}"))?;
                } else {
                    writer.WriteSample(dest, &sample).map_err(|e| format!("scrittura video: {e}"))?;
                }
                written += 1;
                if eos {
                    break;
                }
                continue;
            }
            if eos {
                break;
            }
            if flags & ((MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 as u32) | (MF_SOURCE_READERF_STREAMTICK.0 as u32)) != 0
            {
                continue;
            }
            return Err("lettura video interrotta".into());
        }
    }
    Ok(written)
}

fn repack(
    sample: &windows::Win32::Media::MediaFoundation::IMFSample,
    info: &FrameInfo,
) -> Result<windows::Win32::Media::MediaFoundation::IMFSample, String> {
    unsafe {
        let src = sample.ConvertToContiguousBuffer().map_err(|e| format!("fotogramma non letto: {e}"))?;
        let mut ptr = std::ptr::null_mut();
        let mut cur = 0u32;
        src.Lock(&mut ptr, None, Some(&mut cur)).map_err(|e| format!("fotogramma non letto: {e}"))?;
        if ptr.is_null() {
            let _ = src.Unlock();
            return Err("fotogramma vuoto".into());
        }
        let bytes = std::slice::from_raw_parts(ptr, cur as usize).to_vec();
        src.Unlock().map_err(|e| format!("fotogramma non letto: {e}"))?;
        let mut nv12 = to_nv12(&bytes, info)?;
        if let Some((out_w, out_h)) = info.scale {
            nv12 = scale_nv12(&nv12, info.enc_w, info.enc_h, out_w, out_h);
        }
        let buffer = MFCreateMemoryBuffer(nv12.len() as u32).map_err(|e| e.to_string())?;
        let mut dst = std::ptr::null_mut();
        buffer.Lock(&mut dst, None, None).map_err(|e| e.to_string())?;
        if dst.is_null() {
            let _ = buffer.Unlock();
            return Err("buffer video vuoto".into());
        }
        std::ptr::copy_nonoverlapping(nv12.as_ptr(), dst, nv12.len());
        buffer.Unlock().map_err(|e| e.to_string())?;
        buffer.SetCurrentLength(nv12.len() as u32).map_err(|e| e.to_string())?;
        let out = MFCreateSample().map_err(|e| e.to_string())?;
        out.AddBuffer(&buffer).map_err(|e| e.to_string())?;
        if let Ok(time) = sample.GetSampleTime() {
            out.SetSampleTime(time).map_err(|e| format!("tempo del fotogramma: {e}"))?;
        }
        if let Ok(duration) = sample.GetSampleDuration() {
            out.SetSampleDuration(duration).map_err(|e| format!("tempo del fotogramma: {e}"))?;
        }
        Ok(out)
    }
}

/// Un fotogramma NV12 senza padding riportato a `dw`×`dh`.
fn scale_nv12(nv12: &[u8], sw: u32, sh: u32, dw: u32, dh: u32) -> Vec<u8> {
    let luma = (sw * sh) as usize;
    let (cw, ch) = ((sw / 2) as usize, (sh / 2) as usize);
    let mut u = Vec::with_capacity(cw * ch);
    let mut v = Vec::with_capacity(cw * ch);
    for pair in nv12[luma..luma + cw * ch * 2].chunks_exact(2) {
        u.push(pair[0]);
        v.push(pair[1]);
    }
    let (y2, u2, v2) = crate::sink::resize_i420(&nv12[..luma], &u, &v, sw, sh, dw, dh);
    let mut out = y2;
    for (a, b) in u2.iter().zip(v2.iter()) {
        out.push(*a);
        out.push(*b);
    }
    out
}

fn to_nv12(src: &[u8], info: &FrameInfo) -> Result<Vec<u8>, String> {
    let dst_w = info.enc_w as usize;
    let dst_h = info.enc_h as usize;
    let mut nv12 = vec![16u8; dst_w * dst_h * 3 / 2];
    for byte in nv12.iter_mut().skip(dst_w * dst_h) {
        *byte = 128;
    }
    match info.kind {
        Pix::Rgb24 => rgb_into(src, info, 3, &mut nv12)?,
        Pix::Rgb32 => rgb_into(src, info, 4, &mut nv12)?,
        Pix::Yuy2 => yuy2_into(src, info, &mut nv12)?,
        Pix::I420 => planar_into(src, info, false, &mut nv12)?,
        Pix::Yv12 => planar_into(src, info, true, &mut nv12)?,
        Pix::Nv12 => nv12_into(src, info, &mut nv12)?,
    }
    Ok(nv12)
}

fn rgb_into(src: &[u8], info: &FrameInfo, bpp: usize, nv12: &mut [u8]) -> Result<(), String> {
    let src_w = info.width as usize;
    let src_h = info.height as usize;
    let dst_w = info.enc_w as usize;
    let dst_h = info.enc_h as usize;
    let stride = if info.stride != 0 {
        info.stride
    } else {
        ((src_w * bpp + 3) & !3) as i32
    };
    for y in (0..dst_h).step_by(2) {
        for x in (0..dst_w).step_by(2) {
            let mut u_sum = 0i32;
            let mut v_sum = 0i32;
            for dy in 0..2 {
                for dx in 0..2 {
                    let sy = (y + dy).min(src_h - 1);
                    let sx = (x + dx).min(src_w - 1);
                    let row = image_row(src, sy, src_h, stride)?;
                    let at = sx * bpp;
                    if at + 2 >= row.len() {
                        return Err("fotogramma troppo corto".into());
                    }
                    let (y_val, u_val, v_val) = yuv(row[at + 2] as i32, row[at + 1] as i32, row[at] as i32);
                    nv12[(y + dy) * dst_w + (x + dx)] = y_val;
                    u_sum += u_val as i32;
                    v_sum += v_val as i32;
                }
            }
            let uv = dst_w * dst_h + (y / 2) * dst_w + x;
            nv12[uv] = (u_sum / 4) as u8;
            nv12[uv + 1] = (v_sum / 4) as u8;
        }
    }
    Ok(())
}

fn yuy2_into(src: &[u8], info: &FrameInfo, nv12: &mut [u8]) -> Result<(), String> {
    let src_w = info.width as usize;
    let src_h = info.height as usize;
    let dst_w = info.enc_w as usize;
    let dst_h = info.enc_h as usize;
    let stride = if info.stride != 0 { info.stride } else { (src_w * 2) as i32 };
    for y in 0..dst_h.min(src_h) {
        let row = image_row(src, y, src_h, stride)?;
        for x in 0..dst_w.min(src_w) {
            let pair = x & !1;
            let at = pair * 2;
            if at + 3 >= row.len() {
                return Err("fotogramma troppo corto".into());
            }
            nv12[y * dst_w + x] = row[x * 2];
            if y % 2 == 0 && x % 2 == 0 {
                let uv = dst_w * dst_h + (y / 2) * dst_w + x;
                nv12[uv] = row[at + 1];
                nv12[uv + 1] = row[at + 3];
            }
        }
    }
    Ok(())
}

fn planar_into(src: &[u8], info: &FrameInfo, swap_uv: bool, nv12: &mut [u8]) -> Result<(), String> {
    let src_w = info.width as usize;
    let src_h = info.height as usize;
    let dst_w = info.enc_w as usize;
    let dst_h = info.enc_h as usize;
    let y_stride = if info.stride > 0 { info.stride as usize } else { src_w };
    let c_stride = (y_stride / 2).max(1);
    let y_plane = y_stride * src_h;
    let c_plane = c_stride * (src_h / 2);
    if src.len() < y_plane + c_plane * 2 {
        return Err("fotogramma troppo corto".into());
    }
    let (u_at, v_at) = if swap_uv {
        (y_plane + c_plane, y_plane)
    } else {
        (y_plane, y_plane + c_plane)
    };
    for y in 0..dst_h.min(src_h) {
        let row = y * y_stride;
        nv12[y * dst_w..y * dst_w + dst_w.min(src_w)].copy_from_slice(&src[row..row + dst_w.min(src_w)]);
        if y % 2 == 0 {
            let crow = (y / 2) * c_stride;
            for x in (0..dst_w.min(src_w)).step_by(2) {
                let uv = dst_w * dst_h + (y / 2) * dst_w + x;
                nv12[uv] = src[u_at + crow + x / 2];
                nv12[uv + 1] = src[v_at + crow + x / 2];
            }
        }
    }
    Ok(())
}

fn nv12_into(src: &[u8], info: &FrameInfo, nv12: &mut [u8]) -> Result<(), String> {
    let src_w = info.width as usize;
    let src_h = info.height as usize;
    let dst_w = info.enc_w as usize;
    let dst_h = info.enc_h as usize;
    let stride = if info.stride > 0 { info.stride as usize } else { src_w };
    let y_bytes = stride * src_h;
    let uv_bytes = stride * (src_h / 2);
    if src.len() < y_bytes + uv_bytes {
        return Err("fotogramma troppo corto".into());
    }
    for y in 0..dst_h.min(src_h) {
        let copy = dst_w.min(src_w);
        nv12[y * dst_w..y * dst_w + copy].copy_from_slice(&src[y * stride..y * stride + copy]);
        if y % 2 == 0 {
            let from = y_bytes + (y / 2) * stride;
            let to = dst_w * dst_h + (y / 2) * dst_w;
            nv12[to..to + copy].copy_from_slice(&src[from..from + copy]);
        }
    }
    Ok(())
}

fn image_row(src: &[u8], y: usize, height: usize, stride: i32) -> Result<&[u8], String> {
    let abs = stride.unsigned_abs() as usize;
    if abs == 0 || height == 0 {
        return Err("fotogramma troppo corto".into());
    }
    let offset = if stride >= 0 { y * abs } else { (height - 1 - y) * abs };
    let end = offset + abs;
    if end > src.len() {
        return Err("fotogramma troppo corto".into());
    }
    Ok(&src[offset..end])
}

fn yuv(r: i32, g: i32, b: i32) -> (u8, u8, u8) {
    let y = (((66 * r + 129 * g + 25 * b + 128) >> 8) + 16).clamp(0, 255) as u8;
    let u = (((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128).clamp(0, 255) as u8;
    let v = (((112 * r - 94 * g - 18 * b + 128) >> 8) + 128).clamp(0, 255) as u8;
    (y, u, v)
}

pub(super) struct EncodedVideo {
    writer: windows::Win32::Media::MediaFoundation::IMFSinkWriter,
    video_index: u32,
    audio_index: Option<u32>,
    audio_channels: u16,
    audio_rate: u32,
    audio_ticks: u64,
    resample_to: Option<u32>,
    src_w: u32,
    src_h: u32,
    out_w: u32,
    out_h: u32,
    last_time: i64,
    gate: crate::sink::FrameGate,
    muted: bool,
}

impl EncodedVideo {
    pub(super) fn open(
        output: &Path,
        width: u32,
        height: u32,
        frame_duration: i64,
        audio: Option<(u32, u16)>,
    ) -> Result<Self, String> {
        ensure_runtime()?;
        let (enc_w, enc_h) = crate::sink::target_size(width, height);
        if enc_w < 2 || enc_h < 2 {
            return Err("il video non ha una dimensione".into());
        }
        let gate = crate::sink::FrameGate::new();
        let muted = crate::sink::shape().mute;
        let audio = if muted { None } else { audio };
        let duration = if frame_duration > 0 { frame_duration } else { 333_333 }.max(gate.gap());
        let rate = rate_from_ticks(duration as u64);
        let frame = ((enc_w as u64) << 32) | (enc_h as u64);
        let wide_out = wide(output);
        unsafe {
            let writer = create_writer(&wide_out)?;
            let video_out = MFCreateMediaType().map_err(|e| e.to_string())?;
            video_out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(|e| e.to_string())?;
            video_out.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264).map_err(|e| e.to_string())?;
            video_out.SetUINT64(&MF_MT_FRAME_SIZE, frame).map_err(|e| e.to_string())?;
            video_out.SetUINT64(&MF_MT_FRAME_RATE, rate).map_err(|e| e.to_string())?;
            video_out
                .SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1)
                .map_err(|e| e.to_string())?;
            video_out
                .SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
                .map_err(|e| e.to_string())?;
            video_out
                .SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_Base.0 as u32)
                .map_err(|e| e.to_string())?;
            video_out.SetUINT32(&MF_MT_AVG_BITRATE, crate::sink::bitrate(enc_w, enc_h)).map_err(|e| e.to_string())?;
            let video_index = writer
                .AddStream(&video_out)
                .map_err(|e| format!("H.264 non disponibile: {e}"))?;
            set_encoder_bitrate(&writer, video_index, &video_out, crate::sink::bitrate(enc_w, enc_h));
            let nv12 = nv12_type(frame, rate)?;
            writer
                .SetInputMediaType(video_index, &nv12, None)
                .map_err(|e| format!("ingresso video ({enc_w}×{enc_h}): {e}"))?;
            let mut encoded = EncodedVideo {
                writer,
                video_index,
                audio_index: None,
                audio_channels: 0,
                audio_rate: 0,
                audio_ticks: 0,
                resample_to: None,
                src_w: width,
                src_h: height,
                out_w: enc_w,
                out_h: enc_h,
                last_time: -1,
                gate,
                muted,
            };
            if let Some((rate, channels)) = audio {
                encoded.attach_audio(rate, channels)?;
            }
            encoded
                .writer
                .BeginWriting()
                .map_err(|e| format!("scrittura MP4 non partita: {e}"))?;
            Ok(encoded)
        }
    }

    pub(super) fn write_yuv(
        &mut self,
        y: &[u8],
        u: &[u8],
        v: &[u8],
        width: u32,
        height: u32,
        time: i64,
        duration: i64,
    ) -> Result<(), String> {
        if width != self.src_w || height != self.src_h {
            return Err("il video cambia dimensione".into());
        }
        if !self.gate.pass(time) {
            return Ok(());
        }
        let resized;
        let (y, u, v, width, height) = if (width + (width & 1), height + (height & 1)) != (self.out_w, self.out_h) {
            resized = crate::sink::resize_i420(y, u, v, width, height, self.out_w, self.out_h);
            (&resized.0[..], &resized.1[..], &resized.2[..], self.out_w, self.out_h)
        } else {
            (y, u, v, width, height)
        };
        let (nv12, _, _) = yuv_to_nv12(y, u, v, width, height)?;
        let duration = duration.max(self.gate.gap()).max(1);
        let time = if time <= self.last_time {
            self.last_time.saturating_add(duration)
        } else {
            time
        };
        self.last_time = time;
        let sample = memory_sample(&nv12, time, duration)?;
        unsafe {
            self.writer
                .WriteSample(self.video_index, &sample)
                .map_err(|e| format!("scrittura video: {e}"))?;
        }
        Ok(())
    }

    pub(super) fn write_pcm(&mut self, samples: &[i16], channels: u16, rate: u32) -> Result<(), String> {
        if self.muted {
            return Ok(());
        }
        let index = self.audio_index.ok_or("audio senza traccia")?;
        if samples.is_empty() || channels == 0 || rate == 0 {
            return Ok(());
        }
        let channels = channels.max(1);
        let folded = if channels > 2 {
            fold_stereo(samples, channels)
        } else {
            samples.to_vec()
        };
        let channels = channels.min(2);
        let (folded, rate) = if self.resample_to.is_some() && rate != self.audio_rate {
            (resample_pcm(&folded, channels, rate, self.audio_rate), self.audio_rate)
        } else {
            (folded, rate)
        };
        if channels != self.audio_channels || rate != self.audio_rate {
            return Err("l'audio del video non coincide col formato previsto".into());
        }
        let frames = folded.len() / channels as usize;
        if frames == 0 {
            return Ok(());
        }
        let mut bytes = Vec::with_capacity(folded.len() * 2);
        for sample in &folded {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        let duration = (10_000_000u64 * frames as u64 / rate as u64).max(1);
        let sample = memory_sample(&bytes, self.audio_ticks as i64, duration as i64)?;
        unsafe {
            self.writer
                .WriteSample(index, &sample)
                .map_err(|e| format!("scrittura audio: {e}"))?;
        }
        self.audio_ticks = self.audio_ticks.saturating_add(duration);
        Ok(())
    }

    pub(super) fn finish(self) -> Result<(), String> {
        unsafe {
            self.writer.Finalize().map_err(|e| format!("MP4 non chiuso: {e}"))?;
        }
        Ok(())
    }

}

impl crate::sink::VideoSink for EncodedVideo {
    fn open(
        output: &Path,
        width: u32,
        height: u32,
        frame_duration: i64,
        audio: Option<(u32, u16)>,
    ) -> Result<Self, String> {
        EncodedVideo::open(output, width, height, frame_duration, audio)
    }

    fn write_yuv(
        &mut self,
        y: &[u8],
        u: &[u8],
        v: &[u8],
        width: u32,
        height: u32,
        time: i64,
        duration: i64,
    ) -> Result<(), String> {
        EncodedVideo::write_yuv(self, y, u, v, width, height, time, duration)
    }

    fn write_pcm(&mut self, samples: &[i16], channels: u16, rate: u32) -> Result<(), String> {
        EncodedVideo::write_pcm(self, samples, channels, rate)
    }

    fn finish(self) -> Result<(), String> {
        EncodedVideo::finish(self)
    }
}

impl EncodedVideo {
    fn attach_audio(&mut self, rate: u32, channels: u16) -> Result<(), String> {
        let out_channels = if channels > 2 { 2 } else { channels.max(1) };
        let (out_rate, resample) = if matches!(rate, 44100 | 48000) {
            (rate, None)
        } else {
            (44100, Some(44100))
        };
        self.resample_to = resample;
        self.add_audio_stream(out_rate, out_channels)
    }

    fn add_audio_stream(&mut self, rate: u32, channels: u16) -> Result<(), String> {
        unsafe {
            let pcm = pcm_type(rate, channels)?;
            let aac = MFCreateMediaType().map_err(|e| e.to_string())?;
            aac.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio).map_err(|e| e.to_string())?;
            aac.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_AAC).map_err(|e| e.to_string())?;
            aac.SetUINT32(
                &windows::Win32::Media::MediaFoundation::MF_MT_AUDIO_NUM_CHANNELS,
                channels as u32,
            )
            .map_err(|e| e.to_string())?;
            aac.SetUINT32(
                &windows::Win32::Media::MediaFoundation::MF_MT_AUDIO_SAMPLES_PER_SECOND,
                rate,
            )
            .map_err(|e| e.to_string())?;
            aac.SetUINT32(
                &windows::Win32::Media::MediaFoundation::MF_MT_AUDIO_BITS_PER_SAMPLE,
                16,
            )
            .map_err(|e| e.to_string())?;
            aac.SetUINT32(&MF_MT_AVG_BITRATE, 128_000).map_err(|e| e.to_string())?;
            let index = self.writer.AddStream(&aac).map_err(|e| format!("AAC non disponibile: {e}"))?;
            self.writer
                .SetInputMediaType(index, &pcm, None)
                .map_err(|e| format!("ingresso audio: {e}"))?;
            self.audio_index = Some(index);
            self.audio_channels = channels;
            self.audio_rate = rate;
            Ok(())
        }
    }
}

fn pcm_type(
    rate: u32,
    channels: u16,
) -> Result<windows::Win32::Media::MediaFoundation::IMFMediaType, String> {
    unsafe {
        let block = channels as u32 * 2;
        let pcm = MFCreateMediaType().map_err(|e| e.to_string())?;
        pcm.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio).map_err(|e| e.to_string())?;
        pcm.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM).map_err(|e| e.to_string())?;
        pcm.SetUINT32(
            &windows::Win32::Media::MediaFoundation::MF_MT_AUDIO_NUM_CHANNELS,
            channels as u32,
        )
        .map_err(|e| e.to_string())?;
        pcm.SetUINT32(
            &windows::Win32::Media::MediaFoundation::MF_MT_AUDIO_SAMPLES_PER_SECOND,
            rate,
        )
        .map_err(|e| e.to_string())?;
        pcm.SetUINT32(
            &windows::Win32::Media::MediaFoundation::MF_MT_AUDIO_BITS_PER_SAMPLE,
            16,
        )
        .map_err(|e| e.to_string())?;
        pcm.SetUINT32(
            &windows::Win32::Media::MediaFoundation::MF_MT_AUDIO_BLOCK_ALIGNMENT,
            block,
        )
        .map_err(|e| e.to_string())?;
        pcm.SetUINT32(
            &windows::Win32::Media::MediaFoundation::MF_MT_AUDIO_AVG_BYTES_PER_SECOND,
            rate.saturating_mul(block),
        )
        .map_err(|e| e.to_string())?;
        Ok(pcm)
    }
}

fn memory_sample(
    bytes: &[u8],
    time: i64,
    duration: i64,
) -> Result<windows::Win32::Media::MediaFoundation::IMFSample, String> {
    unsafe {
        let buffer = MFCreateMemoryBuffer(bytes.len() as u32).map_err(|e| e.to_string())?;
        let mut dst = std::ptr::null_mut();
        buffer.Lock(&mut dst, None, None).map_err(|e| e.to_string())?;
        if dst.is_null() {
            let _ = buffer.Unlock();
            return Err("buffer video vuoto".into());
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), dst, bytes.len());
        buffer.Unlock().map_err(|e| e.to_string())?;
        buffer.SetCurrentLength(bytes.len() as u32).map_err(|e| e.to_string())?;
        let sample = MFCreateSample().map_err(|e| e.to_string())?;
        sample.AddBuffer(&buffer).map_err(|e| e.to_string())?;
        sample.SetSampleTime(time).map_err(|e| format!("tempo del fotogramma: {e}"))?;
        sample
            .SetSampleDuration(duration.max(1))
            .map_err(|e| format!("tempo del fotogramma: {e}"))?;
        Ok(sample)
    }
}

fn yuv_to_nv12(y: &[u8], u: &[u8], v: &[u8], width: u32, height: u32) -> Result<(Vec<u8>, u32, u32), String> {
    let w = width as usize;
    let h = height as usize;
    if w < 2 || h < 2 || y.len() < w * h {
        return Err("fotogramma troppo corto".into());
    }
    let cw = w.div_ceil(2);
    let ch = h.div_ceil(2);
    if u.len() < cw * ch || v.len() < cw * ch {
        return Err("fotogramma troppo corto".into());
    }
    let ew = w + (w & 1);
    let eh = h + (h & 1);
    let mut nv12 = vec![16u8; ew * eh * 3 / 2];
    for byte in nv12.iter_mut().skip(ew * eh) {
        *byte = 128;
    }
    for row in 0..h {
        let at = row * ew;
        nv12[at..at + w].copy_from_slice(&y[row * w..row * w + w]);
    }
    for row in 0..ch.min(eh / 2) {
        let at = ew * eh + row * ew;
        let src = row * cw;
        for col in 0..cw.min(ew / 2) {
            nv12[at + col * 2] = u[src + col];
            nv12[at + col * 2 + 1] = v[src + col];
        }
    }
    Ok((nv12, ew as u32, eh as u32))
}

fn rate_from_ticks(ticks: u64) -> u64 {
    let ticks = ticks.max(1);
    let g = gcd(10_000_000, ticks);
    let num = 10_000_000 / g;
    let den = ticks / g;
    if num == 0 || num > u32::MAX as u64 || den > u32::MAX as u64 {
        return (30u64 << 32) | 1;
    }
    (num << 32) | den
}

fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let next = a % b;
        a = b;
        b = next;
    }
    a.max(1)
}

fn fold_stereo(samples: &[i16], channels: u16) -> Vec<i16> {
    let channels = channels as usize;
    let frames = samples.len() / channels;
    let mut out = Vec::with_capacity(frames * 2);
    for frame in 0..frames {
        let base = frame * channels;
        out.push(samples[base]);
        out.push(samples[base + 1]);
    }
    out
}

fn resample_pcm(samples: &[i16], channels: u16, from: u32, to: u32) -> Vec<i16> {
    if from == 0 || from == to {
        return samples.to_vec();
    }
    let ch = channels as usize;
    let frames = samples.len() / ch.max(1);
    if frames == 0 {
        return Vec::new();
    }
    let out_frames = ((frames as u64 * to as u64) / from as u64).max(1) as usize;
    let mut out = Vec::with_capacity(out_frames * ch);
    for frame in 0..out_frames {
        let src = ((frame as u64 * from as u64) / to as u64) as usize;
        let src = src.min(frames - 1);
        out.extend_from_slice(&samples[src * ch..(src + 1) * ch]);
    }
    out
}

fn ensure_runtime() -> Result<(), String> {
    static READY: OnceLock<Result<(), String>> = OnceLock::new();
    READY
        .get_or_init(|| unsafe {
            if CoInitializeEx(None, COINIT_MULTITHREADED).is_err() {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            }
            MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET).map_err(|e| format!("video di Windows non pronto: {e}"))
        })
        .clone()
}

fn wide(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
}
