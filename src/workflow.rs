use crate::db_trait::DB;
use crate::workflow_state::{WorkflowError, WorkflowState, WorkflowStatus};
use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::future::Future;
use std::sync::Arc;
use std::any::Any;

use std::ops::Add;
use std::pin::Pin;
use std::time::{Duration, SystemTime};


#[async_trait]
pub trait Workflow: Send + Sync {

    fn name(&self) -> &str;
    fn static_name() -> &'static str
    where
        Self: Sized;


    async fn run(&mut self, db: Arc<dyn DB>) -> Result<Option<Value>, WorkflowErrorType>;

    fn state_mut(&mut self) -> &mut WorkflowState; // Provides mutable access to the workflow state
    fn state(&self) -> &WorkflowState; // Provides mutable access to the workflow state

    async fn execute(&mut self, db: Arc<dyn DB>) -> Result<(), WorkflowErrorType> {
        {
            let state = self.state_mut();
            state.scheduled_at = SystemTime::add(SystemTime::now(), state.claim_duration);
            if state.start_time.is_none() {
                state.start_time = Some(SystemTime::now());
            }

            db.insert(state.clone()).await;
        }
        let result = self.run(db.clone()).await;
        match result {
            Ok(output) => {
                let state = self.state_mut();
                state.mark_completed();
                state.output = Some(serde_json::to_value(output).unwrap_or_default());
                db.update(state.clone()).await;
                eprintln!(
                    "Workflow successfully completed {}- {}",
                    self.name(),
                    self.state().instance_id
                );
                Ok(())
            }
            Err(e) if matches!(e, WorkflowErrorType::TransientError { .. }) => {
                {
                    let state = self.state_mut();
                    state.retry();
                    db.update(state.clone()).await;
                }
                eprintln!(
                    "Got a transient error on workflow {} with id {}, the rror is {:?}",
                    self.name(),
                    self.state().instance_id,
                    e
                );
                Err(e)
            }
            Err(e2) => {
                let state = self.state_mut();
                state.mark_failed();
                state.output = Some(serde_json::to_value(e2.clone()).unwrap_or_default());
                db.update(state.clone()).await;
                Err(e2)
            }
        }
    }
}

pub async fn run_activity<T, F, Fut>(
    name: &str,
    state: &mut WorkflowState,
    db: Arc<dyn DB>,
    func: F,
) -> Result<T, WorkflowErrorType>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, WorkflowErrorType>>,
    T: Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static,
{
    // Check if the activity is already completed
    if let Some(result) = state.get_activity_result(name) {
        println!("Using cached result for activity '{}'", name);
        let cached_result: T = serde_json::from_value(result.clone()).unwrap();
        return Ok(cached_result);
    }

    // Run the function and await its result
    match func().await {
        Ok(result) => {
            println!("Caching result for activity '{}'", name);
            state.add_activity_result(name, &result);
            db.update(state.clone()).await;
            Ok(result)
        }
        Err(e) => {
            state.errors.push(WorkflowError {
                error_type: e.clone(),
                activity_name: name.to_string(),
                timestamp: SystemTime::now(),
            });
            db.update(state.clone()).await;
            Err(e)
        }
    }
}
pub async fn run_sync_activity<T, F>(
    name: &str,
    state: &mut WorkflowState,
    db: Arc<dyn DB>,
    func: F,
) -> Result<T, WorkflowErrorType>
where
    F: FnOnce() -> Result<T, WorkflowErrorType>,
    T: Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static,
{
    // Check if the activity is already completed
    if let Some(result) = state.get_activity_result(name) {
        println!("Using cached result for activity '{}'", name);
        let cached_result: T = serde_json::from_value(result.clone()).unwrap();
        return Ok(cached_result);
    }

    // Run the function and get its result
    match func() {
        Ok(result) => {
            println!("Caching result for activity '{}'", name);
            state.add_activity_result(name, &result);
            db.update(state.clone()).await; // Await the async update call
            Ok(result)
        }
        Err(e) => {
            state.errors.push(WorkflowError {
                error_type: e.clone(),
                activity_name: name.to_string(),
                timestamp: SystemTime::now(),
            });
            db.update(state.clone()).await; // Await the async update call
            Err(e)
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum WorkflowErrorType {
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

// #[typetag::serde(tag = "workflow_type")]
// #[async_trait]
// pub trait Workflow_2: Send + Sync + Serialize + 'static {
//     type Input: Serialize + for<'de> Deserialize<'de> + Send + Sync;
//     type Output: Serialize + for<'de> Deserialize<'de> + Send + Sync;
//     type Signal: Serialize + for<'de> Deserialize<'de> + Send + Sync;

//     fn workflow_id(&self) -> &str;

//     async fn run(
//         &mut self,
//         input: Self::Input,
//         runner: Arc<WorkflowRunner_2<Self>>,
//     ) -> Result<Self::Output, WorkflowErrorType_2>;

//     async fn execute(
//         &mut self,
//         input: Self::Input,
//         runner: Arc<WorkflowRunner_2<Self>>,
//     ) -> Result<Self::Output, WorkflowErrorType_2> {
//         let result = self.run(input, runner).await?;
//         Ok(result)
//     }
// }
