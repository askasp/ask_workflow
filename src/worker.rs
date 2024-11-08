use cuid::cuid1;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::oneshot;
use tokio::time::sleep;

use crate::db_trait::WorkflowDbTrait;
use crate::workflow::{Workflow, WorkflowErrorType};
use crate::workflow_signal::{Signal, SignalDirection, WorkflowSignal};
use crate::workflow_state::{Closed, WorkflowError, WorkflowState, WorkflowStatus};
use std::borrow::BorrowMut;
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use std::any::TypeId;

pub struct Worker {
    pub db: Arc<dyn WorkflowDbTrait>,

    // pub workflows: HashMap<TypeId, Box<dyn Fn(WorkflowState) -> Box<dyn Workflow + Send + Sync> + Send + Sync>>,
    pub workflows: HashMap<
        String,
        Box<dyn Fn(WorkflowState) -> Box<dyn Workflow + Send + Sync> + Send + Sync>,
    >,
}

impl Worker {
    // Create a new Worker with the provided database
    pub fn new(db: Arc<dyn WorkflowDbTrait>) -> Self {
        Self {
            db,
            workflows: HashMap::new(),
        }
    }
    pub fn workflow_keys(&self) -> Vec<String> {
        self.workflows.keys().cloned().collect()
    }

    pub fn add_workflow<W, F>(&mut self, factory: F)
    where
        W: Workflow + 'static,
        F: Fn(WorkflowState) -> Box<dyn Workflow + Send + Sync> + Send + Sync + 'static,
    {
        let workflow_name = W::static_name().to_string();
        self.workflows.insert(workflow_name, Box::new(factory));
    }
    // Schedule a workflow to run immediately

    pub fn workflow_names(&self) -> Vec<String> {
        self.workflows.keys().cloned().collect()
    }

    pub async fn schedule_now_with_name(
        &self,
        workflow_name: &str,
        instance_id: &str,
        input: Option<Value>,
    ) -> Result<(), &'static str> {
        self.schedule_with_name(workflow_name, instance_id, SystemTime::now(), input)
            .await
    }

    pub async fn schedule_now<W, T>(
        &self,
        instance_id: &str,
        input: Option<T>,
    ) -> Result<String, &'static str>
    where
        W: Workflow + Send + Sync + 'static,
        T: Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static,
    {
        self.schedule::<W, T>(instance_id, SystemTime::now(), input)
            .await
    }

    // Schedule a workflow to run at a specific time
    //
    pub async fn schedule<W, T>(
        &self,
        instance_id: &str,
        scheduled_at: SystemTime,
        input: Option<T>,
    ) -> Result<String, &'static str>
    where
        W: Workflow + Send + Sync + 'static,
        T: Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static,
    {
        if let Some(_workflow_factory) = self.workflows.get(W::static_name()) {
            let workflow_state = WorkflowState::new(
                W::static_name(),
                instance_id,
                scheduled_at,
                input.map(|i| serde_json::to_value(i).unwrap()),
            );
            let res = self.db.insert(workflow_state.clone()).await;

            Ok(res)
        } else {
            Err("Workflow not found")
        }
    }

    pub async fn schedule_with_name(
        &self,
        workflow_name: &str,
        instance_id: &str,
        scheduled_at: SystemTime,
        input: Option<Value>,
    ) -> Result<(), &'static str> {
        if let Some(_workflow_factory) = self.workflows.get(workflow_name) {
            let workflow_state =
                WorkflowState::new(workflow_name, instance_id, scheduled_at, input);
            self.db.insert(workflow_state.clone()).await;

            Ok(())
        } else {
            Err("Workflow not found")
        }
    }

    pub async fn execute<W, T>(
        &self,
        instance_id: &str,
        input: Option<T>,
        timeout: Duration,
    ) -> Result<WorkflowState, WorkflowErrorType>
    where
        W: Workflow + Send + Sync + 'static,
        T: Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static,
    {
        // Schedule the workflow immediately
        let res = self
            .schedule_now::<W, T>(instance_id, input)
            .await
            .map_err(|_| WorkflowErrorType::PermanentError {
                message: "Cant schedule wf".to_string(),
                content: None,
            })?;

        self.await_workflow::<W>(&res, timeout, 100).await
    }

    pub async fn await_workflow<W>(
        &self,
        run_id: &str,
        timeout: Duration,
        interval_millis: u64,
    ) -> Result<WorkflowState, WorkflowErrorType>
    where
        W: Workflow + Send + Sync + 'static,
    {
        // Create a oneshot channel to send the result when found
        let (sender, receiver) = oneshot::channel();
        let run_id = run_id.to_string();
        let db = self.db.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(interval_millis));
            let start = tokio::time::Instant::now();

            loop {
                interval.tick().await;

                // Timeout check
                if start.elapsed() >= timeout {
                    let _ = sender.send(Err(WorkflowErrorType::TransientError {
                        message: "Timeout waiting for result".to_string(),
                        content: None,
                    }));
                    break;
                }
                if let Ok(Some(workflow_state)) = db.get_workflow_state(&run_id).await {
                    match workflow_state.clone().status {
                        WorkflowStatus::Closed(Closed::Completed) => {
                            let _ = sender.send(Ok(workflow_state.clone()));
                            break;
                        }
                        WorkflowStatus::Closed(Closed::Failed { error: e }) => {
                            let _ = sender.send(Err(e));
                            break;
                        }
                        _ => {}
                    }
                } else {
                    eprintln!("Failed to fetch workflow state or activity result");
                }
            }
        });

        receiver
            .await
            .map_err(|_| WorkflowErrorType::TransientError {
                message: "Failed to await workflow".to_string(),
                content: None,
            })?
    }
    pub async fn send_signal<S: WorkflowSignal>(
        &self,
        instance_id: &str,
        signal: S,
    ) -> Result<(), WorkflowErrorType> {
        let signal_data = Signal {
            id: cuid::cuid1().unwrap(),
            instance_id: instance_id.to_string(),
            signal_name: S::static_signal_name().to_string(),
            processed: false,
            direction: S::direction(),
            workflow_name: S::Workflow::static_name().to_string(),
            timestamp: SystemTime::now(),
            data: serde_json::to_value(&signal).unwrap(),
        };
        self.db.insert_signal(signal_data).await
    }

    // Poll the database to retrieve and deserialize the entire signal object
    pub async fn await_signal<S: WorkflowSignal>(
        &self,
        instance_id: &str,
        timeout: Duration,
    ) -> Result<S, WorkflowErrorType> {
        let start = tokio::time::Instant::now();

        loop {
            if start.elapsed() >= timeout {
                return Err(WorkflowErrorType::PermanentError {
                    message: "Signal timeout reached".to_string(),
                    content: None,
                });
            }

            if let Ok(Some(mut signals)) = self
                .db
                .get_signals(
                    S::Workflow::static_name(),
                    instance_id,
                    S::static_signal_name(),
                    S::direction(),
                )
                .await
            {
                if signals.len() == 0 {
                    sleep(Duration::from_millis(200)).await;
                    continue;
                }
                signals.sort_by_key(|signal| signal.timestamp);
                let oldest_signal = signals[0].clone(); // The fir

                self.db.set_signal_processed(oldest_signal.clone()).await?;
                let data: S = serde_json::from_value(oldest_signal.data).map_err(|e| {
                    WorkflowErrorType::PermanentError {
                        message: "Failed to deserialize signal".to_string(),
                        content: None,
                    }
                })?;
                return Ok(data);
            }

            sleep(Duration::from_millis(200)).await;
        }
    }

    pub async fn run(self: Arc<Self>, interval_millis: u64) {
        let mut interval = tokio::time::interval(Duration::from_millis(interval_millis));
        loop {
            interval.tick().await;
            let now = SystemTime::now();

            let due_workflows = self.db.query_due(now).await;

            for workflow_state in due_workflows {
                if let Some(workflow_factory) = self.workflows.get(&workflow_state.workflow_type) {
                    let db = Arc::clone(&self.db);

                    let mut workflow = workflow_factory(workflow_state.clone());
                    let me = self.clone();

                    tokio::spawn(async move {
                        let res = workflow.execute(db, me.clone()).await;
                        if !res.is_ok() {
                            eprintln!("Error executing workflow: {:?}", res);
                        }
                    });
                }
            }
        }
    }
    pub async fn run_activity<T, F, Fut>(
        &self,
        name: &str,
        state: &mut WorkflowState,
        func: F,
    ) -> Result<T, WorkflowErrorType>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, WorkflowErrorType>>,
        T: Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static,
    {
        // Check if the activity is already completed
        if let Some(result) = state.get_activity_result(name) {
            let cached_result: T = serde_json::from_value(result.clone()).unwrap();
            return Ok(cached_result);
        }

        // Run the function and await its result
        match func().await {
            Ok(result) => {
                state.add_activity_result(name, &result);
                self.db.update(state.clone()).await;
                Ok(result)
            }
            Err(e) => {
                state.errors.push(WorkflowError {
                    error_type: e.clone(),
                    activity_name: Some(name.to_string()),
                    timestamp: SystemTime::now(),
                });
                self.db.update(state.clone()).await;
                Err(e)
            }
        }
    }

    pub async fn run_sync_activity<T, F>(
        &self,
        name: &str,
        state: &mut WorkflowState,
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
                self.db.update(state.clone()).await; // Await the async update call
                Ok(result)
            }
            Err(e) => {
                state.errors.push(WorkflowError {
                    error_type: e.clone(),
                    activity_name: Some(name.to_string()),
                    timestamp: SystemTime::now(),
                });
                self.db.update(state.clone()).await; // Await the async update call
                Err(e)
            }
        }
    }
}

// Function to schedule a workflow to run immediately

// Run the worker, periodically checking for due workflows and executing them
//
//
