//! Stampa quello che la scheda «Sistema» delle Impostazioni mostra: serve alle prove sui PC nuovi.
//! Uso: cargo run --release --example sistema
fn main() {
    print!("{}", cedro::sysinfo::report());
    let hw = cedro::accel::hardware();
    println!("Hardware: gpu={:?} istruzioni avanzate={} dotprod={}", hw.gpu, hw.avx512, hw.dotprod);
}
