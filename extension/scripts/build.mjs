import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { build } from "vite";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
process.chdir(root);

rmSync("dist", { recursive: true, force: true });

for (const target of ["chromium", "firefox"]) {
  const out = `dist/${target}`;
  for (const entry of ["background", "content", "popup"]) {
    await build({
      configFile: false,
      logLevel: "warn",
      define: { __TARGET__: JSON.stringify(target) },
      build: {
        outDir: out,
        emptyOutDir: false,
        minify: false,
        lib: {
          entry: `src/${entry}.ts`,
          formats: ["iife"],
          name: `odm_${entry}`,
          fileName: () => `${entry}.js`,
        },
      },
    });
  }
  cpSync("src/popup.html", `${out}/popup.html`);
  cpSync("src/popup.css", `${out}/popup.css`);
  mkdirSync(`${out}/icons`, { recursive: true });
  cpSync("../app/src-tauri/icons/32x32.png", `${out}/icons/32.png`);
  cpSync("../app/src-tauri/icons/128x128.png", `${out}/icons/128.png`);

  const manifest = JSON.parse(readFileSync(`manifests/${target}.json`, "utf8"));
  if (target === "chromium") {
    manifest.key = readFileSync("chromium-key.txt", "utf8").trim();
  }
  writeFileSync(`${out}/manifest.json`, JSON.stringify(manifest, null, 2) + "\n");
}
