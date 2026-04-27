use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workout {
    pub doc_id: String,
    pub occurred_at: i64,
    pub kind: String,
    pub program: Option<String>,
    pub duration_m: i64,
    pub load_text: Option<String>,
    pub strain: Option<String>,
    pub rpe: Option<i64>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewWorkout {
    pub kind: String,
    pub program: Option<String>,
    pub duration_m: i64,
    pub load_text: Option<String>,
    pub strain: Option<String>,
    pub notes: Option<String>,
}
