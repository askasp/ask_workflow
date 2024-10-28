use std::{collections::HashMap, time::SystemTime};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::workflow_state::WorkflowState;

pub struct WorkflowContext {
    pub workflows: Vec<WorkflowView>,
    pub names: Vec<String>
}
impl WorkflowContext {
    pub fn add_to_context(&self, context: &mut tera::Context){
        context.insert("workflows", &self.workflows);
        context.insert("names", &self.names);
    }
}


#[derive(Serialize, Deserialize)]
pub struct WorkflowView {
    pub id: String,
    pub name: String,
    pub status: String,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub results: HashMap<String, serde_json::Value>,
}
impl WorkflowView {
    pub fn new(workflow: &WorkflowState) -> WorkflowView {
        WorkflowView {
            name: workflow.workflow_type.clone(),
            id: workflow.instance_id.clone(),
            results: workflow.results.clone(),
            status: workflow.status.to_string(),
            end_time: workflow.end_time.map(|time| system_time_to_string(time)),
            start_time: workflow.start_time.map(|time| system_time_to_string(time)),
        }
    }
}


fn system_time_to_string(time: SystemTime) -> String {
    let datetime: DateTime<Utc> = time.into();
    datetime.format("%Y-%m-%d %H:%M:%S").to_string()
}
