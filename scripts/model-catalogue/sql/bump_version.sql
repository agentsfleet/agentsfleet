UPDATE model.catalogue_version
SET resource_version = resource_version + 1, updated_at = now()
WHERE id = 1;
