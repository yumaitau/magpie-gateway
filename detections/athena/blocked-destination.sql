SELECT timegenerated, actor, agentid, model, reason
FROM magpie_audit
WHERE from_iso8601_timestamp(timegenerated) > current_timestamp - INTERVAL '1' DAY
  AND decision = 'block'
  AND (
    lower(reason) LIKE '%sovereign mode%'
    OR lower(reason) LIKE '%hosted in australia%'
    OR lower(reason) LIKE '%approved list%'
  );
