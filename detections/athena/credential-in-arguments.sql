-- Matches the hit kind. It does not require the secret to be stored.
SELECT timegenerated, actor, agentid, tool, decision, reason
FROM magpie_audit
WHERE from_iso8601_timestamp(timegenerated) > current_timestamp - INTERVAL '1' DAY
  AND (
    lower(detail) LIKE '%"kind":"secret"%'
    OR lower(reason) LIKE '%private key%'
    OR lower(reason) LIKE '%api key%'
  );
