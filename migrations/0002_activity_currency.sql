-- Multi-currency finance: activity rows carry the currency of their source
-- transaction so cross-module feeds (dashboard, today) can format amounts
-- with the right symbol and precision. NULL for non-finance modules and for
-- finance rows that have no monetary amount.
--
-- Existing finance activity amounts were stored as REAL major-unit yuan;
-- scale them x100 into integer minor units to match the finance module's
-- 002_multi_currency rebuild, and tag them with the default CNY currency.
ALTER TABLE activity ADD COLUMN currency_code TEXT;
UPDATE activity
   SET currency_code = 'CNY',
       amount = CAST(ROUND(amount * 100) AS INTEGER)
 WHERE module = 'FIN' AND amount IS NOT NULL;
