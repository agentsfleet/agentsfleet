SELECT number::text
FROM model.revision
WHERE id = 1
FOR UPDATE;
