-- Remove demo business data inserted by 001_finance.sql. Keep seq high-water
-- marks so future document IDs never collide with historical seed IDs.

DELETE FROM activity
WHERE module = 'FIN'
  AND doc_id IN ('FIN-24091', 'FIN-24090', 'FIN-24089', 'FIN-24088', 'FIN-24087');

DELETE FROM fin_txn
WHERE doc_id IN (
    'FIN-24091',
    'FIN-24090',
    'FIN-24089',
    'FIN-24088',
    'FIN-24087',
    'FIN-24086',
    'FIN-24085',
    'FIN-24084'
);

DELETE FROM fin_budget
WHERE period = '2026-04'
  AND (
      (category_code = 'F&B' AND amount = 3200)
   OR (category_code = 'TRN' AND amount = 1600)
   OR (category_code = 'HLT' AND amount = 1200)
   OR (category_code = 'EDU' AND amount = 1500)
   OR (category_code = 'HSE' AND amount = 2000)
   OR (category_code = 'OTH' AND amount = 1500)
  );

DELETE FROM fin_account
WHERE NOT EXISTS (
    SELECT 1 FROM fin_txn t WHERE t.account_code = fin_account.code
)
AND (
      (code = 'ACC-01' AND type = 'Checking' AND tone = 'blue'   AND ABS(balance - 18421.40) < 0.00001)
   OR (code = 'ACC-02' AND type = 'Savings'  AND tone = 'green'  AND ABS(balance - 22800.00) < 0.00001)
   OR (code = 'ACC-03' AND type = 'Investment' AND tone = 'violet' AND ABS(balance - 15420.88) < 0.00001)
   OR (code = 'ACC-04' AND type = 'Cash'     AND tone = ''       AND ABS(balance - 1200.00) < 0.00001)
);

DELETE FROM fin_category
WHERE NOT EXISTS (
    SELECT 1 FROM fin_txn t WHERE t.category_code = fin_category.code
)
AND NOT EXISTS (
    SELECT 1 FROM fin_budget b WHERE b.category_code = fin_category.code
)
AND (
      (code = 'F&B' AND tone = 'amber'  AND sort_order = 1)
   OR (code = 'TRN' AND tone = 'blue'   AND sort_order = 2)
   OR (code = 'HLT' AND tone = 'green'  AND sort_order = 3)
   OR (code = 'EDU' AND tone = 'violet' AND sort_order = 4)
   OR (code = 'HSE' AND tone = 'rose'   AND sort_order = 5)
   OR (code = 'OTH' AND tone = ''       AND sort_order = 6)
   OR (code = 'INC' AND tone = 'green'  AND sort_order = 7)
   OR (code = 'TFR' AND tone = 'blue'   AND sort_order = 8)
);
