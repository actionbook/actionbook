import { describe, expect, it } from "vitest";
import { ChunkSearchResult, ChunkActionDetail, ParsedElements } from "./types.js";

describe("types", () => {
  it("ChunkSearchResult has correct structure", () => {
    const result: ChunkSearchResult = {
      success: true,
      query: "test",
      results: [
        {
          action_id: 1,
          content: "test content",
          score: 0.95,
          createdAt: "2025-12-05T00:00:00.000Z",
        },
      ],
      count: 1,
      total: 1,
      hasMore: false,
    };

    expect(result.success).toBe(true);
    expect(result.results[0].action_id).toBe(1);
  });

  it("ChunkActionDetail has correct structure", () => {
    const detail: ChunkActionDetail = {
      action_id: 123,
      content: "test content",
      elements: null,
      createdAt: "2025-12-05T00:00:00.000Z",
      documentId: 1,
      documentTitle: "Test",
      documentUrl: "https://example.com",
      chunkIndex: 0,
      heading: "Test",
      tokenCount: 100,
    };

    expect(detail.action_id).toBe(123);
    expect(detail.elements).toBeNull();
  });

  it("ParsedElements can be parsed from JSON", () => {
    const elementsJson = JSON.stringify({
      test_element: {
        css_selector: ".test",
        description: "Test element",
        element_type: "button",
        allow_methods: ["click"],
      },
    });

    const elements: ParsedElements = JSON.parse(elementsJson);
    expect(elements.test_element.css_selector).toBe(".test");
    expect(elements.test_element.allow_methods).toEqual(["click"]);
  });
});
