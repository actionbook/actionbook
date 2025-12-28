import { z } from "zod";

export interface ToolDefinition<T extends z.ZodTypeAny> {
  name: string;
  description: string;
  inputSchema: T;
  handler: (input: z.infer<T>) => Promise<string>;
}

export function defineTool<T extends z.ZodTypeAny>(
  definition: ToolDefinition<T>,
): ToolDefinition<T> {
  return definition;
}

// Export tool creators
export { createSearchActionsTool } from "./search-actions.js";
export { createGetActionByIdTool } from "./get-action-by-id.js";
export { createListSourcesTool } from "./list-sources.js";
export { createSearchSourcesTool } from "./search-sources.js";
