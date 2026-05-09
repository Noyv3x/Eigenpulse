-- Remove demo workouts inserted by 001_fitness.sql. Keep seq high-water marks.

DELETE FROM fit_set
WHERE workout_doc IN ('FIT-S-0412', 'FIT-S-0411', 'FIT-S-0410', 'FIT-S-0409', 'FIT-S-0408');

DELETE FROM fit_workout
WHERE doc_id IN ('FIT-S-0412', 'FIT-S-0411', 'FIT-S-0410', 'FIT-S-0409', 'FIT-S-0408');
