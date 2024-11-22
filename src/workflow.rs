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
    Replace,
}

#[async_trait]
pub trait Workflow: Send + Sync {
    fn name(&self) -> &str;
    fn static_name() -> &'static str
    where
        Self: Sized;

    fn claim_duration(&self) -> Duration {
        Duration::from_secs(60 * 5)
    }
    fn duplicate_strategy() -> DuplicateStrategy
    where
        Self: Sized,
    {
        DuplicateStrategy::Replace
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
            Err(WorkflowErrorType::Pending {schedule_time}) => {
                println!("Workflow {} with id {} is pending, rescheduleing", self.name(), workflow_state.instance_id);
                tracing::info!("Workflow {} with id {} is pending, rescheduleing", self.name(), workflow_state.instance_id);
                
                workflow_state.scheduled_at = schedule_time;
                db.update(workflow_state.clone()).await;
                Ok(())

            },
            Err(e) if matches!(e, WorkflowErrorType::TransientError { .. }) => {
                {
                    workflow_state.retry(e.clone());
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

    // Run the function with the current state
    match func(workflow_state.clone()).await {
        Ok(result) => {
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
