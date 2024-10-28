use std::{collections::HashMap, sync::Mutex};


#[derive(Clone)]
pub struct User{
    id: String,
}

struct MockDatabase {
    users: Mutex<HashMap<String, User>>, // Replace `User` with your actual struct
}

impl MockDatabase {
    fn new() -> Self {
        Self {
            users: Mutex::new(HashMap::new()),
        }
    }

    fn insert_user(&self, user_id: String, user: User) {
        self.users.lock().unwrap().insert(user_id, user);
    }

    fn get_user(&self, user_id: &str) -> Option<User> {
        self.users.lock().unwrap().get(user_id).cloned()
    }
}
