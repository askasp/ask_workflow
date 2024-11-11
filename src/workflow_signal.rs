use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::any::type_name;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::time::sleep;

use crate::workflow::{Workflow };

#[derive(Serialize, Deserialize, Clone)]
pub struct Signal {
    pub id: String,
    pub workflow_name: String,
    pub instance_id: String,
    pub signal_name: String,
    pub sent_at: SystemTime,
    pub processed_at: Option<SystemTime>,
    pub data: serde_json::Value,
    pub processed: bool,
    pub target_or_source_run_id: Option<String>,
    pub direction: SignalDirection,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub enum SignalDirection {
    FromWorkflow,
    ToWorkflow,
}
// Implement Display to convert SignalDirection to a string
impl fmt::Display for SignalDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let direction_str = match self {
            SignalDirection::FromWorkflow => "FromWorkflow",
            SignalDirection::ToWorkflow => "ToWorkflow",
        };
        write!(f, "{}", direction_str)
    }
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
    fn direction() -> SignalDirection;

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
}
