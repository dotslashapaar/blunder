//! Wire protocol for external scheduler communication

use crate::WorkerAssignment;
use serde::{Deserialize, Serialize};

/// Message from Engine to External Scheduler
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EngineMessage {
    ScheduleRequest {
        request_id: u64,
        items: Vec<ExecutionUnitData>,
        worker_count: usize,
    },
    WorkerCompleted {
        worker_id: usize,
        item_id: String,
        success: bool,
        compute_units: u64,
    },
    Heartbeat {
        timestamp_us: u64,
    },
}

/// Message from External Scheduler to Engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SchedulerMessage {
    ScheduleResponse {
        request_id: u64,
        assignments: Vec<WorkerAssignment>,
    },
    HeartbeatAck {
        timestamp_us: u64,
    },
    Ready,
}

/// Lightweight execution unit metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionUnitData {
    Transaction {
        id: String,
        priority: u64,
        accounts: Vec<[u8; 32]>,
    },
    Bundle {
        id: String,
        tip: u64,
        accounts: Vec<[u8; 32]>,
        num_transactions: usize,
    },
}

impl ExecutionUnitData {
    pub fn id(&self) -> &str {
        match self {
            Self::Transaction { id, .. } => id,
            Self::Bundle { id, .. } => id,
        }
    }

    pub fn accounts(&self) -> &[[u8; 32]] {
        match self {
            Self::Transaction { accounts, .. } => accounts,
            Self::Bundle { accounts, .. } => accounts,
        }
    }
}

pub fn serialize_message<T: Serialize>(msg: &T) -> Result<Vec<u8>, bincode::Error> {
    bincode::serialize(msg)
}

pub fn deserialize_message<T: for<'de> Deserialize<'de>>(
    bytes: &[u8],
) -> Result<T, bincode::Error> {
    bincode::deserialize(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_unit_data_transaction() {
        let data = ExecutionUnitData::Transaction {
            id: "tx123".to_string(),
            priority: 5000,
            accounts: vec![[1u8; 32], [2u8; 32]],
        };

        assert_eq!(data.id(), "tx123");
        assert_eq!(data.accounts().len(), 2);
    }

    #[test]
    fn test_execution_unit_data_bundle() {
        let data = ExecutionUnitData::Bundle {
            id: "bundle456".to_string(),
            tip: 100_000,
            accounts: vec![[3u8; 32], [4u8; 32], [5u8; 32]],
            num_transactions: 3,
        };

        assert_eq!(data.id(), "bundle456");
        assert_eq!(data.accounts().len(), 3);
    }

    #[test]
    fn test_message_serialization() {
        let msg = EngineMessage::ScheduleRequest {
            request_id: 1,
            items: vec![ExecutionUnitData::Transaction {
                id: "tx1".to_string(),
                priority: 1000,
                accounts: vec![[1u8; 32]],
            }],
            worker_count: 4,
        };

        let bytes = serialize_message(&msg).unwrap();
        let decoded: EngineMessage = deserialize_message(&bytes).unwrap();

        match decoded {
            EngineMessage::ScheduleRequest {
                request_id,
                items,
                worker_count,
            } => {
                assert_eq!(request_id, 1);
                assert_eq!(items.len(), 1);
                assert_eq!(worker_count, 4);
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_scheduler_message_serialization() {
        let msg = SchedulerMessage::ScheduleResponse {
            request_id: 42,
            assignments: vec![WorkerAssignment {
                unit_id: "tx1".to_string(),
                worker_id: 2,
                is_bundle: false, // FIXED - no priority field
            }],
        };

        let bytes = serialize_message(&msg).unwrap();
        let decoded: SchedulerMessage = deserialize_message(&bytes).unwrap();

        match decoded {
            SchedulerMessage::ScheduleResponse {
                request_id,
                assignments,
            } => {
                assert_eq!(request_id, 42);
                assert_eq!(assignments.len(), 1);
                assert_eq!(assignments[0].worker_id, 2);
            }
            _ => panic!("Wrong message type"),
        }
    }
}
