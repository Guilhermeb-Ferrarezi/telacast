/// ws://host:porta + /publish|/watch + /canal?token=...
pub fn url(server: &str, kind: &str, channel: i64, token: &str) -> String {
    format!("{}/{}/{}?token={}", server.trim_end_matches('/'), kind, channel, token)
}
