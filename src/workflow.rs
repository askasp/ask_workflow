use crate::db_trait::WorkflowDbTrait;
use crate::worker::Worker;
use crate::workflow_signal::{Signal, WorkflowSignal};
use crate::workflow_state::{self, Closed, WorkflowError, WorkflowState, WorkflowStatus};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::future::Future;
use std::sync::Arc;

use std::ops::Add;
use std::time::{Duration, SystemTime};

pub enum DuplicateStrategy {
    Reject,
    Enqueue,
}

#[async_trait]
pub trait Workflow: Send + Sync {
    fn name(&self) -> &str;
    fn static_name() -> &'static str
    where
        Self: Sized;

    fn static_claim_duration() -> Duration
    where
        Self: Sized,
    {
        Duration::from_secs(60 * 5)
    }

    fn claim_duration(&self) -> Duration {
        Duration::from_secs(60 * 5)
    }

    fn duplicate_strategy(&self) -> DuplicateStrategy {
        DuplicateStrategy::Enqueue
    }

    fn static_duplicate_strategy() -> DuplicateStrategy
    where
        Self: Sized,
    {
        DuplicateStrategy::Enqueue
    }

    async fn run(
        &self,
        worker: Arc<Worker>,
        workflow_state: &mut WorkflowState,
    ) -> Result<Option<Value>, WorkflowErrorType>;

    async fn execute(
        &self,
        db: Arc<dyn WorkflowDbTrait>,
        worker: Arc<Worker>,
        workflow_state: &mut WorkflowState,
    ) -> Result<(), WorkflowErrorType> {
        let existing_workflows = db
            .get_running_workflows(&workflow_state.workflow_type, &workflow_state.instance_id)
            .await?;

        if existing_workflows.len() > 1 {
            let current_start_time = workflow_state.start_time.unwrap_or_else(SystemTime::now);
            let current_created_at = workflow_state.created_at.unwrap_or_else(SystemTime::now);
            let mut is_earliest = true;

            for existing in existing_workflows
                .iter()
                .filter(|wf| wf.run_id != workflow_state.run_id)
            {
                match (existing.start_time, existing.created_at) {
                    (Some(existing_start_time), _) => {
                        // Compare start_time when available
                        if existing_start_time < current_start_time {
                            is_earliest = false;
                            break;
                        }
                    }
                    (None, Some(existing_created_at)) => {
                        // Fallback to created_at if no start_time
                        if workflow_state.start_time.is_none()
                            && existing_created_at < current_created_at
                        {
                            is_earliest = false;
                            break;
                        }
                    }
                    (None, None) => {}
                }
            }

            if !is_earliest {
                match self.duplicate_strategy() {
                    DuplicateStrategy::Reject => {
                        workflow_state.mark_failed(WorkflowErrorType::PermanentError {
                            message: "Duplicate workflow detected".to_string(),
                            content: None,
                        });
                        db.update(workflow_state.clone()).await;
                        return Ok(());
                    }
                    DuplicateStrategy::Enqueue => {
                        workflow_state.scheduled_at = SystemTime::now() + self.claim_duration();
                        println!(
                            "Workflow {} with id {} is a duplicate, rescheduling at {:?}",
                            self.name(),
                            workflow_state.instance_id,
                            workflow_state.scheduled_at
                        );

                        db.update(workflow_state.clone()).await;
                        return Ok(());
                    }
                }
            }
        }

        let duration = self.claim_duration();
        workflow_state.scheduled_at = SystemTime::add(SystemTime::now(), duration);
        if workflow_state.start_time.is_none() {
            workflow_state.start_time = Some(SystemTime::now());
        }

        db.update(workflow_state.clone()).await;
        let result = self.run(worker, workflow_state).await;
        match result {
            Ok(_) => {
                workflow_state.mark_completed();
                db.update(workflow_state.clone()).await;
                Ok(())
            }
            Err(WorkflowErrorType::Pending { schedule_time }) => {
                tracing::info!(
                    "Workflow {} with id {} is pending, rescheduleing",
                    self.name(),
                    workflow_state.instance_id
                );

                workflow_state.scheduled_at = schedule_time;
                workflow_state.updated_at = Some(SystemTime::now());
                db.update(workflow_state.clone()).await;
                Ok(())
            }
            Err(e) if matches!(e, WorkflowErrorType::TransientError { .. }) => {
                {
                    workflow_state.retry(e.clone());
                    workflow_state.updated_at = Some(SystemTime::now());
                    db.update(workflow_state.clone()).await;
                }
                eprintln!(
                    "Got a transient error on workflow {} with id {}, the rror is {:?}",
                    self.name(),
                    workflow_state.instance_id,
                    e
                );
                tracing::warn!(
                    "Got a transient error on workflow {} with id {}, the rror is {:?}",
                    self.name(),
                    workflow_state.instance_id,
                    e
                );

                Err(e)
            }
            Err(e2) => {
                // let state = self.state_mut();
                tracing::error!(
                    "Got a permanent error on workflow {} with id {}, the rror is {:?}",
                    self.name(),
                    workflow_state.instance_id,
                    e2
                );
                workflow_state.mark_failed(e2.clone());

                workflow_state.updated_at = Some(SystemTime::now());
                workflow_state.output = Some(serde_json::to_value(e2.clone()).unwrap_or_default());
                db.update(workflow_state.clone()).await;
                Err(e2)
            }
        }
    }
}
pub async fn run_activity<T, F, Fut>(
    workflow_state: &mut WorkflowState, // Mutable reference to workflow state
    name: &str,                         // Name of the activity
    timeout: Option<SystemTime>,        // Timeout for the activity
    func: F,                            // Function representing the activity
) -> Result<T, WorkflowErrorType>
where
    F: FnOnce(WorkflowState) -> Fut,
    Fut: Future<Output = Result<T, WorkflowErrorType>>,
    T: Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static,
{
    // Check if the activity is already completed (caching logic)
    if let Some(result) = workflow_state.get_activity_result(name) {
        tracing::debug!("Using cached result for activity '{}'", name);
        let cached_result: T = serde_json::from_value(result.clone()).unwrap();
        return Ok(cached_result);
    }

    if let Some(timeout) = timeout {
        if SystemTime::now() > timeout {
            return Err(WorkflowErrorType::PermanentError {
                message: "Activity timeout".to_string(),
                content: None,
            });
        }
    }

    // Run the function with the current state
    match func(workflow_state.clone()).await {
        Ok(result) => {
            println!("Caching result for activity '{}'", name);
            tracing::debug!("Caching result for activity '{}'", name);
            workflow_state.add_activity_result(name, &result);
            Ok(result)
        }
        Err(e) => {
            // Update state with error information
            workflow_state.errors.push(WorkflowError {
                error_type: e.clone(),
                activity_name: Some(name.to_string()),
                timestamp: SystemTime::now(),
            });
            Err(e)
        }
    }
}

// activity_generate_timeout
// activirt_ run poller
// activiy update_due and return.. problem we cant crash as nothing is written to the store
//
//
// but is it a n error?  its better than

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum WorkflowErrorType {
    Pending {
        schedule_time: SystemTime,
    },

    TransientError {
        message: String,
        content: Option<Value>,
    },
    PermanentError {
        message: String,
        content: Option<Value>,
    },
}
impl WorkflowErrorType {
    pub fn perm(message: String) -> Self {
        WorkflowErrorType::PermanentError {
            message,
            content: None,
        }
    }
    pub fn perm_detailed(message: String, content: serde_json::Value) -> Self {
        WorkflowErrorType::PermanentError {
            message,
            content: Some(content),
        }
    }
    pub fn transient(message: String) -> Self {
        WorkflowErrorType::TransientError {
            message,
            content: None,
        }
    }
    pub fn perm_from_error<E: std::fmt::Debug>(message: &str, error: E) -> Self {
        WorkflowErrorType::PermanentError {
            message: message.to_string(),
            content: Some(serde_json::json!({ "error": format!("{:?}", error) })),
        }
    }
    pub fn transient_from_error<E: std::fmt::Debug>(message: &str, error: E) -> Self {
        WorkflowErrorType::TransientError {
            message: message.to_string(),
            content: Some(serde_json::json!({ "error": format!("{:?}", error) })),
        }
    }
}

impl From<serde_json::Error> for WorkflowErrorType {
    fn from(err: serde_json::Error) -> Self {
        WorkflowErrorType::PermanentError {
            message: err.to_string(),
            content: None, // Or provide additional context if available
        }
    }
}

pub fn parse_input<S>(state: &WorkflowState) -> Result<S, WorkflowErrorType>
where
    S: Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static,
{
    let input: S = state
        .input
        .as_ref()
        .ok_or_else(|| WorkflowErrorType::PermanentError {
            message: "No input provided".to_string(),
            content: None,
        })
        .and_then(|value| {
            serde_json::from_value(value.clone()).map_err(|e| WorkflowErrorType::PermanentError {
                message: "Failed to deserialize input".to_string(),
                content: None,
            })
        })?;
    Ok(input)
}
