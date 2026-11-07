//! Instância única: só um Telacast por pasta de dados.
//!
//! A primeira instância segura uma trava de arquivo (`instance.lock`) e escuta num socket Unix
//! abstrato. Quem abre depois não cria janela: pede à primeira que venha para a frente
//! e sai. A trava some sozinha quando o processo morre (mesmo se travar), então não sobra
//! "lixo" que impeça abrir de novo.

use std::{
    fs::File,
    io::Write,
    hash::{Hash, Hasher},
    os::linux::net::SocketAddrExt,
    os::unix::net::{SocketAddr, UnixListener, UnixStream},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        OnceLock,
    },
};

/// Mesma pasta onde o eframe guarda `app.ron`.
fn dir() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("telacast")
}

/// Socket "abstrato" do Linux (sem arquivo, some com o processo; o caminho de um socket comum
/// estoura o limite de ~108 bytes em pastas fundas). O nome depende da pasta de dados.
fn addr(dir: &std::path::Path) -> Option<SocketAddr> {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    dir.hash(&mut h);
    SocketAddr::from_abstract_name(format!("telacast-instance-{:016x}", h.finish())).ok()
}

struct Primary {
    _lock: File,
    listener: Option<UnixListener>,
}

static PRIMARY: OnceLock<Primary> = OnceLock::new();
static RAISE: AtomicBool = AtomicBool::new(false);

/// `true` se esta é a instância principal e pode abrir a janela. `false`: já havia outra,
/// que foi chamada para a frente.
pub fn acquire() -> bool {
    let d = dir();
    if std::fs::create_dir_all(&d).is_err() {
        return true; // sem pasta gravável não dá para travar; abre mesmo assim
    }
    let Ok(lock) = File::options().create(true).truncate(false).write(true).open(d.join("instance.lock")) else {
        return true;
    };
    if lock.try_lock().is_err() {
        if let Some(Ok(mut s)) = addr(&d).map(|a| UnixStream::connect_addr(&a)) {
            let _ = s.write_all(b"raise\n");
        }
        eprintln!("O Telacast já está aberto.");
        return false;
    }
    let listener = addr(&d).and_then(|a| UnixListener::bind_addr(&a).ok());
    let _ = PRIMARY.set(Primary { _lock: lock, listener });
    true
}

/// Começa a ouvir os pedidos de outras instâncias (chamar uma vez, com a janela já criada).
pub fn serve(ctx: egui::Context) {
    let Some(p) = PRIMARY.get() else { return };
    let Some(Ok(l)) = p.listener.as_ref().map(|l| l.try_clone()) else { return };
    std::thread::spawn(move || {
        for conn in l.incoming() {
            if conn.is_ok() {
                RAISE.store(true, Ordering::Relaxed);
                ctx.request_repaint();
            }
        }
    });
}

/// `true` uma vez por pedido de outra instância.
pub fn take_raise() -> bool {
    RAISE.swap(false, Ordering::Relaxed)
}
