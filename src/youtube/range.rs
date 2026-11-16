//! Lettura Maven Range: ogni richiesta aperta termina entro due secondi, poi ricontrolla cancel.
//! Il resolver DNS nativo di ureq non è interrompibile: il limite riguarda la connessione aperta.
use std::io::{Read,Seek,SeekFrom};
use std::sync::atomic::{AtomicBool,Ordering};
use std::time::Duration;

fn agent()->ureq::Agent{
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(2)).timeout(Duration::from_secs(2))
        .user_agent(&format!("cedro/{}",crate::update::CURRENT)).build()
}

fn attendi(cancel:&AtomicBool,failures:u32)->Result<(),String>{
    for _ in 0..(failures*10).min(100){
        if cancel.load(Ordering::Acquire){return Err("Annullato".into());}
        std::thread::sleep(Duration::from_millis(100));
    }
    if cancel.load(Ordering::Acquire){return Err("Annullato".into());}
    Ok(())
}

/// Un file remoto letto a pezzi con le richieste «Range»: dello zip si scarica solo quello che serve.
pub(super) struct Remote<'a> {
    cancel:&'a AtomicBool,
    url: String,
    len: u64,
    pos: u64,
    buf: Vec<u8>,
    buf_at: u64,
}

const CHUNK: u64 = 4 * 1024 * 1024;

impl<'a> Remote<'a> {
    pub(super) fn open(url: &str,cancel:&'a AtomicBool) -> Result<Remote<'a>, String> {
        let mut failures=0;
        let r=loop {
            if cancel.load(Ordering::Acquire){return Err("Annullato".into());}
            let result=agent().head(url).call();
            if cancel.load(Ordering::Acquire){return Err("Annullato".into());}
            match result {
                Ok(r)=>break r,
                Err(e)=>{failures+=1;if failures>8{return Err(e.to_string());}attendi(cancel,failures)?;}
            }
        };
        let len = r.header("Content-Length").and_then(|v| v.parse().ok()).ok_or("dimensione sconosciuta")?;
        Ok(Remote { cancel,url: url.to_string(), len, pos: 0, buf: Vec::new(), buf_at: 0 })
    }
}

impl Read for Remote<'_> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.len || out.is_empty() {
            return Ok(0);
        }
        let inside = self.pos >= self.buf_at && self.pos < self.buf_at + self.buf.len() as u64;
        if !inside {
            let end = (self.pos + CHUNK.max(out.len() as u64)).min(self.len) - 1;
            let mut buf=Vec::with_capacity((end+1-self.pos) as usize);
            let mut failures=0;
            while (buf.len() as u64)<end+1-self.pos {
                if self.cancel.load(Ordering::Relaxed){return Err(std::io::Error::other("Annullato"));}
                let from=self.pos+buf.len() as u64;
                let range=format!("bytes={from}-{end}");
                let attempt=(||->Result<(),String>{
                    let r=agent().get(&self.url).set("Range",&range).call().map_err(|e|e.to_string())?;
                    let expected=format!("bytes {from}-{end}/{}",self.len);
                    if r.status()!=206||r.header("Content-Range")!=Some(expected.as_str()){return Err("Intervallo del download non valido".into());}
                    let mut reader=r.into_reader();let mut chunk=[0u8;65536];
                    while (buf.len() as u64)<end+1-self.pos {
                        if self.cancel.load(Ordering::Relaxed){return Err("Annullato".into());}
                        crate::componenti::continua_download("Componenti")?;
                        let max=chunk.len().min((end+1-self.pos-buf.len() as u64) as usize);
                        let n=reader.read(&mut chunk[..max]).map_err(|e|e.to_string())?;
                        if n==0{return Err("Risposta incompleta".into());}
                        buf.extend_from_slice(&chunk[..n]);
                    }
                    Ok(())
                })();
                if let Err(e)=attempt {
                    if self.cancel.load(Ordering::Acquire){return Err(std::io::Error::other("Annullato"));}
                    // Il timeout limita ogni attesa, non il numero di segmenti: una rete
                    // lenta che avanza non consuma il budget degli errori senza progresso.
                    if self.pos+buf.len() as u64>from{failures=0;}
                    failures+=1;
                    if failures>8{return Err(std::io::Error::other(e));}
                    attendi(self.cancel,failures).map_err(std::io::Error::other)?;
                }
            }
            self.buf = buf;
            self.buf_at = self.pos;
        }
        let from = (self.pos - self.buf_at) as usize;
        let n = out.len().min(self.buf.len() - from);
        out[..n].copy_from_slice(&self.buf[from..from + n]);
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for Remote<'_> {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        let next = match to {
            SeekFrom::Start(n) => n as i64,
            SeekFrom::End(n) => self.len as i64 + n,
            SeekFrom::Current(n) => self.pos as i64 + n,
        };
        if next < 0 {
            return Err(std::io::Error::other("posizione prima dell'inizio"));
        }
        self.pos = next as u64;
        Ok(self.pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead,BufReader,Write};
    use std::net::{TcpListener,TcpStream};
    use std::sync::{Arc,mpsc};
    use std::time::Instant;

    fn request(listener:&TcpListener)->(TcpStream,String){
        listener.set_nonblocking(true).unwrap();
        let start=Instant::now();
        let stream=loop {match listener.accept(){
            Ok((stream,_))=>break stream,
            Err(e) if e.kind()==std::io::ErrorKind::WouldBlock&&start.elapsed()<Duration::from_secs(5)=>std::thread::sleep(Duration::from_millis(10)),
            Err(e)=>panic!("connessione attesa: {e}"),
        }};
        stream.set_nonblocking(false).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut reader=BufReader::new(stream);let mut headers=String::new();
        loop {let mut line=String::new();assert!(reader.read_line(&mut line).unwrap()>0);headers.push_str(&line);if line=="\r\n"{break;}}
        (reader.into_inner(),headers)
    }

    #[derive(Clone,Copy,Debug,PartialEq)]
    enum Stallo {Head,Headers,Body}
    fn cancellazione(stallo:Stallo){
        let listener=TcpListener::bind("127.0.0.1:0").unwrap();
        let url=format!("http://{}/component.aar",listener.local_addr().unwrap());
        let (opened_tx,opened_rx)=mpsc::channel();let (release_tx,release_rx)=mpsc::channel();
        let server=std::thread::spawn(move||{
            let (mut stream,headers)=request(&listener);assert!(headers.starts_with("HEAD "));
            if stallo!=Stallo::Head {
                stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\n").unwrap();drop(stream);
                let (next,headers)=request(&listener);stream=next;assert!(headers.starts_with("GET "));
                if stallo==Stallo::Body {stream.write_all(b"HTTP/1.1 206 Partial Content\r\nContent-Length: 10\r\nContent-Range: bytes 0-9/10\r\nConnection: close\r\n\r\na").unwrap();}
            }
            opened_tx.send(()).unwrap();
            // La connessione rimane aperta fino al risultato: non è EOF a sbloccare il client.
            let _=release_rx.recv_timeout(Duration::from_secs(6));drop(stream);
        });
        let cancel=Arc::new(AtomicBool::new(false));let token=cancel.clone();let (done_tx,done_rx)=mpsc::channel();
        let client=std::thread::spawn(move||{
            let result=Remote::open(&url,&token).and_then(|mut remote|{let mut data=Vec::new();remote.read_to_end(&mut data).map(|_|()).map_err(|e|e.to_string())});
            done_tx.send(result).unwrap();
        });
        opened_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        let start=Instant::now();cancel.store(true,Ordering::Release);
        let result=done_rx.recv_timeout(Duration::from_secs(5));let elapsed=start.elapsed();
        let _=release_tx.send(());server.join().unwrap();client.join().unwrap();
        assert!(elapsed<Duration::from_secs(5),"{stallo:?}: {elapsed:?}");
        assert_eq!(result.unwrap().unwrap_err(),"Annullato","{stallo:?}");
    }

    #[test]
    fn maven_annulla_head_senza_risposta(){cancellazione(Stallo::Head);}
    #[test]
    fn maven_annulla_get_senza_headers(){cancellazione(Stallo::Headers);}
    #[test]
    fn maven_annulla_body_fermo(){cancellazione(Stallo::Body);}

    #[test]
    fn maven_range_riprende_byte_e_non_esaurisce_retry_se_avanza(){
        let listener=TcpListener::bind("127.0.0.1:0").unwrap();
        let url=format!("http://{}/component.aar",listener.local_addr().unwrap());
        let bytes=b"0123456789";
        let server=std::thread::spawn(move||{
            let (mut stream,headers)=request(&listener);assert!(headers.starts_with("HEAD "));
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\n").unwrap();drop(stream);
            let mut ranges=Vec::new();
            // Nove interruzioni con progresso superano il limite degli errori senza progresso.
            for from in 0..bytes.len(){
                let (mut stream,headers)=request(&listener);
                let range=headers.lines().find(|l|l.to_ascii_lowercase().starts_with("range:")).unwrap().to_string();
                assert_eq!(range.to_ascii_lowercase(),format!("range: bytes={from}-9"));ranges.push(from);
                write!(stream,"HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {from}-9/10\r\nConnection: close\r\n\r\n",10-from).unwrap();
                stream.write_all(&bytes[from..from+1]).unwrap();
                // EOF tronca la risposta: al prossimo tentativo il prefisso non va riscaricato.
            }
            ranges
        });
        let cancel=AtomicBool::new(false);let mut remote=Remote::open(&url,&cancel).unwrap();
        let mut data=Vec::new();let result=remote.read_to_end(&mut data);
        let ranges=server.join().unwrap();result.unwrap();assert_eq!(data,bytes);assert_eq!(ranges,(0..10).collect::<Vec<_>>());
        remote.seek(SeekFrom::Start(3)).unwrap();let mut slice=[0;3];remote.read_exact(&mut slice).unwrap();assert_eq!(&slice,b"345");
    }
}
