use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::time::{sleep, Instant};

use crate::db_trait::{unique_workflow_id, DB};
use crate::workflow::WorkflowErrorType;
use crate::workflow_state::{WorkflowError, WorkflowState};

#[derive(Serialize, Deserialize, Clone)]
pub struct Signal {
    pub id: String,
    pub timestamp: SystemTime,
    pub data: serde_json::Value,
}

#[async_trait]
pub trait WorkflowSignal:
    Serialize + for<'de> Deserialize<'de> + Clone + Send + Sync + 'static
{
    fn signal_id(&self) -> String;

    async fn send(&self, db: Arc<dyn DB>) -> Result<(), WorkflowErrorType> {
        let signal_data = Signal {
            id: format!("{}-{}", Self::static_signal_name(), self.signal_id()),
            timestamp: SystemTime::now(),
            data: serde_json::to_value(self.clone())?,
        };

        db.insert_signal(signal_data)
            .await
            .map_err(|e| WorkflowErrorType::TransientError {
                message: "Failed to insert signal".to_string(),
                content: Some(serde_json::to_value(e).unwrap()),
            })
    }

    async fn await_signal(
        db: Arc<dyn DB>,
        unique_workflow_id: &str,
        poll_interval: Duration,
        timeout: Duration,
    ) -> Result<Self, WorkflowErrorType>
    where
        Self: Sized + PartialEq,
    {
        let deadline = Instant::now() + timeout;
        let signal_id = format!(
            "{}-{}",
            Self::static_signal_name(),
            unique_workflow_id
        );

        while Instant::now() < deadline {
            if let Some(signal) = db.get_signal(&signal_id).await? {
                let signal: Self = serde_json::from_value(signal.data.clone()).map_err(|e| {
                    WorkflowErrorType::PermanentError {
                        message: "Failed to deserialize signal".to_string(),
                        content: None,
                    }
                })?;
                return Ok(signal);
            }
            sleep(poll_interval).await;
        }

        Err(WorkflowErrorType::PermanentError {
            message: "Signal timeout reached".to_string(),
            content: None,
        })
    }

    // A method to define a static name for the signal (e.g., "UserCreationComplete")
    fn static_signal_name() -> &'static str;
}
