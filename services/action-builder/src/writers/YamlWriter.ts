import fs from "fs";
import path from "path";
import YAML from "yaml";
import { log } from "../utils/logger.js";
import type { SiteCapability, PageCapability } from "../types/index.js";

/**
 * YAML writer for saving and loading site capabilities
 */
export class YamlWriter {
  private outputDir: string;

  constructor(outputDir: string = "./capability-store") {
    this.outputDir = outputDir;
  }

  /**
   * Save a site capability to YAML files
   * Merges with existing data if file already exists
   */
  save(capability: SiteCapability): string {
    const domain = capability.domain;
    const siteDir = path.join(this.outputDir, "sites", domain);
    const pagesDir = path.join(siteDir, "pages");

    // Create directories
    if (!fs.existsSync(pagesDir)) {
      fs.mkdirSync(pagesDir, { recursive: true });
    }

    // Load existing capability if exists (for merging)
    const existing = this.load(domain);

    // Merge capabilities
    const merged = this.mergeCapabilities(existing, capability);

    // Save site.yaml
    const siteYaml: Record<string, unknown> = {
      domain: merged.domain,
      name: merged.name,
      description: merged.description,
      version: merged.version,
      recorded_at: merged.recorded_at,
      scenario: merged.scenario,
    };

    if (merged.health_score !== undefined) {
      siteYaml.health_score = merged.health_score;
    }

    if (Object.keys(merged.global_elements).length > 0) {
      siteYaml.global_elements = merged.global_elements;
    }

    const siteYamlPath = path.join(siteDir, "site.yaml");
    fs.writeFileSync(siteYamlPath, YAML.stringify(siteYaml), "utf-8");
    log("info", `[YamlWriter] Saved: ${siteYamlPath}`);

    // Save page files
    for (const [pageType, page] of Object.entries(merged.pages)) {
      if (Object.keys(page.elements).length === 0) continue;

      const pageYamlPath = path.join(pagesDir, `${pageType}.yaml`);
      fs.writeFileSync(pageYamlPath, YAML.stringify(page), "utf-8");
      log("info", `[YamlWriter] Saved: ${pageYamlPath}`);
    }

    return siteDir;
  }

  /**
   * Merge two capabilities, with new data taking precedence for conflicts
   */
  private mergeCapabilities(
    existing: SiteCapability | null,
    newCap: SiteCapability
  ): SiteCapability {
    if (!existing) {
      return newCap;
    }

    // Merge pages
    const mergedPages: SiteCapability["pages"] = { ...existing.pages };

    for (const [pageType, newPage] of Object.entries(newCap.pages)) {
      if (mergedPages[pageType]) {
        // Merge elements within the page
        mergedPages[pageType] = {
          ...mergedPages[pageType],
          ...newPage,
          elements: {
            ...mergedPages[pageType].elements,
            ...newPage.elements,
          },
        };
      } else {
        mergedPages[pageType] = newPage;
      }
    }

    // Merge global elements
    const mergedGlobalElements = {
      ...existing.global_elements,
      ...newCap.global_elements,
    };

    // Count total elements for logging
    const existingCount = Object.values(existing.pages).reduce(
      (sum, p) => sum + Object.keys(p.elements).length,
      0
    );
    const newCount = Object.values(newCap.pages).reduce(
      (sum, p) => sum + Object.keys(p.elements).length,
      0
    );
    const mergedCount = Object.values(mergedPages).reduce(
      (sum, p) => sum + Object.keys(p.elements).length,
      0
    );
    log(
      "info",
      `[YamlWriter] Merged: ${existingCount} existing + ${newCount} new = ${mergedCount} total elements`
    );

    return {
      ...newCap,
      global_elements: mergedGlobalElements,
      pages: mergedPages,
    };
  }

  /**
   * Load a site capability from YAML files
   */
  load(domain: string): SiteCapability | null {
    const siteDir = path.join(this.outputDir, "sites", domain);
    const siteYamlPath = path.join(siteDir, "site.yaml");

    if (!fs.existsSync(siteYamlPath)) {
      log("warn", `[YamlWriter] Site not found: ${domain}`);
      return null;
    }

    // Load site.yaml
    const siteYaml = YAML.parse(fs.readFileSync(siteYamlPath, "utf-8")) as Record<
      string,
      unknown
    >;

    const capability: SiteCapability = {
      domain: siteYaml.domain as string,
      name: siteYaml.name as string,
      description: (siteYaml.description as string) || "",
      version: (siteYaml.version as string) || "1.0.0",
      recorded_at: siteYaml.recorded_at as string,
      scenario: (siteYaml.scenario as string) || "",
      health_score: siteYaml.health_score as number | undefined,
      global_elements:
        (siteYaml.global_elements as SiteCapability["global_elements"]) || {},
      pages: {},
    };

    // Load page files
    const pagesDir = path.join(siteDir, "pages");
    if (fs.existsSync(pagesDir)) {
      const pageFiles = fs
        .readdirSync(pagesDir)
        .filter((f) => f.endsWith(".yaml"));

      for (const pageFile of pageFiles) {
        const pageYamlPath = path.join(pagesDir, pageFile);
        const page = YAML.parse(
          fs.readFileSync(pageYamlPath, "utf-8")
        ) as PageCapability;
        capability.pages[page.page_type] = page;
      }
    }

    log(
      "info",
      `[YamlWriter] Loaded: ${domain} (${Object.keys(capability.pages).length} pages)`
    );
    return capability;
  }

  /**
   * List all recorded sites
   */
  listSites(): string[] {
    const sitesDir = path.join(this.outputDir, "sites");

    if (!fs.existsSync(sitesDir)) {
      return [];
    }

    return fs
      .readdirSync(sitesDir)
      .filter((dir) => {
        const siteYamlPath = path.join(sitesDir, dir, "site.yaml");
        return fs.existsSync(siteYamlPath);
      });
  }

  /**
   * Check if a site exists
   */
  exists(domain: string): boolean {
    const siteYamlPath = path.join(this.outputDir, "sites", domain, "site.yaml");
    return fs.existsSync(siteYamlPath);
  }

  /**
   * Get the output directory
   */
  getOutputDir(): string {
    return this.outputDir;
  }
}
