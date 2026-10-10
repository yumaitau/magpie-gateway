-- Same check as sentinel/tool-call-spree.kql, for newline-delimited JSON in S3.
-- Column names follow sentinel_record. Adjust them if the crawler lowercases keys.
SELECT agentid,
       date_trunc('minute', from_iso8601_timestamp(timegenerated)) AS window_start,
       count(*) AS calls,
       count(DISTINCT tool) AS distinct_tools
FROM magpie_audit
WHERE from_iso8601_timestamp(timegenerated) > current_timestamp - INTERVAL '1' HOUR
  AND kind = 'decision'
  AND tool <> ''
GROUP BY 1, 2
HAVING count(*) >= 100 OR count(DISTINCT tool) >= 15;
