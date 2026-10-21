use std::path::Path;
use std::sync::OnceLock;

use windows::Win32::Media::MediaFoundation::{
    MFCreateMediaType, MFCreateSinkWriterFromURL, MFCreateSourceReaderFromURL, MFStartup, MF_MT_AVG_BITRATE,
    MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE, MF_MT_PIXEL_ASPECT_RATIO,
    MF_MT_SUBTYPE, MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED, MF_SOURCE_READERF_ENDOFSTREAM,
    MF_SOURCE_READERF_STREAMTICK, MF_SOURCE_READER_FIRST_AUDIO_STREAM, MF_SOURCE_READER_FIRST_VIDEO_STREAM,
    MF_VERSION, MFAudioFormat_AAC, MFAudioFormat_PCM, MFMediaType_Audio, MFMediaType_Video, MFSTARTUP_NOSOCKET,
    MFVideoFormat_H264, MFVideoFormat_NV12, MFVideoInterlace_Progressive,
};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED};
use windows::core::PCWSTR;

pub fn to_mp4(input: &Path, output: &Path) -> Result<(), String> {
    ensure_runtime()?;
    let wide_in = wide(input);
    let wide_out = wide(output);
    unsafe {
        let reader = MFCreateSourceReaderFromURL(PCWSTR(wide_in.as_ptr()), None)
            .map_err(|e| format!("video non aperto: {e}"))?;
        let video_stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
        let audio_stream = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
        let native = reader
            .GetNativeMediaType(video_stream, 0)
            .map_err(|_| "il file non ha un video".to_string())?;
        let frame = native.GetUINT64(&MF_MT_FRAME_SIZE).unwrap_or((640 << 32) | 480);
        let rate = native.GetUINT64(&MF_MT_FRAME_RATE).unwrap_or((30 << 32) | 1);
        let width = (frame >> 32) as u32;
        let height = frame as u32;
        if width == 0 || height == 0 {
            return Err("il video non ha una dimensione".into());
        }

        let uncompressed = MFCreateMediaType().map_err(|e| e.to_string())?;
        uncompressed.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(|e| e.to_string())?;
        uncompressed.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12).map_err(|e| e.to_string())?;
        reader
            .SetCurrentMediaType(video_stream, None, &uncompressed)
            .map_err(|_| "questo video non si può decomprimere".to_string())?;
        let video_in = reader
            .GetCurrentMediaType(video_stream)
            .map_err(|e| e.to_string())?;

        let mut audio_in = None;
        let has_audio = reader.GetNativeMediaType(audio_stream, 0).is_ok();
        if has_audio {
            let pcm = MFCreateMediaType().map_err(|e| e.to_string())?;
            pcm.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio).map_err(|e| e.to_string())?;
            pcm.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM).map_err(|e| e.to_string())?;
            if reader.SetCurrentMediaType(audio_stream, None, &pcm).is_ok() {
                audio_in = reader.GetCurrentMediaType(audio_stream).ok();
            }
        }

        let writer = MFCreateSinkWriterFromURL(PCWSTR(wide_out.as_ptr()), None, None)
            .map_err(|e| format!("MP4 non creato: {e}"))?;
        let video_out = MFCreateMediaType().map_err(|e| e.to_string())?;
        video_out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(|e| e.to_string())?;
        video_out.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264).map_err(|e| e.to_string())?;
        video_out.SetUINT64(&MF_MT_FRAME_SIZE, frame).map_err(|e| e.to_string())?;
        video_out.SetUINT64(&MF_MT_FRAME_RATE, rate).map_err(|e| e.to_string())?;
        video_out.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1).map_err(|e| e.to_string())?;
        video_out
            .SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
            .map_err(|e| e.to_string())?;
        let pixels = (width as u64).saturating_mul(height as u64).max(1);
        let bitrate = (pixels.saturating_mul(4)).clamp(400_000, 8_000_000) as u32;
        video_out.SetUINT32(&MF_MT_AVG_BITRATE, bitrate).map_err(|e| e.to_string())?;
        let video_index = writer
            .AddStream(&video_out)
            .map_err(|e| format!("H.264 non disponibile: {e}"))?;
        writer.SetInputMediaType(video_index, &video_in, None).map_err(|e| e.to_string())?;

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
        pump(&reader, &writer, video_stream, video_index)?;
        if let Some(index) = audio_index {
            pump(&reader, &writer, audio_stream, index)?;
        }
        writer.Finalize().map_err(|e| format!("MP4 non chiuso: {e}"))?;
    }
    Ok(())
}

fn pump(
    reader: &windows::Win32::Media::MediaFoundation::IMFSourceReader,
    writer: &windows::Win32::Media::MediaFoundation::IMFSinkWriter,
    source: u32,
    dest: u32,
) -> Result<(), String> {
    unsafe {
        loop {
            let mut flags: u32 = 0;
            let mut sample = None;
            reader
                .ReadSample(source, 0, None, Some(&mut flags), None, Some(&mut sample))
                .map_err(|e| format!("lettura video: {e}"))?;
            if flags & (MF_SOURCE_READERF_ENDOFSTREAM.0 as u32) != 0 {
                break;
            }
            if flags & ((MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 as u32) | (MF_SOURCE_READERF_STREAMTICK.0 as u32)) != 0
                && sample.is_none()
            {
                continue;
            }
            if let Some(sample) = sample {
                writer.WriteSample(dest, &sample).map_err(|e| format!("scrittura video: {e}"))?;
            }
        }
    }
    Ok(())
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
