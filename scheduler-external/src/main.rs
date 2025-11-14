//! External scheduler server

use blunder_core::{deserialize_message, serialize_message, EngineMessage, SchedulerMessage};
use quinn::{Endpoint, ServerConfig};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{error, info, warn};

mod round_robin;
use round_robin::RoundRobinScheduler;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    rustls::crypto::CryptoProvider::install_default(rustls::crypto::ring::default_provider())
        .expect("install rustls default CryptoProvider");

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let addr: SocketAddr = "127.0.0.1:8080".parse()?;

    info!("Starting external scheduler server...");

    let cert = generate_self_signed_cert()?;
    let server_config = configure_server(cert)?;
    let endpoint = Endpoint::server(server_config, addr)?;

    info!("External scheduler listening on {}", addr);
    info!("Waiting for connections from Engine...");

    while let Some(conn) = endpoint.accept().await {
        tokio::spawn(async move {
            match conn.await {
                Ok(connection) => {
                    info!("New connection from {}", connection.remote_address());
                    if let Err(e) = handle_connection(connection).await {
                        error!("Connection error: {}", e);
                    }
                }
                Err(e) => {
                    error!("Connection failed: {}", e);
                }
            }
        });
    }

    Ok(())
}

async fn handle_connection(conn: quinn::Connection) -> anyhow::Result<()> {
    let scheduler = RoundRobinScheduler::new();
    let mut request_count = 0u64;

    loop {
        match conn.accept_uni().await {
            Ok(mut recv_stream) => {
                let bytes = match recv_stream.read_to_end(1024 * 1024).await {
                    Ok(b) => b,
                    Err(e) => {
                        warn!("Failed to read stream: {}", e);
                        continue;
                    }
                };

                let msg = match deserialize_message::<EngineMessage>(&bytes) {
                    Ok(m) => m,
                    Err(e) => {
                        warn!("Failed to deserialize message: {}", e);
                        continue;
                    }
                };

                match msg {
                    EngineMessage::ScheduleRequest {
                        request_id,
                        items,
                        worker_count,
                    } => {
                        request_count += 1;
                        info!(
                            "Request #{} (id: {}) - {} items, {} workers",
                            request_count,
                            request_id,
                            items.len(),
                            worker_count
                        );

                        let assignments = scheduler.schedule(&items, worker_count);

                        info!(
                            "Generated {} assignments for request {}",
                            assignments.len(),
                            request_id
                        );

                        let response = SchedulerMessage::ScheduleResponse {
                            request_id,
                            assignments,
                        };

                        if let Err(e) = send_response(&conn, response).await {
                            error!("Failed to send response: {}", e);
                        }
                    }
                    EngineMessage::WorkerCompleted {
                        worker_id,
                        item_id,
                        success,
                        compute_units,
                    } => {
                        info!(
                            "Worker {} completed {} - success: {}, CU: {}",
                            worker_id, item_id, success, compute_units
                        );
                    }
                    EngineMessage::Heartbeat { timestamp_us } => {
                        info!("Heartbeat at {}", timestamp_us);

                        let response = SchedulerMessage::HeartbeatAck { timestamp_us };
                        if let Err(e) = send_response(&conn, response).await {
                            error!("Failed to send heartbeat ack: {}", e);
                        }
                    }
                }
            }
            Err(quinn::ConnectionError::ApplicationClosed(_)) => {
                info!("Client closed connection");
                break;
            }
            Err(e) => {
                error!("Error accepting stream: {}", e);
                break;
            }
        }
    }

    info!("Connection closed (processed {} requests)", request_count);
    Ok(())
}

async fn send_response(conn: &quinn::Connection, response: SchedulerMessage) -> anyhow::Result<()> {
    let mut send_stream = conn.open_uni().await?;
    let bytes = serialize_message(&response)?;
    send_stream.write_all(&bytes).await?;
    send_stream.finish()?;
    Ok(())
}

fn generate_self_signed_cert() -> anyhow::Result<(
    Vec<rustls::pki_types::CertificateDer<'static>>,
    rustls::pki_types::PrivateKeyDer<'static>,
)> {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])?;
    let cert_der = cert.serialize_der()?;
    let priv_key = cert.serialize_private_key_der();

    let cert_chain = vec![rustls::pki_types::CertificateDer::from(cert_der)];
    let priv_key = rustls::pki_types::PrivateKeyDer::try_from(priv_key)
        .map_err(|e| anyhow::anyhow!("Failed to parse private key: {}", e))?;

    Ok((cert_chain, priv_key))
}

fn configure_server(
    (cert_chain, priv_key): (
        Vec<rustls::pki_types::CertificateDer<'static>>,
        rustls::pki_types::PrivateKeyDer<'static>,
    ),
) -> anyhow::Result<ServerConfig> {
    let mut server_config = ServerConfig::with_single_cert(cert_chain, priv_key)?;

    let transport_config = Arc::get_mut(&mut server_config.transport)
        .ok_or_else(|| anyhow::anyhow!("Failed to get transport config"))?;

    transport_config.max_concurrent_uni_streams(100_u32.into());
    transport_config.max_idle_timeout(Some(std::time::Duration::from_secs(60).try_into()?));

    Ok(server_config)
}
