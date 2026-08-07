// `webkitdirectory` is the only attribute any browser offers for choosing a
// directory instead of a file. Every engine ships it; it never became standard,
// so React's typings omit it and a `.tsx` that sets it needs this.
//
// The empty export is what makes this file a module — without it the block
// below would declare a NEW ambient module that shadows the real React one,
// and every React export in the workspace stops resolving.
export {};

declare module "react" {
  interface InputHTMLAttributes<T> {
    webkitdirectory?: string;
  }
}
