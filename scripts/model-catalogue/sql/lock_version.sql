SELECT resource_version::text
FROM model.catalogue_version
WHERE id = 1
FOR UPDATE;
