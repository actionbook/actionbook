import { describe, expect, it, vi } from "vitest";
import { createGetActionByIdTool } from "./get-action-by-id.js";
import { ChunkActionDetail } from "../lib/types.js";

describe("get_action_by_id tool", () => {
  it("returns formatted content with elements", async () => {
    const detail: ChunkActionDetail = {
      action_id: 123,
      content: "# Company Card Actions\n\nThis describes company card interactions.",
      elements: JSON.stringify({
        company_card: {
          css_selector: ".company-list li",
          xpath_selector: "//div[@class='company-list']//li",
          description: "Individual company card",
          element_type: "list_item",
          allow_methods: ["click"],
        },
        expand_button: {
          css_selector: ".company-list-card-small__button",
          description: "Button to expand company card",
          element_type: "button",
          allow_methods: ["click"],
        },
      }),
      createdAt: "2025-12-05T00:00:00.000Z",
      documentId: 1,
      documentTitle: "Company Actions Documentation",
      documentUrl: "https://example.com/docs",
      chunkIndex: 0,
      heading: "Company Card Actions",
      tokenCount: 500,
    };

    const apiClient = {
      getActionById: vi.fn().mockResolvedValue(detail),
    };

    const tool = createGetActionByIdTool(apiClient as any);
    const output = await tool.handler({ id: 123 });

    expect(output).toContain("**Action ID**: 123");
    expect(output).toContain("Company Card Actions");
    expect(output).toContain("UI Elements");
    expect(output).toContain("company_card");
    expect(output).toContain(".company-list li");
  });

  it("handles actions without elements", async () => {
    const detail: ChunkActionDetail = {
      action_id: 456,
      content: "General documentation without specific UI elements",
      elements: null,
      createdAt: "2025-12-05T00:00:00.000Z",
      documentId: 2,
      documentTitle: "General Guide",
      documentUrl: "https://example.com/guide",
      chunkIndex: 0,
      heading: "General Guide",
      tokenCount: 200,
    };

    const apiClient = {
      getActionById: vi.fn().mockResolvedValue(detail),
    };

    const tool = createGetActionByIdTool(apiClient as any);
    const output = await tool.handler({ id: 456 });

    expect(output).toContain("**Action ID**: 456");
    expect(output).toContain("General documentation");
    expect(output).not.toContain("UI Elements");
  });
});
