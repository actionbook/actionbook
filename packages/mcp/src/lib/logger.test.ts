import { describe, expect, it, vi } from "vitest";
import { Logger } from "./logger.js";

describe("Logger", () => {
  it("filters messages below level", () => {
    const sink = vi.fn();
    const logger = new Logger("warn", sink);

    logger.info("hidden");
    logger.warn("visible");

    expect(sink).toHaveBeenCalledTimes(1);
    const [level, message] = sink.mock.calls[0];
    expect(level).toBe("warn");
    expect(message).toBe("visible");
  });
});
