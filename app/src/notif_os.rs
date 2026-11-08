//! Notificações do sistema: a parte que fala com o sistema operacional.
//!
//! - **Linux:** D-Bus, `org.freedesktop.Notifications` (a interface que GNOME, KDE, dunst, mako, xfce4-notifyd...
//!   implementam), direto pelo `zbus` — que o eframe já traz (`accesskit_unix`), então não entra crate novo.
//!   Uma thread própria (criada só na primeira notificação) manda `Notify` e escuta `ActionInvoked`
//!   (clique no corpo da notificação = ação `default`) e `NotificationClosed`.
//! - **Windows:** toast do WinRT (`Windows.UI.Notifications`) pelo crate `windows`, que também já está na árvore
//!   (wgpu, accesskit). O clique chega no evento `Activated`, que o Windows dispara numa thread dele.
//! - Outros sistemas: não faz nada.
//!
//! Falha ao notificar nunca derruba o app: o erro vai para o stderr (uma vez) e o app segue.

use crate::notif::{Notice, Target};
use std::sync::mpsc::{channel, Receiver, Sender};

/// Quem entrega as notificações e devolve os cliques (como [`Target`]).
pub struct Notifier {
    ctx: egui::Context,
    clicks_tx: Sender<Target>,
    clicks_rx: Receiver<Target>,
    os: imp::Os,
}

impl Notifier {
    pub fn new(ctx: egui::Context) -> Self {
        let (clicks_tx, clicks_rx) = channel();
        Self { ctx, clicks_tx, clicks_rx, os: imp::Os::default() }
    }

    /// Mostra (ou substitui, se for do mesmo canal) uma notificação.
    pub fn show(&mut self, n: &Notice) {
        self.os.show(n, &self.ctx, &self.clicks_tx);
    }

    /// Cliques que chegaram desde a última chamada.
    pub fn clicks(&self) -> Vec<Target> {
        self.clicks_rx.try_iter().collect()
    }
}

/// Grava o ícone do app (embutido no binário) numa pasta de cache e devolve o caminho.
/// O servidor de notificações lê o arquivo por conta própria, então precisa existir em disco.
fn icon_file() -> Option<std::path::PathBuf> {
    static ICON: &[u8] = include_bytes!("../assets/icon.png");
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from)?;
    #[cfg(not(windows))]
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".cache")))?;
    let dir = base.join("telacast");
    let path = dir.join("icon.png");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() == ICON.len() as u64) {
        return Some(path);
    }
    std::fs::create_dir_all(&dir).ok()?;
    std::fs::write(&path, ICON).ok()?;
    Some(path)
}

// ------------------------------------------------------------------ Linux

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use crate::notif::{escape_markup, Key};
    use std::collections::HashMap;
    use std::time::{Duration, Instant};
    use zbus::zvariant::Value;

    #[zbus::proxy(
        interface = "org.freedesktop.Notifications",
        default_service = "org.freedesktop.Notifications",
        default_path = "/org/freedesktop/Notifications"
    )]
    trait Notifications {
        #[allow(clippy::too_many_arguments)]
        fn notify(
            &self,
            app_name: &str,
            replaces_id: u32,
            app_icon: &str,
            summary: &str,
            body: &str,
            actions: &[&str],
            hints: HashMap<&str, Value<'_>>,
            expire_timeout: i32,
        ) -> zbus::Result<u32>;
        fn get_capabilities(&self) -> zbus::Result<Vec<String>>;
        #[zbus(signal)]
        fn action_invoked(&self, id: u32, action_key: String) -> zbus::Result<()>;
        #[zbus(signal)]
        fn notification_closed(&self, id: u32, reason: u32) -> zbus::Result<()>;
    }

    enum Ev {
        Cmd(Result<Notice, async_channel::RecvError>),
        Action(Option<(u32, String)>),
        Closed(Option<u32>),
    }

    #[derive(Default)]
    pub struct Os {
        tx: Option<async_channel::Sender<Notice>>,
        /// Depois de uma falha (sem D-Bus, sem servidor de notificação), só tenta de novo após este instante.
        retry_at: Option<Instant>,
    }

    impl Os {
        pub fn show(&mut self, n: &Notice, ctx: &egui::Context, clicks: &Sender<Target>) {
            if self.retry_at.is_some_and(|t| Instant::now() < t) {
                return;
            }
            if self.tx.is_none() {
                let (tx, rx) = async_channel::unbounded();
                let (clicks, ctx) = (clicks.clone(), ctx.clone());
                let spawned = std::thread::Builder::new().name("notif".into()).spawn(move || {
                    if let Err(e) = futures_lite::future::block_on(run(rx, clicks, ctx)) {
                        eprintln!("notificações: {e}");
                    }
                });
                match spawned {
                    Ok(_) => self.tx = Some(tx),
                    Err(e) => {
                        eprintln!("notificações: não consegui criar a thread: {e}");
                        self.retry_at = Some(Instant::now() + Duration::from_secs(60));
                        return;
                    }
                }
            }
            let sent = self.tx.as_ref().is_some_and(|tx| tx.try_send(n.clone()).is_ok());
            if !sent {
                // a thread terminou (erro de D-Bus): descarta e tenta de novo em 1 minuto
                self.tx = None;
                self.retry_at = Some(Instant::now() + Duration::from_secs(60));
            }
        }
    }

    async fn run(rx: async_channel::Receiver<Notice>, clicks: Sender<Target>, ctx: egui::Context) -> zbus::Result<()> {
        use futures_lite::{future::or, StreamExt};
        let conn = zbus::Connection::session().await?;
        let proxy = NotificationsProxy::new(&conn).await?;
        // sem "body-markup" o corpo é texto puro: escapar `<` mostraria "&lt;" literal
        let markup = proxy.get_capabilities().await.map(|c| c.iter().any(|x| x == "body-markup")).unwrap_or(false);
        let mut actions = proxy.receive_action_invoked().await?;
        let mut closed = proxy.receive_notification_closed().await?;
        let icon = super::icon_file().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        let mut targets: HashMap<u32, Target> = HashMap::new();
        let mut by_key: HashMap<Key, u32> = HashMap::new();
        loop {
            let ev = or(
                async { Ev::Cmd(rx.recv().await) },
                or(
                    async {
                        Ev::Action(match actions.next().await {
                            Some(s) => s.args().ok().map(|a| (*a.id(), a.action_key().to_string())),
                            None => None,
                        })
                    },
                    async { Ev::Closed(closed.next().await.and_then(|s| s.args().ok().map(|a| *a.id()))) },
                ),
            )
            .await;
            match ev {
                Ev::Cmd(Err(_)) => return Ok(()), // o app fechou o canal
                Ev::Cmd(Ok(n)) => {
                    let body = if markup { escape_markup(&n.body) } else { n.body.clone() };
                    let mut hints: HashMap<&str, Value<'_>> = HashMap::new();
                    hints.insert("urgency", Value::U8(1));
                    hints.insert("category", Value::from("im.received"));
                    let replaces = by_key.get(&n.key).copied().unwrap_or(0);
                    match proxy.notify("Telacast", replaces, &icon, &n.title, &body, &["default", "Abrir"], hints, -1).await {
                        Ok(id) => {
                            if targets.len() > 64 {
                                targets.clear();
                                by_key.clear();
                            }
                            targets.insert(id, n.target);
                            by_key.insert(n.key, id);
                        }
                        Err(e) => eprintln!("notificações: Notify falhou: {e}"),
                    }
                }
                Ev::Action(Some((id, key))) => {
                    if key == "default" {
                        if let Some(t) = targets.get(&id) {
                            let _ = clicks.send(*t);
                            ctx.request_repaint();
                        }
                    }
                }
                Ev::Closed(Some(id)) => {
                    targets.remove(&id);
                    by_key.retain(|_, v| *v != id);
                }
                Ev::Action(None) | Ev::Closed(None) => {}
            }
        }
    }
}

// ------------------------------------------------------------------ Windows

#[cfg(windows)]
mod imp {
    use super::*;
    use crate::notif::Key;
    use windows::{
        core::{w, HSTRING},
        Data::Xml::Dom::XmlDocument,
        Foundation::TypedEventHandler,
        UI::Notifications::{ToastNotification, ToastNotificationManager},
        Win32::System::Registry::{RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ},
    };

    /// Identidade do app nas notificações. Sem registro, o Windows só aceita toast de apps instalados
    /// com atalho; gravar isto em HKCU (por usuário, sem administrador) dá nome e ícone próprios.
    const AUMID: &str = "Telacast";

    #[derive(Default)]
    pub struct Os {
        registered: bool,
        failed: bool,
    }

    fn reg_sz(name: windows::core::PCWSTR, value: &str) {
        let wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: `wide` vive até o fim da chamada e `cbdata` é o tamanho exato em bytes (com o NUL).
        unsafe {
            let _ = RegSetKeyValueW(
                HKEY_CURRENT_USER,
                w!("Software\\Classes\\AppUserModelId\\Telacast"),
                name,
                REG_SZ.0,
                Some(wide.as_ptr() as *const _),
                (wide.len() * 2) as u32,
            );
        }
    }

    impl Os {
        pub fn show(&mut self, n: &Notice, ctx: &egui::Context, clicks: &Sender<Target>) {
            if self.failed {
                return;
            }
            if !self.registered {
                reg_sz(w!("DisplayName"), "Telacast");
                if let Some(p) = super::icon_file() {
                    reg_sz(w!("IconUri"), &p.to_string_lossy());
                }
                self.registered = true;
            }
            if let Err(e) = toast(n, ctx.clone(), clicks.clone()) {
                eprintln!("notificações: toast falhou: {e}");
                // erro de ativação costuma se repetir; não fica enchendo o stderr
                self.failed = true;
            }
        }
    }

    fn toast(n: &Notice, ctx: egui::Context, clicks: Sender<Target>) -> windows::core::Result<()> {
        let xml = XmlDocument::new()?;
        // o texto entra por SetInnerText (o Windows escapa), nunca concatenado no XML
        xml.LoadXml(&HSTRING::from(
            "<toast activationType=\"foreground\"><visual><binding template=\"ToastGeneric\"><text/><text/></binding></visual></toast>",
        ))?;
        let texts = xml.GetElementsByTagName(&HSTRING::from("text"))?;
        texts.Item(0)?.SetInnerText(&HSTRING::from(n.title.as_str()))?;
        texts.Item(1)?.SetInnerText(&HSTRING::from(n.body.as_str()))?;
        let toast = ToastNotification::CreateToastNotification(&xml)?;
        // mesma etiqueta + grupo = a nova substitui a antiga na central de ações
        let tag = match n.key {
            Key::Channel(c) => format!("c{c}"),
            Key::Friends => "amigos".to_string(),
        };
        toast.SetTag(&HSTRING::from(tag))?;
        toast.SetGroup(&HSTRING::from("telacast"))?;
        let target = n.target;
        toast.Activated(&TypedEventHandler::new(move |_, _| {
            let _ = clicks.send(target);
            ctx.request_repaint();
            Ok(())
        }))?;
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(AUMID))?.Show(&toast)
    }
}

// ------------------------------------------------------------------ outros sistemas

#[cfg(not(any(target_os = "linux", windows)))]
mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Os;

    impl Os {
        pub fn show(&mut self, _n: &Notice, _ctx: &egui::Context, _clicks: &Sender<Target>) {}
    }
}
