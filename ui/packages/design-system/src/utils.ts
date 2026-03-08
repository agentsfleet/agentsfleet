export type ClassValue = string | number | boolean | null | undefined | ClassValue[];

function flattenClassValues(input: ClassValue, out: string[]) {
  if (!input) return;
  if (Array.isArray(input)) {
    for (const item of input) flattenClassValues(item, out);
    return;
  }
  out.push(String(input));
}

export function cn(...inputs: ClassValue[]) {
  const out: string[] = [];
  for (const input of inputs) flattenClassValues(input, out);
  return out.join(" ");
}
