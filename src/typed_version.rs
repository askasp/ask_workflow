use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, fmt, sync::Arc, time::{Duration, SystemTime}};
use tokio::time;

// Adjusted DB_2 to work with `Value` instead of generics
pub trait DB_2: Send + Sync {
    fn insert_workflow_state(&mut self, instance_id: String, state: Value);
    fn get_workflow_state(&self, instance_id: &str) -> Option<Value>;
}

// Mock in-memory database for testing
pub struct MockDB_2 {
    pub states: HashMap<String, Value>,
}

impl DB_2 for MockDB_2 {
    fn insert_workflow_state(&mut self, instance_id: String, state: Value) {
        self.states.insert(instance_id, state);
    }

    fn get_workflow_state(&self, instance_id: &str) -> Option<Value> {
        self.states.get(instance_id).cloned()
    }
}

// Workflow Status
#[derive(Deserialize, Serialize, Debug, Clone)]
pub enum WorkflowStatus_2<S> {
    Open(Open_2<S>),
    Closed(Closed_2),
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub enum Open_2<S> {
    Running,
    AwaitingSignal(S),
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub enum Closed_2 {
    Completed,
    Cancelled,
    Failed,
}

// Workflow Error
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WorkflowError_2 {
    pub message: String,
    pub activity_name: String,
    pub timestamp: SystemTime,
}

// Workflow State
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WorkflowState_2<S> {
    pub workflow_type: String,
    pub instance_id: String,
    pub results: HashMap<String, Value>,
    pub scheduled_at: SystemTime,
    pub status: WorkflowStatus_2<S>,
    pub retries: u32,
    pub max_retries: u32,
    pub output: Option<Value>,
    pub errors: Vec<WorkflowError_2>,
    pub claim_duration: Duration,
    pub start_time: Option<SystemTime>,
    pub end_time: Option<SystemTime>,
    pub input: Option<Value>,
}

impl<S> WorkflowState_2<S> {
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
            retries: 0,
            max_retries: 17,
            output: None,
            status: WorkflowStatus_2::Open(Open_2::Running),
            errors: vec![],
            claim_duration: Duration::from_secs(60),
            start_time: None,
            end_time: None,
            input,
        }
    }

    pub fn mark_awaiting_signal(&mut self, signal: S) {
        self.status = WorkflowStatus_2::Open(Open_2::AwaitingSignal(signal));
    }

    pub fn mark_completed(&mut self) {
        self.status = WorkflowStatus_2::Closed(Closed_2::Completed);
        self.end_time = Some(SystemTime::now());
    }
}

// Workflow Trait
#[async_trait]
pub trait Workflow_2: Send + Sync {
    type Input: Serialize + for<'de> Deserialize<'de> + Send + Sync;
    type Output: Serialize + for<'de> Deserialize<'de> + Send + Sync;
    type Signal: Serialize + for<'de> Deserialize<'de> + Send + Sync;

    fn name(&self) -> &'static str;

    async fn run(
        &mut self,
        instance_id: &str,
        db: Arc<dyn DB_2>,
        input: Self::Input,
    ) -> Result<Self::Output, &'static str>;

    async fn await_signal(
        &self,
        db: Arc<dyn DB_2>,
        instance_id: &str,
        timeout: Duration,
    ) -> Result<Self::Signal, &'static str>;
}

// Workflow Runner
pub struct WorkflowRunner_2<W: Workflow_2> {
    workflow_factory: Box<dyn Fn() -> W + Send + Sync>,
    db: Arc<dyn DB_2>,
}

impl<W> WorkflowRunner_2<W>
where
    W: Workflow_2 + 'static,
{
    pub fn new<F>(workflow_factory: F, db: Arc<dyn DB_2>) -> Self
    where
        F: Fn() -> W + Send + Sync + 'static,
    {
        Self {
            workflow_factory: Box::new(workflow_factory),
            db,
        }
    }

    pub async fn execute(
        &self,
        instance_id: &str,
        input: W::Input,
    ) -> Result<W::Output, &'static str> {
        let mut workflow = (self.workflow_factory)();
        workflow.run(instance_id, self.db.clone(), input).await
    }

    pub async fn await_signal(
        &self,
        instance_id: &str,
        timeout: Duration,
    ) -> Result<W::Signal, &'static str> {
        let workflow = (self.workflow_factory)();
        workflow.await_signal(self.db.clone(), instance_id, timeout).await
    }
}

// CreateUserWorkflow
#[derive(Serialize, Deserialize, Debug)]
pub struct CreateUserInput_2 {
    pub name: String,
}

#[derive(Serialize, Deserialize,Debug)]
pub struct UserCreated_2 {
    pub user_id: String,
    pub name: String,
}

#[derive(Serialize, Deserialize,Debug)]
pub struct VerificationCode_2 {
    pub code: String,
}

pub struct CreateUserWorkflow_2;

#[async_trait]
impl Workflow_2 for CreateUserWorkflow_2 {
    type Input = CreateUserInput_2;
    type Output = UserCreated_2;
    type Signal = VerificationCode_2;

    fn name(&self) -> &'static str {
        "CreateUserWorkflow_2"
    }

    async fn run(
        &mut self,
        instance_id: &str,
        db: Arc<dyn DB_2>,
        input: Self::Input,
    ) -> Result<Self::Output, &'static str> {
        let user_id = format!("user_{}", instance_id);
        let output = UserCreated_2 {
            user_id,
            name: input.name,
        };
        Ok(output)
    }

    async fn await_signal(
        &self,
        _db: Arc<dyn DB_2>,
        _instance_id: &str,
        timeout: Duration,
    ) -> Result<Self::Signal, &'static str> {
        time::sleep(timeout).await;
        Ok(VerificationCode_2 {
            code: "1234".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test]
    async fn test_create_user_workflow() {
        let db = Arc::new(MockDB_2 {
            states: HashMap::new(),
        });
        
        // Initialize the workflow runner
        let runner = WorkflowRunner_2::new(|| CreateUserWorkflow_2, db.clone());

        // Create input for the workflow
        let input = CreateUserInput_2 {
            name: "Alice".to_string(),
        };

        // Execute the workflow and get the result
        let result = runner.execute("instance_1", input).await;
        
        // Assert that the execution result is as expected
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.name, "Alice");
        assert_eq!(output.user_id, "user_instance_1");

        // Await a signal and get the result
        let signal_result = runner.await_signal("instance_1", Duration::from_secs(2)).await;

        // Assert that the signal result is as expected
        assert!(signal_result.is_ok());
        let signal = signal_result.unwrap();
        assert_eq!(signal.code, "1234");
    }
}
