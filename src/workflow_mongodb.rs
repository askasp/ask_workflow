use async_trait::async_trait;
use futures::stream::TryStreamExt;
use mongodb::bson::de::from_document;
use mongodb::bson::ser::to_document;
use mongodb::{
    bson::{self, doc, Bson, DateTime as BsonDateTime, Document},
    Collection, Database,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::db_trait::WorkflowDbTrait;
use crate::workflow::WorkflowErrorType;
use crate::workflow_signal::{Signal, SignalDirection};
use crate::{
    db_trait::unique_workflow_id,
    workflow_state::{Open, WorkflowState, WorkflowStatus},
};

pub struct MongoDB {
    // db: Arc<Database>,
    workflows: Collection<Document>,
    signals: Collection<Document>,
}

impl MongoDB {
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            workflows: db.collection("workflows"),
            signals: db.collection("signals"),
            // db,
        }
    }
    fn serialize_workflow_state(state: &WorkflowState) -> Document {
        let mut doc = to_document(state).expect("Failed to serialize WorkflowState");
        doc.insert("_id", Bson::String(state.unique_id()));

        // Convert SystemTime fields to BSON DateTime
        if let Some(scheduled_at) = state.scheduled_at.duration_since(UNIX_EPOCH).ok() {
            doc.insert(
                "scheduled_at",
                Bson::DateTime(BsonDateTime::from_millis(scheduled_at.as_millis() as i64)),
            );
        }

        if let Some(start_time) = state
            .start_time
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        {
            doc.insert(
                "start_time",
                Bson::DateTime(BsonDateTime::from_millis(start_time.as_millis() as i64)),
            );
        }

        if let Some(end_time) = state
            .end_time
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        {
            doc.insert(
                "end_time",
                Bson::DateTime(BsonDateTime::from_millis(end_time.as_millis() as i64)),
            );
        }

        doc
    }
    fn deserialize_workflow_state(mut doc: Document) -> WorkflowState {
        // Convert Document to serde_json::Value
        let mut json_value: serde_json::Value =
            serde_json::to_value(&doc).expect("Failed to convert to JSON");

        // Manually set the SystemTime fields by converting them
        if let Some(Bson::DateTime(bson_dt)) = doc.remove("scheduled_at") {
            json_value["scheduled_at"] =
                serde_json::to_value(bson_datetime_to_system_time(bson_dt))
                    .expect("Failed to serialize scheduled_at");
        }
        if let Some(Bson::DateTime(bson_dt)) = doc.remove("start_time") {
            json_value["start_time"] = serde_json::to_value(bson_datetime_to_system_time(bson_dt))
                .expect("Failed to serialize start_time");
        }
        if let Some(Bson::DateTime(bson_dt)) = doc.remove("end_time") {
            json_value["end_time"] = serde_json::to_value(bson_datetime_to_system_time(bson_dt))
                .expect("Failed to serialize end_time");
        }

        // Deserialize the updated serde_json::Value into WorkflowState
        serde_json::from_value(json_value).expect("Failed to deserialize WorkflowState")
    }

    fn system_time_to_bson_datetime(time: SystemTime) -> BsonDateTime {
        let duration = time.duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO);
        BsonDateTime::from_millis(duration.as_millis() as i64)
    }

    // Convert BsonDateTime to SystemTime
    fn bson_datetime_to_system_time(bson_date: BsonDateTime) -> SystemTime {
        let timestamp = bson_date.timestamp_millis();
        UNIX_EPOCH + Duration::from_millis(timestamp as u64)
    }
}

#[async_trait]
impl WorkflowDbTrait for MongoDB {
    async fn get_all(&self) -> Vec<WorkflowState> {
        let mut cursor = self.workflows.find(doc! {}).await.unwrap();

        let mut results = Vec::new();
        println!("Getting all workflows");

        // Iterate over the cursor to process each document
        while let Some(doc) = cursor.try_next().await.unwrap() {
            // Use the custom deserialization function on each document
            let workflow_state = Self::deserialize_workflow_state(doc);
            results.push(workflow_state);
        }

        results
    }

    async fn insert(&self, state: WorkflowState) {
        let doc = Self::serialize_workflow_state(&state);
        self.workflows
            .insert_one(doc)
            .await
            .expect("Failed to insert document");
    }

    async fn get_workflow_state(
        &self,
        workflow_name: &str,
        instance_id: &str,
    ) -> Result<Option<WorkflowState>, &'static str> {
        let query = doc! {
            "_id": unique_workflow_id(workflow_name, instance_id),
        };
        let result = self
            .workflows
            .find_one(query)
            .await
            .map_err(|_| "Database query failed")?;

        // If a document is found, use `deserialize_workflow_state` to convert it
        Ok(result.map(Self::deserialize_workflow_state))
    }

    async fn update(&self, state: WorkflowState) {
        let query = doc! {
            "_id": state.unique_id(),
        };
        let update = Self::serialize_workflow_state(&state);

        self.workflows
            .replace_one(query, update)
            .await
            .expect("Failed to update document");
    }
    async fn query_due(&self, now: SystemTime) -> Vec<WorkflowState> {
        let bson_now = MongoDB::system_time_to_bson_datetime(now);

        let due_query = doc! {
            "scheduled_at": { "$lte": bson_now },
            "status.Open": "Running",
        };

        let mut cursor = self.workflows.find(due_query).await.unwrap();

        let mut results = Vec::new();

        // Iterate over the cursor to process each document
        while let Some(doc) = cursor.try_next().await.unwrap() {
            // Use the custom deserialization function on each document
            let workflow_state = Self::deserialize_workflow_state(doc);
            results.push(workflow_state);
        }

        results
    }

    async fn set_signal_processed(&self, signal: Signal) -> Result<(), WorkflowErrorType> {
        let mut signal_clone = signal.clone();
        signal_clone.processed = true;

        let query = doc! {
            "_id": signal.id
        };

        let doc = to_document(&signal_clone).map_err(|e| WorkflowErrorType::TransientError {
            message: format!("Serialization failed: {}", e),
            content: None,
        })?;

        self.signals.replace_one(query, doc).await.map_err(|e| {
            WorkflowErrorType::TransientError {
                message: format!("Failed to update signal: {}", e),
                content: None,
            }
        })?;

        Ok(())
    }

    async fn insert_signal(&self, signal: Signal) -> Result<(), WorkflowErrorType> {
        let mut doc = to_document(&signal).map_err(|e| WorkflowErrorType::TransientError {
            message: format!("Serialization failed: {}", e),
            content: None,
        })?;

        doc.insert("_id", signal.id);

        self.signals
            .insert_one(doc)
            .await
            .map_err(|e| WorkflowErrorType::TransientError {
                message: format!("Failed to insert signal: {}", e),
                content: None,
            })?;
        Ok(())
    }

    async fn get_signals(
        &self,
        workflow_name: &str,
        instance_id: &str,
        signal_name: &str,
        direction: SignalDirection,
    ) -> Result<Option<Vec<Signal>>, WorkflowErrorType> {
        let query = doc! { "processed": false, "workflow_name": workflow_name, "instance_id": instance_id, "signal_name": signal_name, "direction": direction.to_string() };
        let result =
            self.signals
                .find_one(query)
                .await
                .map_err(|e| WorkflowErrorType::TransientError {
                    message: format!("Failed to retrieve signal: {}", e),
                    content: None,
                })?;
        Ok(result.and_then(|doc| from_document(doc).ok()))
    }
}

fn bson_datetime_to_system_time(bson_date: BsonDateTime) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(bson_date.timestamp_millis() as u64)
}
