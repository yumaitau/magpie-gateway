SELECT date_trunc('hour', from_iso8601_timestamp(timegenerated)) AS window_start,
       count(*) AS hits
FROM magpie_audit
WHERE from_iso8601_timestamp(timegenerated) > current_timestamp - INTERVAL '1' DAY
  AND lower(reason) LIKE '%dlp hit%'
GROUP BY 1
HAVING count(*) >= 20;
