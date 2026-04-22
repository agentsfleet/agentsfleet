export type UzErrorCode = string;

export class ApiError extends Error {
  status: number;
  code: UzErrorCode;
  requestId: string | undefined;

  constructor(message: string, status: number, code: UzErrorCode, requestId?: string) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
    this.requestId = requestId;
  }
}
