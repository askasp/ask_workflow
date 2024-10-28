use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Toast {
    pub variant: String,
    pub title: String,
}
