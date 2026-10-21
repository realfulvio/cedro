use std::path::Path;

pub(crate) trait VideoSink: Sized {
    fn open(
        output: &Path,
        width: u32,
        height: u32,
        frame_duration: i64,
        audio: Option<(u32, u16)>,
    ) -> Result<Self, String>;

    fn write_yuv(
        &mut self,
        y: &[u8],
        u: &[u8],
        v: &[u8],
        width: u32,
        height: u32,
        time: i64,
        duration: i64,
    ) -> Result<(), String>;

    fn write_pcm(&mut self, samples: &[i16], channels: u16, rate: u32) -> Result<(), String>;

    fn finish(self) -> Result<(), String>;
}
