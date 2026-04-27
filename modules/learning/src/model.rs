use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Book {
    pub doc_id: String,
    pub name: String,
    pub author: Option<String>,
    pub status: String, // reading | done | todo
    pub progress: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub doc_id: String,
    pub title: String,
    pub body: Option<String>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Course {
    pub doc_id: String,
    pub name: String,
    pub provider: Option<String>,
    pub progress: f64,
    pub due_on: Option<String>,
    pub tone: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningData {
    pub books: Vec<Book>,
    pub notes: Vec<Note>,
    pub courses: Vec<Course>,
}
