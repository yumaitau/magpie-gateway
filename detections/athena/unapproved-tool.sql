SELECT timegenerated, actor, agentid, tool, model, reason
FROM magpie_audit
WHERE from_iso8601_timestamp(timegenerated) > current_timestamp - INTERVAL '1' DAY
  AND decision = 'block'
  AND lower(reason) LIKE '%not approved%';
