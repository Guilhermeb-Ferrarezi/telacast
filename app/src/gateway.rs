//! Conexão WebSocket com o gateway, numa thread própria.
use eframe::egui::Context;
use std::{
    sync::mpsc::{channel, Receiver, Sender, TryRecvError},
    time::Duration,
};
use telacast_proto::{C2S, S2C};
use tungstenite::{connect, stream::MaybeTlsStream, Message};

pub enum Event {
    Connected,
    Closed(String),
    Msg(S2C),
}

pub struct Gateway {
    pub tx: Sender<C2S>,
    pub rx: Receiver<Event>,
}

impl Gateway {
    pub fn send(&self, m: C2S) {
        let _ = self.tx.send(m);
    }
}

pub fn open(server: &str, ctx: Context) -> Gateway {
    let url = format!("{}/ws", server.trim_end_matches('/'));
    let (out_tx, out_rx) = channel::<C2S>();
    let (ev_tx, ev_rx) = channel::<Event>();
    std::thread::spawn(move || {
        let end = match run(&url, out_rx, &ev_tx, &ctx) {
            Ok(()) => "Desconectado".to_string(),
            Err(e) => format!("Sem conexão com o servidor ({e})"),
        };
        let _ = ev_tx.send(Event::Closed(end));
        ctx.request_repaint();
    });
    Gateway { tx: out_tx, rx: ev_rx }
}

fn run(url: &str, out: Receiver<C2S>, ev: &Sender<Event>, ctx: &Context) -> anyhow::Result<()> {
    let (mut ws, _) = connect(url)?;
    if let MaybeTlsStream::Plain(s) = ws.get_mut() {
        s.set_read_timeout(Some(Duration::from_millis(50)))?;
    }
    let _ = ev.send(Event::Connected);
    ctx.request_repaint();
    loop {
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
                if let Ok(m) = serde_json::from_str::<S2C>(&t) {
                    let _ = ev.send(Event::Msg(m));
                    ctx.request_repaint();
                }
            }
            Ok(Message::Close(_)) => return Ok(()),
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
            Err(e) => return Err(e.into()),
        }
    }
}
