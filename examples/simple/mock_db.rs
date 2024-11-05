use std::{collections::HashMap, sync::Mutex};

use serde::{Deserialize, Serialize};


#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct User{
    pub id: String,
    pub name: String,
}

pub struct MockDatabase {
    users: Mutex<HashMap<String, User>>, // Replace `User` with your actual struct
}

impl MockDatabase {
    pub fn new() -> Self {
        Self {
            users: Mutex::new(HashMap::new()),
        }
    }

    pub fn insert_user(&self, user: User) {
        self.users.lock().unwrap().insert(user.id.clone(), user);
    }

    pub fn get_user(&self, user_id: &str) -> Option<User> {
        self.users.lock().unwrap().get(user_id).cloned()
    }
}
