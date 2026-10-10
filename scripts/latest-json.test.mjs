import { describe, expect, it } from "vitest";
import { buildLatestJson, installerUrl, rfc3339, topSection } from "./latest-json.mjs";

const CHANGELOG = `# Changelog

All notable changes to ODM are documented here.

## [Unreleased]

- Something not released yet.

## [0.2.0] - 2026-10-20

### Added

- A Settings page.
- Automatic updates.

### Fixed

- A crash.

## [0.1.0] - 2026-09-01

First public release.

[0.2.0]: https://github.com/salimmay/ODM/releases/tag/v0.2.0
[0.1.0]: https://github.com/salimmay/ODM/releases/tag/v0.1.0
`;

describe("topSection", () => {
  it("takes the first versioned section, skipping Unreleased", () => {
    const s = topSection(CHANGELOG);
    expect(s.version).toBe("0.2.0");
    expect(s.notes).toBe("Added\n\n- A Settings page.\n- Automatic updates.\n\nFixed\n\n- A crash.");
  });
  it("leaves out the link references at the end", () => {
    const s = topSection("## [0.1.0] - 2026-09-01\n\nFirst public release.\n\n[0.1.0]: https://example.com\n");
    expect(s).toEqual({ version: "0.1.0", notes: "First public release." });
  });
  it("returns null when there is no versioned section", () => {
    expect(topSection("# Changelog\n\n## [Unreleased]\n\n- x\n")).toBeNull();
  });
});

describe("installerUrl", () => {
  it("points at the versioned installer of the GitHub release", () => {
    expect(installerUrl("0.2.0")).toBe("https://github.com/salimmay/ODM/releases/download/v0.2.0/ODM_0.2.0_x64-setup.exe");
  });
});

describe("rfc3339", () => {
  it("formats in UTC without milliseconds", () => {
    expect(rfc3339(new Date(Date.UTC(2026, 9, 10, 8, 5, 3, 456)))).toBe("2026-10-10T08:05:03Z");
  });
});

describe("buildLatestJson", () => {
  it("writes the manifest the Tauri updater reads", () => {
    const json = buildLatestJson({
      version: "0.2.0",
      changelog: CHANGELOG,
      signature: "dW50cnVzdGVkIGNvbW1lbnQ=\n",
      date: new Date(Date.UTC(2026, 9, 20, 12, 0, 0)),
    });
    expect(json).toEqual({
      version: "0.2.0",
      notes: "Added\n\n- A Settings page.\n- Automatic updates.\n\nFixed\n\n- A crash.",
      pub_date: "2026-10-20T12:00:00Z",
      platforms: {
        "windows-x86_64": {
          signature: "dW50cnVzdGVkIGNvbW1lbnQ=",
          url: "https://github.com/salimmay/ODM/releases/download/v0.2.0/ODM_0.2.0_x64-setup.exe",
        },
      },
    });
  });
  it("refuses when the top section is for another version", () => {
    expect(() => buildLatestJson({ version: "0.3.0", changelog: CHANGELOG, signature: "x", date: new Date() })).toThrow(
      "CHANGELOG.md's top section is 0.2.0, but the app version is 0.3.0.",
    );
  });
  it("refuses without a versioned section or a signature", () => {
    expect(() => buildLatestJson({ version: "0.2.0", changelog: "# Changelog\n", signature: "x", date: new Date() })).toThrow(
      "CHANGELOG.md has no versioned section.",
    );
    expect(() => buildLatestJson({ version: "0.2.0", changelog: CHANGELOG, signature: "  ", date: new Date() })).toThrow(
      "The installer's signature is empty.",
    );
  });
});
