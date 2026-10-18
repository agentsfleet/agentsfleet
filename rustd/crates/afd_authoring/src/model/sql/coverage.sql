SELECT w.balance_nanos,
       count(a.id)::bigint AS open_count,
       COALESCE(max(a.reserved_nanos), 0)::bigint AS largest_reserved
  FROM billing.wallets w
  LEFT JOIN authoring.attempts a
         ON a.tenant_id = w.tenant_id
        AND (a.status IN ($2, $3) OR a.usage_unknown)
 WHERE w.tenant_id = $1::uuid
 GROUP BY w.balance_nanos
