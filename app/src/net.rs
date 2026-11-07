//! Conexão WebSocket do cliente: `ws://` (só para a própria máquina e a rede local) e `wss://` (TLS, para a internet).
use anyhow::anyhow;
use std::{net::TcpStream, sync::Arc, time::Duration};
use tungstenite::{client::IntoClientRequest, stream::MaybeTlsStream, Connector, WebSocket};

pub type Ws = WebSocket<MaybeTlsStream<TcpStream>>;

/// ws://host:porta + /publish|/watch + /canal?token=...
pub fn url(server: &str, kind: &str, channel: i64, token: &str) -> String {
    format!("{}/{}/{}?token={}", server.trim_end_matches('/'), kind, channel, token)
}

/// `true` se o endereço usa `ws://` (sem criptografia) para um servidor que não é a própria máquina
/// nem a rede local: senha e token passariam em claro pela internet.
pub fn insecure_remote(server: &str) -> bool {
    let s = server.trim();
    let Some(rest) = s.strip_prefix("ws://") else { return false };
    let host = rest.split(['/', '?']).next().unwrap_or("");
    let host = if let Some(h) = host.strip_prefix('[') { h.split(']').next().unwrap_or("") } else { host.split(':').next().unwrap_or("") };
    !is_local_host(host)
}

fn is_local_host(host: &str) -> bool {
    if host == "localhost" || host == "::1" || host.ends_with(".local") {
        return true;
    }
    match host.parse::<std::net::Ipv4Addr>() {
        Ok(ip) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
        Err(_) => false,
    }
}

/// Abre a conexão (com TLS se o endereço for `wss://`).
pub fn connect_ws(url: &str) -> anyhow::Result<Ws> {
    let req = url.into_client_request()?;
    let uri = req.uri().clone();
    let tls = uri.scheme_str() == Some("wss");
    let host = uri.host().ok_or_else(|| anyhow!("endereço sem servidor"))?.to_string();
    let port = uri.port_u16().unwrap_or(if tls { 443 } else { 80 });
    let stream = TcpStream::connect((host.as_str(), port))?;
    let _ = stream.set_nodelay(true);
    let connector = tls.then(|| Connector::Rustls(tls_config()));
    let (ws, _) = tungstenite::client_tls_with_config(req, stream, None, connector)?;
    Ok(ws)
}

/// Raízes públicas (webpki) e, se existir, o certificado de `TELACAST_CA_FILE` (CA própria ou teste).
fn tls_config() -> Arc<rustls::ClientConfig> {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    if let Ok(path) = std::env::var("TELACAST_CA_FILE") {
        use rustls::pki_types::{pem::PemObject, CertificateDer};
        match CertificateDer::pem_file_iter(&path) {
            Ok(certs) => {
                for c in certs.flatten() {
                    let _ = roots.add(c);
                }
            }
            Err(e) => eprintln!("TELACAST_CA_FILE ({path}) não pôde ser lido: {e}"),
        }
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    Arc::new(
        rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .expect("versões TLS")
            .with_root_certificates(roots)
            .with_no_client_auth(),
    )
}

/// Timeout de leitura (para o laço poder checar pedidos de parada), com ou sem TLS.
pub fn set_read_timeout(ws: &mut Ws, d: Duration) -> std::io::Result<()> {
    match ws.get_mut() {
        MaybeTlsStream::Plain(s) => s.set_read_timeout(Some(d)),
        MaybeTlsStream::Rustls(s) => s.get_mut().set_read_timeout(Some(d)),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ws_so_para_maquina_e_rede_local() {
        assert!(!insecure_remote("ws://127.0.0.1:8080"));
        assert!(!insecure_remote("ws://localhost:8080"));
        assert!(!insecure_remote("ws://[::1]:8080"));
        assert!(!insecure_remote("ws://192.168.0.10:8080"));
        assert!(!insecure_remote("ws://10.1.2.3:8080/x"));
        assert!(!insecure_remote("ws://meupc.local:8080"));
        assert!(insecure_remote("ws://203.0.113.5:8080"));
        assert!(insecure_remote("ws://exemplo.com"));
        assert!(!insecure_remote("wss://exemplo.com"), "wss sempre é permitido");
    }
}
