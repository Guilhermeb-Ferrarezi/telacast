//! Conexão WebSocket com o gateway, numa thread própria.
use eframe::egui::Context;
use std::{
    sync::{
        mpsc::{channel, Receiver, Sender, TryRecvError},
        Arc, Mutex,
    },
    time::Duration,
};
use telacast_proto::{C2S, S2C};
use tungstenite::Message;

pub enum Event {
    Connected,
    Closed(String),
    Msg(S2C),
}

/// Quem recebe, direto na thread da rede, os quadros binários (áudio da voz). Não depende de a janela repintar.
pub type BinarySink = Box<dyn Fn(&[u8]) + Send + Sync>;

pub struct Gateway {
    pub tx: Sender<C2S>,
    pub rx: Receiver<Event>,
    /// Quadros binários a enviar (áudio). Têm prioridade sobre as mensagens JSON.
    pub bin_tx: Sender<Vec<u8>>,
    /// Destino dos quadros binários recebidos; troque ao abrir/fechar a voz.
    pub sink: Arc<Mutex<Option<BinarySink>>>,
}

impl Gateway {
    pub fn send(&self, m: C2S) {
        let _ = self.tx.send(m);
    }

    pub fn send_binary(&self, frame: Vec<u8>) {
        let _ = self.bin_tx.send(frame);
    }

    pub fn set_sink(&self, sink: Option<BinarySink>) {
        if let Ok(mut g) = self.sink.lock() {
            *g = sink;
        }
    }
}

pub fn open(server: &str, ctx: Context) -> Gateway {
    let url = format!("{}/ws", server.trim_end_matches('/'));
    let (out_tx, out_rx) = channel::<C2S>();
    let (ev_tx, ev_rx) = channel::<Event>();
    let (bin_tx, bin_rx) = channel::<Vec<u8>>();
    let sink: Arc<Mutex<Option<BinarySink>>> = Arc::new(Mutex::new(None));
    let sink2 = sink.clone();
    std::thread::spawn(move || {
        let end = match run(&url, out_rx, bin_rx, &sink2, &ev_tx, &ctx) {
            Ok(()) => "Desconectado".to_string(),
            Err(e) => format!("Sem conexão com o servidor ({e})"),
        };
        let _ = ev_tx.send(Event::Closed(end));
        ctx.request_repaint();
    });
    Gateway { tx: out_tx, rx: ev_rx, bin_tx, sink }
}

fn run(url: &str, out: Receiver<C2S>, bin_out: Receiver<Vec<u8>>, sink: &Arc<Mutex<Option<BinarySink>>>, ev: &Sender<Event>, ctx: &Context) -> anyhow::Result<()> {
    let mut ws = crate::net::connect_ws(url)?;
    // leitura curta: os quadros de áudio a enviar esperam no máximo isto (20 ms de áudio por quadro)
    crate::net::set_read_timeout(&mut ws, Duration::from_millis(8))?;
    let _ = ev.send(Event::Connected);
    ctx.request_repaint();
    // Relógio de parede: depois de o PC suspender, o salto de tempo derruba a conexão velha na hora.
    let mut last_rx = std::time::SystemTime::now();
    loop {
        if last_rx.elapsed().map_or(true, |d| d > Duration::from_secs(50)) {
            anyhow::bail!("o servidor parou de responder");
        }
        while let Ok(frame) = bin_out.try_recv() {
            ws.send(Message::Binary(frame.into()))?;
        }
        loop {
            match out.try_recv() {
                Ok(m) => ws.send(Message::Text(serde_json::to_string(&m)?.into()))?,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    let _ = ws.close(None);
                    return Ok(());
                }
            }
        }
        match ws.read() {
            Ok(Message::Text(t)) => {
                last_rx = std::time::SystemTime::now();
                if let Ok(m) = serde_json::from_str::<S2C>(&t) {
                    let _ = ev.send(Event::Msg(m));
                    ctx.request_repaint();
                }
            }
            Ok(Message::Binary(b)) => {
                last_rx = std::time::SystemTime::now();
                if let Ok(g) = sink.lock() {
                    if let Some(f) = g.as_ref() {
                        f(&b);
                    }
                }
            }
            Ok(Message::Close(_)) => return Ok(()),
            // ping do servidor (o tungstenite já responde com pong) e pong também provam que a conexão vive
            Ok(_) => last_rx = std::time::SystemTime::now(),
            Err(tungstenite::Error::Io(e))
                if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
            Err(e) => return Err(e.into()),
        }
    }
}
