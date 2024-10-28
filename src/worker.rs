use serde_json::Value;
use tokio::sync::oneshot;

use crate::db_trait::DB;
use crate::workflow::{Workflow, WorkflowErrorType};
use crate::workflow_state::WorkflowState;
use std::borrow::BorrowMut;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

pub struct Worker {
    pub db: Arc<dyn DB>,
    pub workflows: HashMap<
        String,
        Box<dyn Fn(WorkflowState) -> Box<dyn Workflow + Send + Sync> + Send + Sync>,
    >,
}

impl Worker {
    // Create a new Worker with the provided database
    pub fn new(db: Arc<dyn DB>) -> Self {
        Self {
            db,
            workflows: HashMap::new(),
        }
    }
    pub fn workflow_names(&self) -> Vec<String> {
        self.workflows.keys().cloned().collect()
    }

    pub fn add_workflow<F>(&mut self, name: &str, factory: F)
    where
        F: Fn(WorkflowState) -> Box<dyn Workflow + Send + Sync> + Send + Sync + 'static,
    {
        self.workflows.insert(name.to_string(), Box::new(factory));
    }
    // Schedule a workflow to run immediately
    pub async fn schedule_now(
        &self,
        workflow_name: &str,
        instance_id: &str,
        input: Option<Value>,
    ) -> Result<(), &'static str> {
        self.schedule(workflow_name, instance_id, SystemTime::now())
            .await
    }

    // Schedule a workflow to run at a specific time
    pub async fn schedule(
        &self,
        workflow_name: &str,
        instance_id: &str,
        scheduled_at: SystemTime,
    ) -> Result<(), &'static str> {
        if let Some(_workflow_factory) = self.workflows.get(workflow_name) {
            let workflow_state = WorkflowState::new(workflow_name, instance_id, scheduled_at);
            self.db.insert(workflow_state.clone()).await;

            Ok(())
        } else {
            Err("Workflow not found")
        }
    }

    pub async fn run(&self, interval_secs: u64) {
        let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
        loop {
            interval.tick().await;
            let now = SystemTime::now();
            let due_workflows = self.db.query_due(now).await;
            for workflow_state in due_workflows {
                if let Some(workflow_factory) = self.workflows.get(&workflow_state.workflow_type) {
                    let db = Arc::clone(&self.db);

                    let mut workflow = workflow_factory(workflow_state.clone());

                    tokio::spawn(async move {
                        let res = workflow.execute(db).await;
                        if !res.is_ok() {
                            eprintln!("Error executing workflow: {:?}", res);
                        }
                    });
                }
            }
        }
    }

    // Function to schedule a workflow to run immediately
}

// Run the worker, periodically checking for due workflows and executing them
