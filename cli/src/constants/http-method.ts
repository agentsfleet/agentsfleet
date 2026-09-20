// The HTTP methods this client issues, named once.
//
// Nine declarations lived across eight files under two spellings —
// `METHOD_POST` and `HTTP_METHOD_POST` for the same four characters — and the
// request type then listed them a tenth time as an inline union, so adding a
// verb meant finding every copy. The union is derived here instead: a method
// the client cannot name is a method it cannot send.

export const HTTP_METHOD = {
  get: "GET",
  post: "POST",
  put: "PUT",
  patch: "PATCH",
  delete: "DELETE",
} as const;

/** Every verb `HttpClient.request` accepts, derived from the table above. */
export type HttpMethod = (typeof HTTP_METHOD)[keyof typeof HTTP_METHOD];
