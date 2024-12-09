use std::{collections::HashMap, sync::Mutex};

use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct User {
    pub id: String,
    pub name: String,
}

pub struct MockDatabase {
    users: Mutex<HashMap<String, User>>, // Replace `User` with your actual struct
    counter: Mutex<u64>,
}

impl MockDatabase {
    pub fn new() -> Self {
        Self {
            users: Mutex::new(HashMap::new()),
            counter: Mutex::new(0),
        }
    }

    pub fn insert_user(&self, user: User) {
        self.users.lock().unwrap().insert(user.id.clone(), user);
    }

    pub fn get_user(&self, user_id: &str) -> Option<User> {
        self.users.lock().unwrap().get(user_id).cloned()
    }
    pub fn get_counter(&self) -> u64 {
        *self.counter.lock().unwrap()
    }
    pub fn increment_counter(&self) {
        *self.counter.lock().unwrap() += 1;
    }
}
