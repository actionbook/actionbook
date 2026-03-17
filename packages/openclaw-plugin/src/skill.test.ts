import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const skillPath = resolve(__dirname, "../skills/actionbook/SKILL.md");

describe("SKILL.md", () => {
  it("should exist and be readable", () => {
    const content = readFileSync(skillPath, "utf-8");
    expect(content.length).toBeGreaterThan(100);
  });

  it("should have valid YAML frontmatter", () => {
    const content = readFileSync(skillPath, "utf-8");
    expect(content).toMatch(/^---\n/);
    expect(content).toMatch(/name:\s*actionbook/);
    expect(content).toMatch(/description:\s*.+/);
    expect(content).toMatch(/---\n/);
  });

  it("should contain selector priority guidance", () => {
    const content = readFileSync(skillPath, "utf-8");
    expect(content).toContain("data-testid");
    expect(content).toContain("aria-label");
    expect(content).toContain("role selector");
  });

  it("should contain fallback strategy", () => {
    const content = readFileSync(skillPath, "utf-8");
    expect(content).toContain("Fallback");
    expect(content).toContain("browser tools");
  });

  it("should contain search query construction guidance", () => {
    const content = readFileSync(skillPath, "utf-8");
    expect(content).toContain("Constructing an Effective Search Query");
    expect(content).toContain("Good query");
  });
});
