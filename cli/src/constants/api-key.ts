export const API_KEY_CREATED_AT = "created_at" as const;
export const API_KEY_KEY_NAME = "key_name" as const;
export const API_KEY_SORT_CREATED_AT_DESC = `-${API_KEY_CREATED_AT}` as const;
export const API_KEY_SORT_KEY_NAME_DESC = `-${API_KEY_KEY_NAME}` as const;
export const API_KEY_SORTS = [
  API_KEY_SORT_CREATED_AT_DESC,
  API_KEY_CREATED_AT,
  API_KEY_SORT_KEY_NAME_DESC,
  API_KEY_KEY_NAME,
] as const;
