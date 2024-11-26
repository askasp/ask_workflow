use std::{collections::HashMap, time::SystemTime};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    workflow_signal::{Signal, SignalDirection},
    workflow_state::{ActivityResult, WorkflowError, WorkflowState},
};

pub struct WorkflowContext {
    pub workflows: Vec<WorkflowView>,
    pub names: Vec<String>,
    pub prev_cursor: Option<String>,
    pub next_cursor: Option<String>,
    pub limit: usize, // New field for the limit
}
impl WorkflowContext {
    pub fn add_to_context(&self, context: &mut tera::Context) {
        context.insert("workflows", &self.workflows);
        context.insert("names", &self.names);
        context.insert("prev_cursor", &self.prev_cursor);
        context.insert("next_cursor", &self.next_cursor);
        context.insert("limit", &self.limit); // Add the limit to the context
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum WorkflowAction {
    SentSignal(SignalView),
    ReceivedSignal(SignalView),
    Activity(ActivityView),
    Error(WorkflowError),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ActivityView {
    pub name: String,
    pub data: String,      // Store the result data as a string for easy display
    pub timestamp: String, // Format timestamp as a string
}

impl ActivityView {
    pub fn new(activity: &ActivityResult) -> ActivityView {
        ActivityView {
            name: activity.name.clone(),
            data: serde_json::to_string_pretty(&activity.data).unwrap_or_else(|_| "{}".to_string()),
            timestamp: system_time_to_string(activity.timestamp),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SignalView {
    pub signal_name: String,
    pub direction: SignalDirection,
    pub payload: String,
    pub sent_at: String,
    pub processed_at: Option<String>,
    // Add original timestamps for sorting purposes (not serialized)
}

impl SignalView {
    pub fn new(signal: &Signal) -> SignalView {
        SignalView {
            signal_name: signal.signal_name.clone(),
            direction: signal.direction.clone(),
            payload: serde_json::to_string_pretty(&signal.data)
                .unwrap_or_else(|_| "{}".to_string()),
            sent_at: system_time_to_string(signal.sent_at),
            processed_at: signal.processed_at.map(|time| system_time_to_string(time)),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WorkflowView {
    pub run_id: String,
    pub instance_id: String,
    pub name: String,
    pub status: String,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub actions: Vec<WorkflowAction>,
    pub updated_at: Option<String>,
    pub scheduled_at: Option<String>,
}
impl WorkflowView {
    pub fn new(workflow: &WorkflowState, filtered_signals: &[Signal]) -> WorkflowView {
        let mut actions: Vec<WorkflowAction> = Vec::new();

        // Add Sent and Received signals as actions
        for signal in filtered_signals {
            let action = match signal.direction {
                SignalDirection::FromWorkflow => {
                    WorkflowAction::SentSignal(SignalView::new(&signal))
                }
                SignalDirection::ToWorkflow => {
                    WorkflowAction::ReceivedSignal(SignalView::new(&signal))
                }
            };
            actions.push(action);
        }

        // Add activities as actions
        for (name, activity) in workflow.results.clone() {
            actions.push(WorkflowAction::Activity(ActivityView::new(&activity)));
        }

        // Add errors as actions
        for error in workflow.errors.clone() {
            actions.push(WorkflowAction::Error(error));
        }

        // Sort actions by their respective timestamps
        actions.sort_by_key(|action| match action {
            WorkflowAction::SentSignal(signal) => signal.sent_at.clone(),
            WorkflowAction::ReceivedSignal(signal) => signal
                .processed_at
                .clone()
                .unwrap_or(signal.sent_at.clone()),
            WorkflowAction::Activity(activity) => activity.timestamp.clone(),
            WorkflowAction::Error(error) => system_time_to_string(error.timestamp),
        });

        WorkflowView {
            name: workflow.workflow_type.clone(),
            run_id: workflow.run_id.clone(),
            instance_id: workflow.instance_id.clone(),
            status: workflow.status.to_string(),
            start_time: workflow.start_time.map(|time| system_time_to_string(time)),
            end_time: workflow.end_time.map(|time| system_time_to_string(time)),
            actions,
            updated_at: workflow.updated_at.map(|time| system_time_to_string(time)),
            scheduled_at: Some(system_time_to_string(workflow.scheduled_at)),
        }
    }
}

fn system_time_to_string(time: SystemTime) -> String {
    let datetime: DateTime<Utc> = time.into();
    datetime.format("%Y-%m-%d %H:%M:%S").to_string()
}
