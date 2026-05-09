-- Remove demo learning records inserted by 001_learning.sql. Keep seq
-- high-water marks so future IDs never collide with historical seed IDs.

DELETE FROM lrn_note
WHERE doc_id IN ('LRN-N-221', 'LRN-N-220', 'LRN-N-219');

DELETE FROM lrn_book
WHERE doc_id IN ('LRN-B-014', 'LRN-B-013', 'LRN-B-012', 'LRN-B-011')
  AND NOT EXISTS (
      SELECT 1 FROM lrn_note n WHERE n.book_doc = lrn_book.doc_id
  );

DELETE FROM lrn_course
WHERE doc_id IN ('LRN-C-08', 'LRN-C-09', 'LRN-C-10', 'LRN-C-11')
  AND NOT EXISTS (
      SELECT 1 FROM lrn_note n WHERE n.course_doc = lrn_course.doc_id
  );
