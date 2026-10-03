// Generates the fixed Chromium extension key. The private key is never written anywhere:
// the extension is loaded unpacked, so only the public key (manifest "key") matters.
import { generateKeyPairSync } from "node:crypto";
import { existsSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { idFromKey } from "./chromium-id.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const keyFile = join(root, "chromium-key.txt");
const idFile = join(root, "chromium-id.txt");
const force = process.argv.includes("--force");

if (!force && (existsSync(keyFile) || existsSync(idFile))) {
  console.error("chromium-key.txt or chromium-id.txt already exists. Use --force to replace them.");
  process.exit(1);
}

const { publicKey } = generateKeyPairSync("rsa", {
  modulusLength: 2048,
  publicKeyEncoding: { type: "spki", format: "der" },
  privateKeyEncoding: { type: "pkcs8", format: "pem" },
});
const b64 = publicKey.toString("base64");
writeFileSync(keyFile, b64 + "\n");
writeFileSync(idFile, idFromKey(b64) + "\n");
console.log("Wrote chromium-key.txt and chromium-id.txt");
