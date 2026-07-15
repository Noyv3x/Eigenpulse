use ep_core::{IconKind, ModuleDescriptor};
use serde::{Deserialize, Serialize};

pub const DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    slug: "fitness",
    route: "/fitness",
    name_key: "fitness.module.name",
    description_key: "fitness.module.description",
    icon: IconKind::Fitness,
    read_scope: crate::SCOPE_READ,
    write_scope: crate::SCOPE_WRITE,
    read_scope_label_key: "app.settings.security.scope.fit_read",
    write_scope_label_key: "app.settings.security.scope.fit_write",
};

pub const MAX_EXERCISE_MEDIA: i64 = 12;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UnitSystem {
    #[default]
    Metric,
    Imperial,
}

impl UnitSystem {
    pub fn from_storage(value: &str) -> Option<Self> {
        match value {
            "metric" => Some(Self::Metric),
            "imperial" => Some(Self::Imperial),
            _ => None,
        }
    }

    pub const fn weight_symbol(self) -> &'static str {
        match self {
            Self::Metric => "kg",
            Self::Imperial => "lb",
        }
    }

    pub const fn waist_symbol(self) -> &'static str {
        match self {
            Self::Metric => "cm",
            Self::Imperial => "in",
        }
    }

    pub const fn as_storage(self) -> &'static str {
        match self {
            Self::Metric => "metric",
            Self::Imperial => "imperial",
        }
    }
}

#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FitnessSettings {
    pub unit_system: String,
    pub weekly_workout_target: i64,
    pub weekly_cardio_minutes_target: i64,
    pub updated_at: i64,
}

#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exercise {
    pub id: i64,
    pub name: String,
    pub category: String,
    pub tracking_mode: String,
    pub primary_muscle: Option<String>,
    pub equipment: Option<String>,
    pub notes: Option<String>,
    pub archived: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExerciseMedia {
    pub id: i64,
    pub exercise_id: i64,
    pub object_key: String,
    pub title: Option<String>,
    pub media_type: String,
    pub byte_size: i64,
    pub sha256: String,
    pub sort_order: i64,
    pub created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExerciseDetail {
    pub exercise: Exercise,
    pub media: Vec<ExerciseMedia>,
}

#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub id: i64,
    pub name: String,
    pub notes: Option<String>,
    pub archived: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanExercise {
    pub id: i64,
    pub plan_id: i64,
    pub exercise_id: i64,
    pub exercise_name: String,
    pub tracking_mode: String,
    pub position: i64,
    pub notes: Option<String>,
}

#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanSet {
    pub id: i64,
    pub plan_exercise_id: i64,
    pub position: i64,
    pub target_reps: Option<i64>,
    pub target_weight_g: Option<i64>,
    pub target_duration_s: Option<i64>,
    pub target_distance_m: Option<i64>,
    pub target_rpe_x10: Option<i64>,
    pub set_type: String,
    pub rest_seconds: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanExerciseDetail {
    pub exercise: PlanExercise,
    pub sets: Vec<PlanSet>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanDetail {
    pub plan: Plan,
    pub exercises: Vec<PlanExerciseDetail>,
}

#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workout {
    pub id: i64,
    pub plan_id: Option<i64>,
    pub plan_name_snapshot: Option<String>,
    pub status: String,
    /// Persisted YYYY-MM-DD business date. Unlike the UTC instants below,
    /// this value does not change when the application timezone changes.
    pub workout_date: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub paused_at: Option<i64>,
    pub paused_seconds: i64,
    pub revision: i64,
    pub notes: Option<String>,
}

#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkoutExercise {
    pub id: i64,
    pub workout_id: i64,
    pub exercise_id: Option<i64>,
    pub exercise_name_snapshot: String,
    pub tracking_mode_snapshot: String,
    pub position: i64,
    pub notes: Option<String>,
}

#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkoutSet {
    pub id: i64,
    pub workout_exercise_id: i64,
    pub position: i64,
    pub target_reps: Option<i64>,
    pub target_weight_g: Option<i64>,
    pub target_duration_s: Option<i64>,
    pub target_distance_m: Option<i64>,
    pub target_rpe_x10: Option<i64>,
    pub actual_reps: Option<i64>,
    pub actual_weight_g: Option<i64>,
    pub actual_duration_s: Option<i64>,
    pub actual_distance_m: Option<i64>,
    pub actual_rpe_x10: Option<i64>,
    pub set_type: String,
    pub status: String,
    pub rest_seconds: i64,
    pub completed_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkoutExerciseDetail {
    pub exercise: WorkoutExercise,
    pub media: Vec<ExerciseMedia>,
    pub sets: Vec<WorkoutSet>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkoutDetail {
    pub workout: Workout,
    pub exercises: Vec<WorkoutExerciseDetail>,
}

#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BodyMeasurement {
    pub id: i64,
    pub measured_at: i64,
    pub weight_g: Option<i64>,
    pub body_fat_bp: Option<i64>,
    pub waist_mm: Option<i64>,
    pub notes: Option<String>,
}

#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonalRecord {
    pub id: i64,
    pub exercise_id: i64,
    pub exercise_name: String,
    pub kind: String,
    pub value_g: i64,
    pub workout_set_id: i64,
    pub achieved_at: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FitnessHomeSummary {
    pub active_workout_id: Option<i64>,
    pub active_status: Option<String>,
    pub completed_workouts_this_week: i64,
    pub completed_sets_this_week: i64,
    pub streak_days: i64,
}

/// One local-calendar training week, oldest first in analytics responses.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeeklyActivityPoint {
    pub week_start: i64,
    pub label: String,
    pub completed_workouts: i64,
    pub completed_sets: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeeklyWorkoutGauge {
    pub completed: i64,
    pub target: i64,
}

/// A chart geometry value paired with the exact text shown to the user.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FitnessChartValue {
    pub value: f64,
    pub display: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BodyMetricPoint {
    pub label: String,
    pub value: FitnessChartValue,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BodyMetricTrend {
    pub metric: String,
    pub unit: String,
    pub points: Vec<BodyMetricPoint>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StrengthTrendPoint {
    pub label: String,
    pub estimated_1rm: Option<FitnessChartValue>,
    pub volume: FitnessChartValue,
}

#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrengthExerciseOption {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FitnessAnalytics {
    pub weekly_activity: Vec<WeeklyActivityPoint>,
    pub workout_target: WeeklyWorkoutGauge,
    pub body_metric: BodyMetricTrend,
    pub strength_trend: Vec<StrengthTrendPoint>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FitnessData {
    pub settings: FitnessSettings,
    /// Current local calendar date prepared by the server for date inputs.
    pub today: String,
    pub home: FitnessHomeSummary,
    pub exercises: Vec<ExerciseDetail>,
    /// Active weighted exercises plus exercises with completed weighted
    /// history. Historical eligibility follows the workout snapshot rather
    /// than the exercise's mutable current tracking mode.
    pub strength_exercises: Vec<StrengthExerciseOption>,
    pub plans: Vec<PlanDetail>,
    pub active_workout: Option<WorkoutDetail>,
    pub history: Vec<WorkoutDetail>,
    pub measurements: Vec<BodyMeasurement>,
    pub personal_records: Vec<PersonalRecord>,
    /// Completed workouts use their stable business date; an active workout
    /// uses its application-timezone start time. View code never calls a
    /// wall-clock or timezone API on wasm32.
    pub workout_dates: std::collections::HashMap<i64, String>,
    pub measurement_dates: std::collections::HashMap<i64, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg(feature = "ssr")]
pub struct ExerciseInput {
    pub name: String,
    pub category: String,
    pub tracking_mode: String,
    pub primary_muscle: Option<String>,
    pub equipment: Option<String>,
    pub notes: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg(feature = "ssr")]
pub struct ExerciseMediaInput {
    pub object_key: String,
    pub title: Option<String>,
    pub media_type: String,
    pub byte_size: i64,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg(feature = "ssr")]
pub struct PlanSetInput {
    pub target_reps: Option<i64>,
    pub target_weight_g: Option<i64>,
    pub target_duration_s: Option<i64>,
    pub target_distance_m: Option<i64>,
    pub target_rpe_x10: Option<i64>,
    pub set_type: String,
    pub rest_seconds: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg(feature = "ssr")]
pub struct PlanExerciseInput {
    pub exercise_id: i64,
    pub notes: Option<String>,
    pub sets: Vec<PlanSetInput>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg(feature = "ssr")]
pub struct PlanInput {
    pub name: String,
    pub notes: Option<String>,
    pub exercises: Vec<PlanExerciseInput>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg(feature = "ssr")]
pub struct SetResultInput {
    pub actual_reps: Option<i64>,
    pub actual_weight_g: Option<i64>,
    pub actual_duration_s: Option<i64>,
    pub actual_distance_m: Option<i64>,
    pub actual_rpe_x10: Option<i64>,
    pub status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg(feature = "ssr")]
pub struct QuickLogSetInput {
    pub reps: Option<i64>,
    pub weight_g: Option<i64>,
    pub duration_s: Option<i64>,
    pub distance_m: Option<i64>,
    pub rpe_x10: Option<i64>,
    pub set_type: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg(feature = "ssr")]
pub struct QuickLogExerciseInput {
    pub exercise_id: Option<i64>,
    pub new_exercise_name: Option<String>,
    pub tracking_mode: Option<String>,
    pub sets: Vec<QuickLogSetInput>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg(feature = "ssr")]
pub struct QuickLogInput {
    pub occurred_at: Option<i64>,
    pub notes: Option<String>,
    pub exercises: Vec<QuickLogExerciseInput>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinishWorkoutResult {
    pub workout: WorkoutDetail,
    pub new_records: Vec<PersonalRecord>,
}

/// Epley one-repetition maximum using integer grams and an i128 intermediate.
/// Reps outside 1..=12 are deliberately excluded from PR calculations.
#[cfg(any(feature = "ssr", test))]
pub fn epley_1rm_g(weight_g: i64, reps: i64) -> Option<i64> {
    if weight_g <= 0 || !(1..=12).contains(&reps) {
        return None;
    }
    let numerator = i128::from(weight_g) * i128::from(30 + reps);
    i64::try_from((numerator + 15) / 30).ok()
}

#[cfg(any(feature = "ssr", test))]
pub fn kilograms_to_grams(kg: f64) -> Option<i64> {
    finite_scaled_to_i64(kg, 1_000.0)
}

#[cfg(any(feature = "ssr", test))]
pub fn pounds_to_grams(lb: f64) -> Option<i64> {
    finite_scaled_to_i64(lb, 453.592_37)
}

#[cfg(any(feature = "ssr", test))]
pub fn centimetres_to_millimetres(cm: f64) -> Option<i64> {
    finite_scaled_to_i64(cm, 10.0)
}

#[cfg(any(feature = "ssr", test))]
pub fn inches_to_millimetres(inches: f64) -> Option<i64> {
    finite_scaled_to_i64(inches, 25.4)
}

#[cfg(any(feature = "ssr", test))]
pub fn display_weight_to_grams(value: f64, units: UnitSystem) -> Option<i64> {
    match units {
        UnitSystem::Metric => kilograms_to_grams(value),
        UnitSystem::Imperial => pounds_to_grams(value),
    }
}

#[cfg(any(feature = "ssr", test))]
pub fn display_waist_to_millimetres(value: f64, units: UnitSystem) -> Option<i64> {
    match units {
        UnitSystem::Metric => centimetres_to_millimetres(value),
        UnitSystem::Imperial => inches_to_millimetres(value),
    }
}

#[cfg(any(feature = "ssr", test))]
pub fn body_fat_percent_to_basis_points(percent: f64) -> Option<i64> {
    finite_scaled_to_i64(percent, 100.0).filter(|value| (1..=10_000).contains(value))
}

pub fn grams_to_display_weight(grams: i64, units: UnitSystem) -> f64 {
    match units {
        UnitSystem::Metric => grams as f64 / 1_000.0,
        UnitSystem::Imperial => grams as f64 / 453.592_37,
    }
}

pub fn millimetres_to_display_waist(millimetres: i64, units: UnitSystem) -> f64 {
    match units {
        UnitSystem::Metric => millimetres as f64 / 10.0,
        UnitSystem::Imperial => millimetres as f64 / 25.4,
    }
}

pub fn format_weight(grams: i64, units: UnitSystem) -> String {
    format!(
        "{:.2} {}",
        grams_to_display_weight(grams, units),
        units.weight_symbol()
    )
}

pub fn format_waist(millimetres: i64, units: UnitSystem) -> String {
    format!(
        "{:.1} {}",
        millimetres_to_display_waist(millimetres, units),
        units.waist_symbol()
    )
}

pub fn format_body_fat(basis_points: i64) -> String {
    format!("{:.2}%", basis_points as f64 / 100.0)
}

pub fn format_countdown(seconds: i64) -> String {
    let seconds = seconds.max(0);
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

#[cfg(any(feature = "hydrate", test))]
pub fn countdown_tick(remaining: i64, running: bool) -> (i64, bool) {
    if !running || remaining <= 0 {
        return (remaining.max(0), false);
    }
    let next = remaining - 1;
    (next, next > 0)
}

#[cfg(any(feature = "ssr", test))]
fn finite_scaled_to_i64(value: f64, scale: f64) -> Option<i64> {
    let scaled = value * scale;
    (value.is_finite() && value >= 0.0 && scaled <= i64::MAX as f64).then(|| scaled.round() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epley_has_expected_boundaries_and_rounding() {
        assert_eq!(epley_1rm_g(100_000, 1), Some(103_333));
        assert_eq!(epley_1rm_g(100_000, 12), Some(140_000));
        assert_eq!(epley_1rm_g(100_000, 13), None);
        assert_eq!(epley_1rm_g(0, 5), None);
        assert_eq!(epley_1rm_g(i64::MAX, 12), None);
    }

    #[test]
    fn display_units_convert_to_canonical_si() {
        assert_eq!(kilograms_to_grams(82.35), Some(82_350));
        assert_eq!(pounds_to_grams(100.0), Some(45_359));
        assert_eq!(centimetres_to_millimetres(81.2), Some(812));
        assert_eq!(inches_to_millimetres(32.0), Some(813));
        assert_eq!(kilograms_to_grams(f64::NAN), None);
        assert_eq!(kilograms_to_grams(-1.0), None);
    }

    #[test]
    fn configured_units_round_trip_canonical_values() {
        assert_eq!(UnitSystem::from_storage("metric"), Some(UnitSystem::Metric));
        assert_eq!(
            UnitSystem::from_storage("imperial"),
            Some(UnitSystem::Imperial)
        );
        assert_eq!(UnitSystem::from_storage("us"), None);

        let metric = display_weight_to_grams(82.35, UnitSystem::Metric).unwrap();
        assert_eq!(metric, 82_350);
        assert!((grams_to_display_weight(metric, UnitSystem::Metric) - 82.35).abs() < 0.001);

        let imperial = display_weight_to_grams(180.0, UnitSystem::Imperial).unwrap();
        assert_eq!(imperial, 81_647);
        assert!((grams_to_display_weight(imperial, UnitSystem::Imperial) - 180.0).abs() < 0.01);

        assert_eq!(
            display_waist_to_millimetres(32.0, UnitSystem::Imperial),
            Some(813)
        );
        assert_eq!(body_fat_percent_to_basis_points(18.25), Some(1_825));
        assert_eq!(body_fat_percent_to_basis_points(0.0), None);
        assert_eq!(body_fat_percent_to_basis_points(100.01), None);
    }

    #[test]
    fn presentation_formatters_include_selected_units() {
        assert_eq!(format_weight(82_350, UnitSystem::Metric), "82.35 kg");
        assert_eq!(format_weight(45_359, UnitSystem::Imperial), "100.00 lb");
        assert_eq!(format_waist(812, UnitSystem::Metric), "81.2 cm");
        assert_eq!(format_waist(813, UnitSystem::Imperial), "32.0 in");
        assert_eq!(format_body_fat(1_825), "18.25%");
    }

    #[test]
    fn countdown_stops_exactly_at_zero() {
        assert_eq!(format_countdown(125), "02:05");
        assert_eq!(countdown_tick(2, true), (1, true));
        assert_eq!(countdown_tick(1, true), (0, false));
        assert_eq!(countdown_tick(0, true), (0, false));
        assert_eq!(countdown_tick(9, false), (9, false));
    }
}
