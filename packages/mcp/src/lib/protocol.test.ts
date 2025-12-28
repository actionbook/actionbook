import { describe, expect, it } from "vitest";
import {
  CURRENT_PROTOCOL_VERSION,
  SUPPORTED_PROTOCOL_VERSIONS,
  isSupportedVersion,
  negotiateVersion,
} from "./protocol.js";

describe("protocol version negotiation", () => {
  it("returns true for supported versions", () => {
    expect(isSupportedVersion(CURRENT_PROTOCOL_VERSION)).toBe(true);
  });

  it("negotiates to client version if supported", () => {
    const clientVersion = SUPPORTED_PROTOCOL_VERSIONS[0];
    expect(negotiateVersion(clientVersion)).toBe(clientVersion);
  });

  it("falls back to current version when unsupported", () => {
    expect(negotiateVersion("1999-01-01")).toBe(CURRENT_PROTOCOL_VERSION);
  });
});
