import { createHash } from "node:crypto";

/** Chromium extension ID for a base64 DER SubjectPublicKeyInfo key. */
export function idFromKey(base64Spki) {
  const der = Buffer.from(base64Spki, "base64");
  const hex = createHash("sha256").update(der).digest("hex").slice(0, 32);
  return [...hex].map((c) => String.fromCharCode("a".charCodeAt(0) + parseInt(c, 16))).join("");
}
