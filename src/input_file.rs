//! File in ingresso: i grant SAF si leggono dal descriptor, con cursori indipendenti.
//! I normali file desktop mantengono le operazioni di std::fs::File.
use std::fs::{File as StdFile, Metadata};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
#[cfg(unix)]
use std::sync::{Arc, Mutex, OnceLock};

#[cfg(unix)]
static GRANTS: OnceLock<Mutex<std::collections::HashMap<i32,Arc<StdFile>>>> = OnceLock::new();

/// Il chiamante conserva una lease autorizzata; `alias_fd` è il numero nel symlink privato.
#[cfg(unix)]
pub fn register(alias_fd:i32,file:Arc<StdFile>) {
    GRANTS.get_or_init(Default::default).lock().unwrap_or_else(|e|e.into_inner()).insert(alias_fd,file);
}

pub struct File {
    inner: Source,
}
enum Source {
    Regular(StdFile),
    #[cfg(unix)]
    Granted {file:Arc<StdFile>,cursor:u64},
}
impl From<StdFile> for File {
    fn from(file:StdFile)->Self {Self{inner:Source::Regular(file)}}
}
impl File {
    pub fn open(path:impl AsRef<Path>)->io::Result<Self> {
        let path=path.as_ref();
        #[cfg(unix)]
        if let Some(grants)=GRANTS.get() {
            // Read only the explicit private alias, never canonicalize the provider pathname.
            if let Ok(target)=std::fs::read_link(path) {
                if let Some(fd)=target.to_str().and_then(|s|s.strip_prefix("/proc/self/fd/")).and_then(|s|s.parse::<i32>().ok()) {
                    if let Some(file)=grants.lock().unwrap_or_else(|e|e.into_inner()).get(&fd).cloned() {
                        return Ok(Self{inner:Source::Granted{file,cursor:0}});
                    }
                }
            }
        }
        StdFile::open(path).map(Self::from)
    }
    pub fn create(path:impl AsRef<Path>)->io::Result<Self> {StdFile::create(path).map(Self::from)}
    pub fn metadata(&self)->io::Result<Metadata> {
        match &self.inner {Source::Regular(f)=>f.metadata(),#[cfg(unix)] Source::Granted{file,..}=>file.metadata()}
    }
}
impl Read for File {
    fn read(&mut self,buf:&mut [u8])->io::Result<usize> {
        match &mut self.inner {
            Source::Regular(f)=>f.read(buf),
            #[cfg(unix)]
            Source::Granted{file,cursor}=>{
                use std::os::unix::fs::FileExt;
                let n=file.read_at(buf,*cursor)?;
                *cursor=cursor.checked_add(n as u64).ok_or_else(||io::Error::new(io::ErrorKind::InvalidInput,"file offset overflow"))?;
                Ok(n)
            }
        }
    }
}
impl Seek for File {
    fn seek(&mut self,pos:SeekFrom)->io::Result<u64> {
        match &mut self.inner {
            Source::Regular(f)=>f.seek(pos),
            #[cfg(unix)]
            Source::Granted{file,cursor}=>{
                let next=match pos {
                    SeekFrom::Start(n)=>Some(n),
                    SeekFrom::Current(n)=>cursor.checked_add_signed(n),
                    SeekFrom::End(n)=>file.metadata()?.len().checked_add_signed(n),
                }.ok_or_else(||io::Error::new(io::ErrorKind::InvalidInput,"invalid file offset"))?;
                *cursor=next;Ok(next)
            }
        }
    }
}
impl Write for File {
    fn write(&mut self,buf:&[u8])->io::Result<usize> {
        match &mut self.inner {Source::Regular(f)=>f.write(buf),#[cfg(unix)] Source::Granted{..}=>Err(io::Error::new(io::ErrorKind::PermissionDenied,"read-only SAF grant"))}
    }
    fn flush(&mut self)->io::Result<()> {
        match &mut self.inner {Source::Regular(f)=>f.flush(),#[cfg(unix)] Source::Granted{..}=>Ok(())}
    }
}

/// Testi ausiliari (es. MIRAX INI) usano lo stesso ingresso dei blocchi binari.
pub fn read(path:impl AsRef<Path>)->io::Result<Vec<u8>> {
    let mut bytes=Vec::new();File::open(path)?.read_to_end(&mut bytes)?;Ok(bytes)
}

/// Copia richiesta dall'operazione (es. copia anonima), mai materializzazione automatica SAF.
pub fn copy(from:impl AsRef<Path>,to:impl AsRef<Path>)->io::Result<u64> {
    let mut source=File::open(from.as_ref())?;
    match &source.inner {
        Source::Regular(_)=>std::fs::copy(from,to),
        #[cfg(unix)]
        Source::Granted{..}=>io::copy(&mut source,&mut StdFile::create(to)?),
    }
}
impl symphonia::core::io::MediaSource for File {
    fn is_seekable(&self)->bool {self.metadata().is_ok_and(|m|m.is_file())}
    fn byte_len(&self)->Option<u64> {self.metadata().ok().map(|m|m.len())}
}

#[cfg(all(test,unix))]
mod tests {
    use super::*;
    use std::os::fd::AsRawFd;
    #[test]
    fn grant_readers_have_independent_positions_in_parallel() {
        let dir=std::env::temp_dir().join(format!("cedro-grant-{}",std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let data=dir.join("data");std::fs::write(&data,b"0123456789abcdef").unwrap();
        let mut original=StdFile::open(&data).unwrap();original.seek(SeekFrom::Start(7)).unwrap();
        let original=Arc::new(original);let fd=original.as_raw_fd();register(fd,original.clone());
        let alias=dir.join("slide.ndpi");std::os::unix::fs::symlink(format!("/proc/self/fd/{fd}"),&alias).unwrap();
        std::thread::scope(|scope| {
            for offset in 0..12u64 {let path=&alias;scope.spawn(move || {
                let mut f=File::open(path).unwrap();
                for _ in 0..100 {
                    f.seek(SeekFrom::Start(offset)).unwrap();let mut b=[0;4];f.read_exact(&mut b).unwrap();
                    assert_eq!(&b,&b"0123456789abcdef"[offset as usize..offset as usize+4]);
                }
                assert!(f.seek(SeekFrom::End(-17)).is_err());
                assert_eq!(f.seek(SeekFrom::End(-1)).unwrap(),15);
                assert!(f.write_all(b"x").is_err());
            });}
        });
        assert_eq!((&*original).stream_position().unwrap(),7);
        GRANTS.get().unwrap().lock().unwrap().remove(&fd);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
