import { describe, expect, it } from "vitest";
import { errorText, makeRequest } from "./protocol";

describe("protocol", () => {
  it("makes a request with a uuid id", () => {
    const r = makeRequest("PING");
    expect(r.id).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/);
    expect(r.cmd).toBe("PING");
    expect(r.payload).toEqual({});
  });
  it("extracts error text", () => {
    expect(errorText({ id: "1", ok: false, error: { code: "x", message: "m" } })).toBe("m");
    expect(errorText(null)).toBe("ODM did not answer.");
    expect(errorText({ id: "1", ok: true })).toBe("");
  });
});
