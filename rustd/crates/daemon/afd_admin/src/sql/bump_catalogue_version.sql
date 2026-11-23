UPDATE model.revision
SET number = number + 1,
    updated_at = now()
WHERE id = 1
