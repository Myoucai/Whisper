//! TLS client transport for the HTTP capabilities (rustls, ring backend).
//!
//! One process-wide client config with the webpki root store; tests can
//! inject a custom config (self-signed roots) via `tls_connect_with_config`.

use std::net::TcpStream;
use std::sync::Arc;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

fn default_config() -> Arc<ClientConfig> {
    let roots = RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let mut cfg = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    // We speak HTTP/1.1 by hand; offering the ALPN pins servers off h2.
    cfg.alpn_protocols = vec![b"http/1.1".to_vec()];
    Arc::new(cfg)
}

fn server_name(host: &str) -> Result<ServerName<'static>, String> {
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return Ok(ServerName::from(ip));
    }
    ServerName::try_from(host.to_owned())
        .map_err(|e| format!("invalid TLS server name '{host}': {e}"))
}

/// Upgrade a connected TcpStream to TLS with the default webpki roots.
pub fn tls_connect(
    stream: TcpStream,
    host: &str,
) -> Result<StreamOwned<ClientConnection, TcpStream>, String> {
    static CONFIG: std::sync::OnceLock<Arc<ClientConfig>> = std::sync::OnceLock::new();
    let config = CONFIG.get_or_init(default_config).clone();
    tls_connect_with_config(stream, host, config)
}

/// Upgrade with an explicit config (tests inject a self-signed root).
pub fn tls_connect_with_config(
    stream: TcpStream,
    host: &str,
    config: Arc<ClientConfig>,
) -> Result<StreamOwned<ClientConnection, TcpStream>, String> {
    let mut conn = ClientConnection::new(config, server_name(host)?)
        .map_err(|e| format!("TLS setup for '{host}' failed: {e}"))?;
    // Drive the handshake to completion here: `Connection::write` returns
    // Ok(0) while handshaking, so a later write_all would spin forever.
    while conn.is_handshaking() {
        if std::env::var("WHISPER_TLS_TRACE").is_ok() {
            eprintln!("[tls] handshake iter: wants_read={} wants_write={}",
                conn.wants_read(), conn.wants_write());
        }
        conn.complete_io(&mut { &stream })
            .map_err(|e| format!("TLS handshake with '{host}' failed: {e}"))?;
    }
    Ok(StreamOwned::new(conn, stream))
}
