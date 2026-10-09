-- 08:00-18:00 Australia/Sydney, weekdays. Athena uses the session timezone.
-- Set the workgroup timezone or convert explicitly before you rely on this.
SELECT timegenerated, actor, agentid, tool, decision
FROM magpie_audit
WHERE from_iso8601_timestamp(timegenerated) > current_timestamp - INTERVAL '7' DAY
  AND kind = 'decision'
  AND tool <> ''
  AND (
    day_of_week(at_timezone(from_iso8601_timestamp(timegenerated), 'Australia/Sydney')) IN (6, 7)
    OR hour(at_timezone(from_iso8601_timestamp(timegenerated), 'Australia/Sydney')) < 8
    OR hour(at_timezone(from_iso8601_timestamp(timegenerated), 'Australia/Sydney')) >= 18
  );
