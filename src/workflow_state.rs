use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;
use std::time::{Duration, SystemTime};

use crate::workflow::WorkflowErrorType;

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub enum WorkflowStatus {
    Open(Open),
    Closed(Closed),
}
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub enum Open {
    Running,
}
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub enum Closed {
    Completed,
    Cancelled,
    Failed { error: WorkflowErrorType },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorkflowError {
    pub error_type: WorkflowErrorType,
    pub activity_name: Option<String>,
    pub timestamp: SystemTime,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct WorkflowState {
    pub workflow_type: String,
    pub instance_id: String,
    pub run_id: String,
    pub results: HashMap<String, Value>, // Store activity results by activity name
    pub scheduled_at: SystemTime,
    pub status: WorkflowStatus,
    pub retries: u32,          // Track the number of retries
    pub max_retries: u32,      // Maximum number of retries allowed
    pub output: Option<Value>, // Optional output of the workflow
    pub errors: Vec<WorkflowError>,
    pub start_time: Option<SystemTime>,
    pub end_time: Option<SystemTime>,
    pub input: Option<Value>,
}

impl WorkflowState {
    pub fn unique_id(&self) -> String {
        format!("{}-{}", self.workflow_type, self.instance_id)
    }

    pub fn new(
        workflow_type: &str,
        instance_id: &str,
        scheduled_at: SystemTime,
        input: Option<Value>,
    ) -> Self {
        Self {
            workflow_type: workflow_type.to_string(),
            instance_id: instance_id.to_string(),
            results: HashMap::new(),
            scheduled_at,
            run_id: cuid::cuid1().unwrap(),
            retries: 0,      // Start with 0 retries
            max_retries: 17, // Default max retries set to 17 for exponential backoff over 3 days
            output: None,
            status: WorkflowStatus::Open(Open::Running),
            errors: vec![],
            start_time: None,
            end_time: None,
            input,
        }
    }

    // Mark the workflow as completed
    pub fn mark_completed(&mut self) {
        self.status = WorkflowStatus::Closed(Closed::Completed);
        self.end_time = Some(SystemTime::now());
    }
    pub fn mark_failed(&mut self, error: WorkflowErrorType) {
        self.status = WorkflowStatus::Closed(Closed::Failed { error })
    }

    // Check if an activity has already completed
    pub fn has_activity_result(&self, activity_name: &str) -> bool {
        self.results.contains_key(activity_name)
    }

    // Get the result of a completed activity
    pub fn get_activity_result(&self, activity_name: &str) -> Option<&Value> {
        self.results.get(activity_name)
    }

    // Add an activity result
    pub fn add_activity_result<R: Serialize>(&mut self, activity_name: &str, result: R) {
        let json_result = serde_json::to_value(result).unwrap();
        self.results.insert(activity_name.to_string(), json_result);
    }

    // Calculate the next retry time using exponential backoff
    pub fn calculate_next_retry(&self) -> Option<SystemTime> {
        if self.retries >= self.max_retries {
            return None; // No more retries
        }

        // Initial delay in milliseconds
        let base_delay_ms = 500;
        // Exponential backoff: delay = base_delay * (2^n), where n is the retry count
        let exponential_backoff_ms = base_delay_ms * (1 << self.retries);

        let retry_duration = Duration::from_millis(exponential_backoff_ms as u64);
        Some(SystemTime::now() + retry_duration)
    }

    // Mark the workflow as failed and ready for the next retry
    pub fn retry(&mut self, error: WorkflowErrorType) {
        if self.retries < self.max_retries {
            self.retries += 1;
            if let Some(next_time) = self.calculate_next_retry() {
                self.scheduled_at = next_time;
            }
        } else {
            self.status = WorkflowStatus::Closed(Closed::Failed { error })
        }
    }
}

impl fmt::Display for WorkflowStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status_str = match self {
            WorkflowStatus::Open(Open::Running) => "Running",
            WorkflowStatus::Closed(Closed::Completed) => "Completed",
            WorkflowStatus::Closed(Closed::Cancelled) => "Cancelled",
            WorkflowStatus::Closed(Closed::Failed { error: x }) => &format!("Failed {:?}", x),
        };
        write!(f, "{}", status_str)
    }
}
