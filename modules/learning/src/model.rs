use serde::{Deserialize, Serialize};

// `lrn_book` / `lrn_note` / `lrn_course` columns match these structs
// field-for-field, so the server-only `sqlx::FromRow` derive lets full-row
// `SELECT`s decode straight into the model without hand-written tuple mapping.

#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Book {
    pub doc_id: String,
    pub name: String,
    pub author: Option<String>,
    pub status: String, // reading | done | todo
    pub progress: f64,
}

#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub doc_id: String,
    pub title: String,
    pub body: Option<String>,
    pub updated_at: i64,
}

#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
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
    pub summary: LearningSummary,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LearningSummary {
    pub notes_30d: u32,
    pub books_done: u32,
    pub books_reading: u32,
    pub books_todo: u32,
    /// Mean progress across non-archived courses, 0..1.
    pub courses_avg_progress: f32,
    /// 28-day note density: 4 weeks × 7 days, oldest week first, value 0..4.
    /// Feeds directly into `<Heatmap>`.
    pub note_heatmap_28d: Vec<u8>,
}
