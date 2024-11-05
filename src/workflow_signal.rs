use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::any::type_name;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::time::{sleep };

use crate::db_trait::{ DB};
use crate::workflow::{Workflow, WorkflowErrorType};
use crate::workflow_state::{WorkflowError, WorkflowState};

#[derive(Serialize, Deserialize, Clone)]
pub struct Signal {
    pub id: String,
    pub timestamp: SystemTime,
    pub data: serde_json::Value,
}

#[async_trait]
pub trait WorkflowSignal: Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static {
    type Workflow: Workflow;
    fn signal_name(&self) -> &'static str {
        Self::static_signal_name()
    }
    fn workflow_name() -> &'static str {
        <Self::Workflow as Workflow>::static_name()
    }

    fn static_signal_name() -> &'static str {
        let full_name = type_name::<Self>();
        full_name.split("::").last().unwrap_or(full_name)
    }

    fn create_signal_id(&self, instance_id: &str) -> String {
        format!(
            "{}-{}-{}",
            self.signal_name(),
            Self::workflow_name(),
            instance_id
        )
    }
    fn static_create_signal_id(instance_id: &str) -> String {
        format!(
            "{}-{}-{}",
            Self::static_signal_name(),
            Self::workflow_name(),
            instance_id
        )
    }
    async fn receive_signal(
        &self,
        db: Arc<dyn DB>,
        instance_id: &str,
        poll_interval: Duration,
        timeout: Duration,
    ) -> Result<Self, WorkflowErrorType>
    where
        Self: Sized,
    {
        let signal_id = self.create_signal_id(instance_id);
        let start = tokio::time::Instant::now();

        loop {
            if start.elapsed() >= timeout {
                return Err(WorkflowErrorType::PermanentError {
                    message: "Signal timeout reached".to_string(),
                    content: None,
                });
            }

            if let Ok(Some(signal)) = db.get_signal(&signal_id).await {
                let data: Self = serde_json::from_value(signal.data).map_err(|e| {
                    WorkflowErrorType::PermanentError {
                        message: "Failed to deserialize signal data".to_string(),
                        content: None,
                    }
                })?;
                return Ok(data);
            }

            sleep(poll_interval).await;
        }
    }
    async fn send_signal(
        &self,
        db: Arc<dyn DB>,
        instance_id: &str,
    ) -> Result<(), WorkflowErrorType> {
        let signal_data = Signal {
            id: self.create_signal_id(instance_id),
            timestamp: SystemTime::now(),
            data: serde_json::to_value(self).map_err(|e| WorkflowErrorType::PermanentError {
                message: "Failed to serialize signal data".to_string(),
                content: None,
            })?,
        };
        db.insert_signal(signal_data).await
    }
}
