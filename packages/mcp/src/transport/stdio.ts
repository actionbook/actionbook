import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { logger } from "../lib/logger.js";

export function createStdioTransport(): StdioServerTransport {
  const transport = new StdioServerTransport();

  transport.onerror = (error) => {
    logger.error("Stdio transport error", error);
  };

  transport.onclose = () => {
    logger.info("Stdio transport closed");
  };

  return transport;
}
