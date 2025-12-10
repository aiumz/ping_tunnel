use crate::transport::accept::register_on_accept_stream;
use crate::transport::base::{ServerConfig, TransformServer};
use crate::transport::quic::QuinnServerEndpoint;
use crate::tunnel::common::{AUTH_TOKEN_KEY, get_client_id_from_token};
use crate::tunnel::inbound::{InboundConfig, bind_tcp_inbound};
use crate::tunnel::outbound::tcp_outbound;
use crate::tunnel::packet::{TunnelCommand, TunnelMeta, TunnelPacket, response_packet};
use crate::tunnel::session::{
    TransportSession, get_session, insert_session, refresh_session_by_id,
};
use serde_json::Value;
use tracing::{debug, error, info, warn};

pub async fn start_server(
    quic_bind_addr: String,
    tcp_bind_addr: String,
    cert_path: String,
    key_path: String,
) -> anyhow::Result<()> {
    info!(
        "Initializing with QUIC={} TCP={} cert={} key={}",
        quic_bind_addr, tcp_bind_addr, cert_path, key_path
    );
    let config = ServerConfig {
        addr: quic_bind_addr.clone(),
        ssl_cert_path: cert_path.clone(),
        ssl_key_path: key_path.clone(),
    };
    let inbound_config = InboundConfig {
        inbound_addr: tcp_bind_addr.clone(),
    };

    register_on_accept_stream(move |_conn, stream| async move {
        debug!("Bi-directional QUIC stream accepted, waiting for command...");
        let (mut stream_reader, stream_writer) = tokio::io::split(stream);
        let packet = match TunnelPacket::read_command(&mut stream_reader).await {
            Ok(packet) => packet,
            Err(err) => {
                error!("Failed to read command packet: {:?}", err);
                return Err(err);
            }
        };
        match packet.command {
            TunnelCommand::Forward => {
                debug!("Forward command meta: {:?}", packet.meta);
                if let Err(err) =
                    tcp_outbound(stream_reader, stream_writer, packet, Option::None).await
                {
                    error!("forward_to_tcp failed: {:?}", err);
                    return Err(err);
                }
            }
            TunnelCommand::Ping => {
                let client_id = match packet.meta.get(AUTH_TOKEN_KEY) {
                    Some(token) => match token.as_str() {
                        Some(token) => token,
                        None => "",
                    },
                    None => "",
                };
                if get_session(client_id).await.is_some() {
                    refresh_session_by_id(client_id).await;
                    if let Err(err) =
                        response_packet(stream_writer, TunnelCommand::Pong, &packet.meta).await
                    {
                        error!("Failed to respond Pong: {:?}", err);
                        return Err(err);
                    }
                } else {
                    warn!(
                        "[QUIC Server] Session not found for client_id: {}",
                        client_id
                    );
                }
            }
            TunnelCommand::Auth => {
                let mut meta = TunnelMeta::from([("result".to_string(), Value::Bool(false))]);
                let token: Option<&Value> = packet.meta.get(AUTH_TOKEN_KEY);
                if let Some(token) = token {
                    if let Some(token_str) = token.as_str() {
                        let client_id = get_client_id_from_token(token_str);
                        _conn.set_client_id(client_id.clone());
                        insert_session(
                            client_id,
                            TransportSession {
                                conn: _conn.clone(),
                                meta: packet.meta.clone(),
                            },
                        )
                        .await;
                        meta = TunnelMeta::from([("result".to_string(), Value::Bool(true))]);
                    }
                }
                if let Err(err) =
                    response_packet(stream_writer, TunnelCommand::AuthResult, &meta).await
                {
                    error!("Failed to respond AuthResult: {:?}", err);
                    return Err(err);
                }
            }
            _ => {
                warn!("Unsupported command: {:?}", packet.command);
            }
        }

        Ok(())
    });

    if let Err(e) = QuinnServerEndpoint::bind(config).await {
        error!("Failed to bind QUIC server: {:?}", e);
        return Err(e);
    }

    if let Err(e) = bind_tcp_inbound(inbound_config, false).await {
        error!("Failed to bind TCP inbound: {:?}", e);
        return Err(e);
    }
    Ok(())
}
