import { describe, expect, it } from "vitest";
import { classifySendError, errorText, makeRequest, olderThan, readFormat, TIMEOUT_SENTINEL } from "./protocol";

describe("protocol", () => {
  it("makes a request with a uuid id", () => {
    const r = makeRequest("PING");
    expect(r.id).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/);
    expect(r.cmd).toBe("PING");
    expect(r.payload).toEqual({});
  });
  it("sends the extension's version when it has one", () => {
    expect(makeRequest("PING", {}, "0.4.3").ext).toBe("0.4.3");
    expect("ext" in makeRequest("PING")).toBe(false);
  });
  it("compares versions by number", () => {
    expect(olderThan("0.4.2", "0.4.3")).toBe(true);
    expect(olderThan("0.4.9", "0.10.0")).toBe(true);
    expect(olderThan("0.4.3", "0.4.3")).toBe(false);
    expect(olderThan("0.5.0", "0.4.3")).toBe(false);
    expect(olderThan("0.4.3", "junk")).toBe(false);
  });
  it("extracts error text", () => {
    expect(errorText({ id: "1", ok: false, error: { code: "x", message: "m" } })).toBe("m");
    expect(errorText(null)).toBe("ODM did not answer.");
    expect(errorText({ id: "1", ok: true })).toBe("");
  });
  it("classifies send failures", () => {
    expect(classifySendError(new Error("Specified native messaging host not found."))).toBe("missing_host");
    expect(classifySendError(new Error("Error when communicating with the native messaging host."))).toBe("other");
    expect(classifySendError(TIMEOUT_SENTINEL)).toBe("timeout");
  });
});

describe("readFormat", () => {
  it("keeps the two formats", () => {
    expect(readFormat("mp3")).toBe("mp3");
    expect(readFormat("video")).toBe("video");
  });
  it("drops anything else", () => {
    for (const v of [undefined, null, "wav", 3]) expect(readFormat(v)).toBeUndefined();
  });
});
