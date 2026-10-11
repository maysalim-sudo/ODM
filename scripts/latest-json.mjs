// Builds latest.json, the manifest the ODM updater reads from the latest GitHub release.
// Pure functions only, so they can be tested; release.mjs does the file reading and writing.

import { join } from "node:path";

const RELEASES = "https://github.com/salimmay/ODM/releases/download";

/**
 * Updater files an earlier build may have left: a plain build must not leave an old
 * latest.json or signature in release-assets (they would not match its installer), and a
 * signed build must find its own fresh .sig next to the installer, not an old one.
 */
export function staleReleaseFiles({ assets, nsisDir, setupName }) {
  return [
    join(assets, "latest.json"),
    join(assets, setupName),
    join(assets, `${setupName}.sig`),
    join(nsisDir, `${setupName}.sig`),
  ];
}

/** The versioned installer the manifest points at, attached to the `v<version>` release. */
export function installerUrl(version) {
  return `${RELEASES}/v${version}/ODM_${version}_x64-setup.exe`;
}

/** RFC 3339 in UTC, without milliseconds: 2026-10-10T08:05:03Z. */
export function rfc3339(date) {
  return date.toISOString().replace(/\.\d{3}Z$/, "Z");
}

/**
 * The first versioned section of a Keep-a-Changelog file ("## [0.2.0] - date"), skipping
 * "Unreleased". Its notes are plain text: sub-headings lose their "#" marks and link
 * references are left out. Null when there is no versioned section.
 */
export function topSection(changelog) {
  const lines = changelog.replace(/\r\n/g, "\n").split("\n");
  const start = lines.findIndex((l) => /^##\s+\[\d+\.\d+\.\d+[^\]]*\]/.test(l));
  if (start === -1) return null;
  const version = /^##\s+\[([^\]]+)\]/.exec(lines[start])[1];
  const body = [];
  for (const l of lines.slice(start + 1)) {
    if (/^##\s/.test(l)) break;
    if (/^\[[^\]]+\]:\s/.test(l)) continue;
    body.push(l.replace(/^#{3,}\s*/, ""));
  }
  return { version, notes: body.join("\n").trim() };
}

/** The latest.json object for one Windows x64 release. */
export function buildLatestJson({ version, changelog, signature, date }) {
  const top = topSection(changelog);
  if (!top) throw new Error("CHANGELOG.md has no versioned section.");
  if (top.version !== version) {
    throw new Error(`CHANGELOG.md's top section is ${top.version}, but the app version is ${version}.`);
  }
  const sig = signature.trim();
  if (!sig) throw new Error("The installer's signature is empty.");
  return {
    version,
    notes: top.notes,
    pub_date: rfc3339(date),
    platforms: {
      "windows-x86_64": { signature: sig, url: installerUrl(version) },
    },
  };
}
