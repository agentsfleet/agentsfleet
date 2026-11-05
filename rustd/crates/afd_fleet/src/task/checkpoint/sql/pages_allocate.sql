INSERT INTO fleet.task_checkpoint_pages (id, task_checkpoint_id, page_number, created_at, updated_at)
SELECT page.id::uuid, $1::uuid, page.position - 1, now(), now()
FROM unnest($2::text[]) WITH ORDINALITY AS page(id, position)
