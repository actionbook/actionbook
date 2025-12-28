import { describe, expect, it, vi } from "vitest";
import { createSearchActionsTool } from "./search-actions.js";
import { ChunkSearchResult } from "../lib/types.js";

describe("search_actions tool", () => {
  const mockResult: ChunkSearchResult = {
    success: true,
    query: "company",
    results: [
      {
        action_id: 123,
        content: "Company card element with click functionality",
        score: 0.95,
        createdAt: "2025-12-05T00:00:00.000Z",
      },
    ],
    count: 1,
    total: 1,
    hasMore: false,
  };

  it("formats search results", async () => {
    const apiClient = {
      searchActions: vi.fn().mockResolvedValue(mockResult),
    };

    const tool = createSearchActionsTool(apiClient as any);
    const output = await tool.handler({ query: "company", type: "hybrid", limit: 5 });
    expect(output).toContain("Search Results for \"company\"");
    expect(output).toContain("Action ID: 123");
    expect(output).toContain("**Score**: 0.950");
  });

  it("handles empty results", async () => {
    const apiClient = {
      searchActions: vi.fn().mockResolvedValue({
        success: true,
        query: "nonexistent",
        results: [],
        count: 0,
        total: 0,
        hasMore: false,
      }),
    };

    const tool = createSearchActionsTool(apiClient as any);
    const output = await tool.handler({ query: "nonexistent", type: "fulltext", limit: 10 });
    expect(output).toContain("No actions found");
  });
});
