use async_trait::async_trait;
use chrono::DateTime;
use futures::stream::StreamExt;
use futures::stream::TryStreamExt;
use mongodb::bson::de::from_document;
use mongodb::bson::ser::to_document;
use mongodb::Cursor;
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
use crate::workflow_state::{Open, WorkflowState, WorkflowStatus};

pub struct MongoDB {
    // db: Arc<Database>,
    workflows: Collection<Document>,
    signals: Collection<Document>,
}

impl MongoDB {
    pub fn new(db: Database) -> Self {
        Self {
            workflows: db.collection("workflows"),
            signals: db.collection("signals"),
            // db,
        }
    }
    fn serialize_workflow_state(state: &WorkflowState) -> Document {
        let mut doc = to_document(state).expect("Failed to serialize WorkflowState");
        doc.insert("_id", Bson::String(state.run_id.clone()));

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

    async fn insert(&self, state: WorkflowState) -> String {
        let doc = Self::serialize_workflow_state(&state);
        let res = self
            .workflows
            .insert_one(doc)
            .await
            .expect("Failed to insert document");

        res.inserted_id.to_string()
    }
    async fn get_running_workflows(
        &self,
        workflow_name: &str,
        instance_id: &str,
    ) -> Result<Vec<WorkflowState>, WorkflowErrorType> {
        let query = doc! {
            "workflow_type": workflow_name,
            "instance_id": instance_id,
            "status.Open": "Running",
        };

        let cursor =
            self.workflows
                .find(query)
                .await
                .map_err(|e| WorkflowErrorType::TransientError {
                    message: format!("Query failed: {:?}", e),
                    content: None,
                })?;

        let mut workflows = Vec::new();
        let results = cursor.try_collect::<Vec<_>>().await.map_err(|e| {
            WorkflowErrorType::TransientError {
                message: format!("Error while fetching workflows: {:?}", e),
                content: None,
            }
        })?;

        for doc in results {
            workflows.push(Self::deserialize_workflow_state(doc));
        }

        Ok(workflows)
    }

    async fn get_workflow_state(
        &self,
        run_id: &str,
    ) -> Result<Option<WorkflowState>, WorkflowErrorType> {
        let query = doc! {
            "_id": run_id,
        };
        let result = self.workflows.find_one(query).await.map_err(|_| {
            WorkflowErrorType::TransientError {
                message: "Query failed".to_string(),
                content: None,
            }
        })?;

        // If a document is found, use `deserialize_workflow_state` to convert it
        Ok(result.map(Self::deserialize_workflow_state))
    }

    async fn update(&self, state: WorkflowState) {
        let query = doc! {
            "_id": state.run_id.clone(),

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

    async fn update_signal(&self, signal: Signal) -> Result<(), WorkflowErrorType> {
        let query = doc! {
            "_id": signal.id.clone()
        };
        let doc = to_document(&signal.clone()).map_err(|e| WorkflowErrorType::TransientError {
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

    async fn get_all_signals(&self) -> Result<Vec<Signal>, WorkflowErrorType> {
        let cursor =
            self.signals
                .find(doc! {})
                .await
                .map_err(|e| WorkflowErrorType::TransientError {
                    message: format!("Failed to retrieve signals: {}", e),
                    content: None,
                })?; // Return an error if the query fails
                     //
        let results: Vec<Signal> = cursor
            .filter_map(|doc| async {
                match doc {
                    Ok(doc) => match from_document::<Signal>(doc) {
                        Ok(signal) => Some(signal),
                        Err(e) => {
                            eprintln!("Failed to deserialize signal: {}", e); // log the error
                            None
                        }
                    },
                    Err(e) => {
                        eprintln!("Error reading document from cursor: {}", e); // log the error
                        None
                    }
                }
            })
            .collect()
            .await;

        Ok(results)
    }

    async fn get_signals(
        &self,
        workflow_name: &str,
        instance_id: &str,
        signal_name: &str,
        direction: SignalDirection,
        accept_processed: bool,
    ) -> Result<Option<Vec<Signal>>, WorkflowErrorType> {
        let mut query = doc! {"workflow_name": workflow_name, "instance_id": instance_id, "signal_name": signal_name, "direction": direction.to_string() };
        if !accept_processed {
            query.insert("processed", false);
        }
        tracing::debug!("Query: {:?}", query);
        let cursor =
            self.signals
                .find(query)
                .await
                .map_err(|e| WorkflowErrorType::TransientError {
                    message: format!("Failed to retrieve signal: {}", e),
                    content: None,
                })?;
        let results: Vec<Signal> = cursor
            .filter_map(|doc| async {
                match doc {
                    Ok(doc) => match from_document::<Signal>(doc) {
                        Ok(signal) => Some(signal),
                        Err(e) => {
                            eprintln!("Failed to deserialize signal: {}", e); // log the error
                            None
                        }
                    },
                    Err(e) => {
                        eprintln!("Error reading document from cursor: {}", e); // log the error
                        None
                    }
                }
            })
            .collect()
            .await;

        Ok(Some(results))

        //
        // println!("Result: {:?}", results);
        // let res = results
        //     .map(|doc| from_document(doc).ok())
        //     .collect::<Result<Vec<Signal>, _>>()
        //     .map_err(|e| WorkflowErrorType::TransientError {
        //     message: format!("Failed to deserialize signal: {}", e),
        //     content: None,
        //     })?;

        // Ok(results.and_then(|doc| from_document(doc).ok()))
    }
    async fn cancel_signals(
        &self,
        workflow_name: &str,
        instance_id: &str,
    ) -> Result<(), WorkflowErrorType> {
        // MongoDB query to match signals
        let filter = doc! {
            "workflow_name": workflow_name,
            "instance_id": instance_id,
            "processed": false, // Only update unprocessed signals
        };

        // MongoDB update to mark signals as processed
        let update = doc! {
            "$set": {
                "processed": true,
                "processed_at": mongodb::bson::DateTime::now(),
            }
        };

        // Execute the update query
        self.signals
            .update_many(filter, update)
            .await
            .map_err(|e| WorkflowErrorType::TransientError {
                message: format!("Failed to mark signals as processed: {}", e),
                content: None,
            })?;

        Ok(())
    }
    async fn get_by_cursor(
        &self,
        cursor: Option<String>,
        limit: usize,
        reverse: bool,
    ) -> Result<Vec<WorkflowState>, WorkflowErrorType> {
        let mut query = doc! {};

        // Use `updated_at` as the cursor for pagination
        if let Some(mut cursor_timestamp) = cursor {
            println!("Cursor timestamp: {:?}", cursor_timestamp);
            let sanitized_cursor = cursor_timestamp.replace(' ', "+"); // Replace spaces with "+"

            let datetime = DateTime::parse_from_rfc3339(&sanitized_cursor).map_err(|e| {
                WorkflowErrorType::PermanentError {
                    message: format!("cant parse rfc to datetime {:?}", e),
                    content: None,
                }
            })?;
            let cursor_time = SystemTime::from(datetime);
            // Convert `SystemTime` to MongoDB-friendly structure
            let secs = cursor_time
                .duration_since(UNIX_EPOCH)
                .map_err(|e| WorkflowErrorType::TransientError {
                    message: format!("Invalid SystemTime: {}", e),
                    content: None,
                })?
                .as_secs();

            // Adjust query based on sort order
            if reverse {
                query.insert("updated_at.secs_since_epoch", doc! { "$gt": secs as i64 });
            } else {
                query.insert("updated_at.secs_since_epoch", doc! { "$lte": secs as i64 });
            }
        }
        let sort_order = if reverse { 1 } else { -1 };

        let cursor = self
            .workflows
            .find(query)
            .sort(doc! { "updated_at.secs_since_epoch": sort_order })
            .limit(limit as i64)
            .await
            .map_err(|e| WorkflowErrorType::TransientError {
                message: format!("Failed to query workflows: {}", e),
                content: None,
            })?;

        let results: Vec<WorkflowState> = cursor
            .filter_map(|doc| async {
                match doc {
                    Ok(doc) => Some(Self::deserialize_workflow_state(doc)),
                    Err(e) => {
                        eprintln!("Error reading document from cursor: {}", e);
                        None
                    }
                }
            })
            .collect()
            .await;

        Ok(results)
    }
    async fn get_signals_for_workflows(
        &self,
        workflow_ids: Vec<String>,
    ) -> Result<Vec<Signal>, WorkflowErrorType> {
        let query = doc! {
            "instance_id": { "$in": workflow_ids },
        };

        let cursor =
            self.signals
                .find(query)
                .await
                .map_err(|e| WorkflowErrorType::TransientError {
                    message: format!("Failed to query signals: {}", e),
                    content: None,
                })?;

        let results: Vec<Signal> = cursor
            .filter_map(|doc| async {
                match doc {
                    Ok(doc) => match from_document::<Signal>(doc) {
                        Ok(signal) => Some(signal),
                        Err(e) => {
                            eprintln!("Failed to deserialize signal: {}", e);
                            None
                        }
                    },
                    Err(e) => {
                        eprintln!("Error reading document from cursor: {}", e);
                        None
                    }
                }
            })
            .collect()
            .await;

        Ok(results)
    }
}

fn bson_datetime_to_system_time(bson_date: BsonDateTime) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(bson_date.timestamp_millis() as u64)
}
