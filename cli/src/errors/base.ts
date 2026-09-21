// The base every tagged CLI error extends.
//
// `errors/index.ts` and `errors/auth.ts` each carried a byte-identical copy of
// this class under a different name — `CliErrorBase` and `AuthFlowErrorBase` —
// so the rendered message format, the `_tag` convention and the prototype
// repair all existed twice. A fix to one was a fix to half the errors.

import { SUGGESTION_PREFIX } from "../constants/rejection.ts";

/** What every CLI error carries: what went wrong, and what to do about it. */
export interface Rejection {
  readonly detail: string;
  readonly suggestion: string;
}

/**
 * A tagged error whose message is `detail` then `suggestion`.
 *
 * `Object.setPrototypeOf(this, new.target.prototype)` is load-bearing, not
 * ceremony: extending `Error` across a transpile target that downlevels
 * classes loses the subclass prototype, and `instanceof` — which the exit-code
 * mapping and `Effect.catchTag` both lean on — starts answering false.
 */
export abstract class TaggedCliError<Tag extends string> extends Error {
  readonly _tag: Tag;
  readonly detail: string;
  readonly suggestion: string;

  protected constructor(tag: Tag, fields: Rejection) {
    super(`${fields.detail}${SUGGESTION_PREFIX}${fields.suggestion}`);
    this.name = tag;
    this._tag = tag;
    this.detail = fields.detail;
    this.suggestion = fields.suggestion;
    Object.setPrototypeOf(this, new.target.prototype);
  }
}

/** A tagged error that also carries the server's request id, when it had one. */
export abstract class TaggedRequestError<Tag extends string> extends TaggedCliError<Tag> {
  readonly requestId: string | null | undefined;

  protected constructor(
    tag: Tag,
    fields: Rejection & { readonly requestId?: string | null },
  ) {
    super(tag, fields);
    this.requestId = fields.requestId;
  }
}
