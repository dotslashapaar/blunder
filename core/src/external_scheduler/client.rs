//! QUIC client for external scheduler

use super::protocol::*;
use crate::{MevError, Result, WorkerAssignment};
use quinn::{ClientConfig, Endpoint};
use std::collections::{HashMap, HashSet};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

const BIND_ADDR: SocketAddr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0));

// Certificate verifier that skips validation for localhost testing
#[derive(Debug)]
struct SkipServerVerification;

impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ED25519,
        ]
    }
}

pub struct ExternalSchedulerClient {
    #[allow(dead_code)]
    endpoint: Endpoint,
    connection: Arc<Mutex<Option<quinn::Connection>>>,
    request_counter: Arc<Mutex<u64>>,
    pending_requests: Arc<Mutex<HashMap<u64, PendingRequest>>>,
}

struct PendingRequest {
    response_tx: mpsc::UnboundedSender<Vec<WorkerAssignment>>,
}

impl ExternalSchedulerClient {
    pub async fn connect(server_addr: SocketAddr) -> Result<Self> {
        // Create rustls config that skips certificate verification for localhost
        let rustls_config = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
            .with_no_client_auth();

        // Wrap in QuicClientConfig for quinn
        let crypto = quinn::crypto::rustls::QuicClientConfig::try_from(rustls_config)
            .map_err(|e| MevError::InitError(format!("Crypto config failed: {:?}", e)))?;

        let client_config = ClientConfig::new(Arc::new(crypto));

        let mut endpoint = Endpoint::client(BIND_ADDR)
            .map_err(|e| MevError::InitError(format!("Endpoint creation failed: {}", e)))?;
        endpoint.set_default_client_config(client_config);

        let connection = endpoint
            .connect(server_addr, "localhost")
            .map_err(|e| MevError::InitError(format!("Connect failed: {}", e)))?
            .await
            .map_err(|e| MevError::InitError(format!("Connection failed: {}", e)))?;

        let client = Self {
            endpoint,
            connection: Arc::new(Mutex::new(Some(connection.clone()))),
            request_counter: Arc::new(Mutex::new(0)),
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
        };

        let connection_clone = client.connection.clone();
        let pending_clone = client.pending_requests.clone();
        tokio::spawn(async move {
            Self::receive_loop(connection_clone, pending_clone).await;
        });

        Ok(client)
    }

    pub async fn schedule(
        &self,
        items: Vec<ExecutionUnitData>,
        worker_count: usize,
    ) -> Result<Vec<WorkerAssignment>> {
        let request_id = {
            let mut counter = self.request_counter.lock().await;
            *counter += 1;
            *counter
        };

        let msg = EngineMessage::ScheduleRequest {
            request_id,
            items: items.clone(),
            worker_count,
        };

        let bytes =
            serialize_message(&msg).map_err(|e| MevError::SerializationError(format!("{}", e)))?;

        let (response_tx, mut response_rx) = mpsc::unbounded_channel::<Vec<WorkerAssignment>>();
        self.pending_requests
            .lock()
            .await
            .insert(request_id, PendingRequest { response_tx });

        let conn_guard = self.connection.lock().await;
        if let Some(ref conn) = *conn_guard {
            let mut send_stream = conn
                .open_uni()
                .await
                .map_err(|e| MevError::NetworkError(format!("Stream open failed: {}", e)))?;

            send_stream
                .write_all(&bytes)
                .await
                .map_err(|e| MevError::NetworkError(format!("Send failed: {}", e)))?;

            send_stream
                .finish()
                .map_err(|e| MevError::NetworkError(format!("Finish failed: {}", e)))?;
        } else {
            return Err(MevError::NetworkError("Not connected".to_string()));
        }
        drop(conn_guard);

        let result = tokio::select! {
            Some(assignments) = response_rx.recv() => {
                Self::validate_assignments(&assignments, &items, worker_count)?;
                Ok(assignments)
            },
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                self.pending_requests.lock().await.remove(&request_id);
                Err(MevError::Timeout("Scheduler timeout".to_string()))
            }
        };

        result
    }

    pub async fn report_completion(
        &self,
        worker_id: usize,
        item_id: String,
        success: bool,
        compute_units: u64,
    ) -> Result<()> {
        let msg = EngineMessage::WorkerCompleted {
            worker_id,
            item_id,
            success,
            compute_units,
        };

        let bytes =
            serialize_message(&msg).map_err(|e| MevError::SerializationError(format!("{}", e)))?;

        let conn_guard = self.connection.lock().await;
        if let Some(ref conn) = *conn_guard {
            if let Ok(mut send_stream) = conn.open_uni().await {
                send_stream.write_all(&bytes).await.ok();
                send_stream.finish().ok();
            }
        }

        Ok(())
    }

    fn validate_assignments(
        assignments: &[WorkerAssignment],
        items: &[ExecutionUnitData],
        worker_count: usize,
    ) -> Result<()> {
        let mut seen = HashSet::new();
        let valid_ids: HashSet<String> = items.iter().map(|i| i.id().to_string()).collect();

        for assignment in assignments {
            if !seen.insert(assignment.unit_id.clone()) {
                return Err(MevError::InvalidAssignment(
                    "Duplicate assignment".to_string(),
                ));
            }

            if !valid_ids.contains(&assignment.unit_id) {
                return Err(MevError::InvalidAssignment(format!(
                    "Unknown unit_id: {}",
                    assignment.unit_id
                )));
            }

            if assignment.worker_id >= worker_count {
                return Err(MevError::InvalidAssignment(format!(
                    "Invalid worker_id: {}",
                    assignment.worker_id
                )));
            }
        }

        Ok(())
    }

    async fn receive_loop(
        connection: Arc<Mutex<Option<quinn::Connection>>>,
        pending: Arc<Mutex<HashMap<u64, PendingRequest>>>,
    ) {
        loop {
            let conn_guard = connection.lock().await;
            if let Some(ref conn) = *conn_guard {
                let conn_clone = conn.clone();
                drop(conn_guard);

                match conn_clone.accept_uni().await {
                    Ok(mut recv_stream) => match recv_stream.read_to_end(1024 * 1024).await {
                        Ok(bytes) => {
                            if let Ok(msg) = deserialize_message::<SchedulerMessage>(&bytes) {
                                Self::handle_message(msg, &pending).await;
                            }
                        }
                        Err(_) => break,
                    },
                    Err(_) => break,
                }
            } else {
                break;
            }
        }
    }

    async fn handle_message(
        msg: SchedulerMessage,
        pending: &Arc<Mutex<HashMap<u64, PendingRequest>>>,
    ) {
        if let SchedulerMessage::ScheduleResponse {
            request_id,
            assignments,
        } = msg
        {
            let mut pending_guard = pending.lock().await;
            if let Some(req) = pending_guard.remove(&request_id) {
                let _ = req.response_tx.send(assignments);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_assignments_success() {
        let items = vec![
            ExecutionUnitData::Transaction {
                id: "tx1".to_string(),
                priority: 1000,
                accounts: vec![[1u8; 32]],
            },
            ExecutionUnitData::Transaction {
                id: "tx2".to_string(),
                priority: 2000,
                accounts: vec![[2u8; 32]],
            },
        ];

        let assignments = vec![
            WorkerAssignment {
                unit_id: "tx1".to_string(),
                worker_id: 0,
                is_bundle: false,
            },
            WorkerAssignment {
                unit_id: "tx2".to_string(),
                worker_id: 1,
                is_bundle: false,
            },
        ];

        let result = ExternalSchedulerClient::validate_assignments(&assignments, &items, 4);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_assignments_duplicate() {
        let items = vec![ExecutionUnitData::Transaction {
            id: "tx1".to_string(),
            priority: 1000,
            accounts: vec![[1u8; 32]],
        }];

        let assignments = vec![
            WorkerAssignment {
                unit_id: "tx1".to_string(),
                worker_id: 0,
                is_bundle: false,
            },
            WorkerAssignment {
                unit_id: "tx1".to_string(),
                worker_id: 1,
                is_bundle: false,
            },
        ];

        let result = ExternalSchedulerClient::validate_assignments(&assignments, &items, 4);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            MevError::InvalidAssignment(_)
        ));
    }

    #[test]
    fn test_validate_assignments_unknown_id() {
        let items = vec![ExecutionUnitData::Transaction {
            id: "tx1".to_string(),
            priority: 1000,
            accounts: vec![[1u8; 32]],
        }];

        let assignments = vec![WorkerAssignment {
            unit_id: "tx999".to_string(),
            worker_id: 0,
            is_bundle: false,
        }];

        let result = ExternalSchedulerClient::validate_assignments(&assignments, &items, 4);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_assignments_invalid_worker() {
        let items = vec![ExecutionUnitData::Transaction {
            id: "tx1".to_string(),
            priority: 1000,
            accounts: vec![[1u8; 32]],
        }];

        let assignments = vec![WorkerAssignment {
            unit_id: "tx1".to_string(),
            worker_id: 99,
            is_bundle: false,
        }];

        let result = ExternalSchedulerClient::validate_assignments(&assignments, &items, 4);
        assert!(result.is_err());
    }
}
