INSERT INTO runner.checkpoint_pages (id, checkpoint_id, page_number, created_at, updated_at)
SELECT page.id, $1, page.position - 1, now(), now()
FROM unnest($2::uuid[]) WITH ORDINALITY AS page(id, position)
