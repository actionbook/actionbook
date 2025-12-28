import { describe, expect, it } from "vitest";
import { z } from "zod";
import { toolInputToJsonSchema } from "./schema.js";

describe("toolInputToJsonSchema", () => {
  it("converts zod schema to json schema without $schema", () => {
    const schema = z.object({
      query: z.string(),
      page: z.number().optional(),
    });

    const jsonSchema = toolInputToJsonSchema(schema) as Record<string, unknown>;
    expect(jsonSchema).not.toHaveProperty("$schema");
    expect(jsonSchema).toHaveProperty("type", "object");
    expect(jsonSchema).toHaveProperty("properties");
  });
});
