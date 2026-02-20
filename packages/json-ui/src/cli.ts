#!/usr/bin/env node
/**
 * json-ui CLI
 *
 * Render JSON report to HTML and open in browser
 *
 * Usage:
 *   json-ui render report.json              # Render and open
 *   json-ui render report.json -o out.html  # Render to file
 *   json-ui render report.json --no-open    # Don't open browser
 *   cat report.json | json-ui render -      # Read from stdin
 */

import fs from 'fs/promises';
import path from 'path';
import { execSync } from 'child_process';
import os from 'os';

// ============================================
// HTML Template
// ============================================

// ============================================
// i18n Helpers
// ============================================

type I18nValue = string | { en: string; zh: string };

/** Render an i18n value as HTML. If it's an i18n object, output dual spans. */
function renderI18n(value: unknown, escape = true): string {
  if (value != null && typeof value === 'object' && 'en' in value && 'zh' in value) {
    const obj = value as { en: string; zh: string };
    const en = escape ? escapeHtml(obj.en) : obj.en;
    const zh = escape ? escapeHtml(obj.zh) : obj.zh;
    return `<span class="i18n-en">${en}</span><span class="i18n-zh">${zh}</span>`;
  }
  return escape ? escapeHtml(String(value ?? '')) : String(value ?? '');
}

/** Check if a value is an i18n object */
function isI18n(value: unknown): value is { en: string; zh: string } {
  return value != null && typeof value === 'object' && 'en' in value && 'zh' in value;
}

/** Resolve i18n value to a plain string for a specific language (used in attributes) */
function resolveI18n(value: unknown, lang: 'en' | 'zh' = 'en'): string {
  if (isI18n(value)) return value[lang];
  return String(value ?? '');
}

function generateHTML(json: ReportJSON, options: { title?: string } = {}): string {
  const rawTitle = options.title || json.props?.title || 'Paper Report';
  const title = resolveI18n(rawTitle, 'en');

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>${escapeHtml(title)}</title>
  <link rel="preconnect" href="https://fonts.googleapis.com">
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
  <link href="https://fonts.googleapis.com/css2?family=Space+Grotesk:wght@400;500;600;700&family=IBM+Plex+Mono:wght@400;500;600&display=swap" rel="stylesheet">
  <style>
    :root {
      --color-primary: #3b82f6;
      --color-success: #10b981;
      --color-warning: #f59e0b;
      --color-danger: #ef4444;
      --color-text: #374151;
      --color-text-muted: #6b7280;
      --color-bg: #ffffff;
      --color-bg-muted: #f9fafb;
      --color-border: #e5e7eb;
    }

    @media (prefers-color-scheme: dark) {
      :root {
        --color-text: #f3f4f6;
        --color-text-muted: #9ca3af;
        --color-bg: #111827;
        --color-bg-muted: #1f2937;
        --color-border: #374151;
      }
    }

    * { box-sizing: border-box; margin: 0; padding: 0; }

    body {
      font-family: system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
      line-height: 1.6;
      color: var(--color-text);
      background: var(--color-bg);
      padding: 2rem;
    }

    .report {
      max-width: 800px;
      margin: 0 auto;
    }

    /* Brand Header */
    .brand-header {
      background: var(--color-bg-muted);
      padding: 0.75rem 1rem;
      border-radius: 8px;
      margin-bottom: 1.5rem;
      display: flex;
      justify-content: space-between;
      align-items: center;
    }

    .brand-header .powered-by {
      color: var(--color-text-muted);
      font-size: 0.875rem;
    }

    /* Paper Header */
    .paper-header { margin-bottom: 1.5rem; }
    .paper-header h1 { font-size: 1.75rem; margin-bottom: 0.5rem; }
    .paper-header .meta {
      display: flex;
      gap: 1rem;
      color: var(--color-text-muted);
      font-size: 0.875rem;
      flex-wrap: wrap;
    }
    .paper-header .categories {
      margin-top: 0.5rem;
      display: flex;
      gap: 0.5rem;
    }
    .paper-header .category {
      background: var(--color-bg-muted);
      padding: 0.125rem 0.5rem;
      border-radius: 9999px;
      font-size: 0.75rem;
    }

    /* Authors */
    .authors { margin-bottom: 1rem; color: var(--color-text); }
    .authors .affiliation { color: var(--color-text-muted); }

    /* Section */
    .section { margin-bottom: 1.5rem; }
    .section h2 {
      display: flex;
      align-items: center;
      gap: 0.5rem;
      border-bottom: 2px solid var(--color-border);
      padding-bottom: 0.5rem;
      margin-bottom: 1rem;
      font-size: 1.25rem;
    }

    /* Abstract */
    .abstract {
      color: var(--color-text);
      text-align: justify;
    }
    .abstract mark {
      background: #fef08a;
      padding: 0 2px;
      border-radius: 2px;
    }

    /* Contribution List */
    .contribution-list { list-style: decimal; padding-left: 1.5rem; }
    .contribution-list li { margin-bottom: 0.75rem; }
    .contribution-list .badge {
      background: var(--color-primary);
      color: white;
      padding: 0.125rem 0.5rem;
      border-radius: 4px;
      font-size: 0.75rem;
      margin-right: 0.5rem;
    }
    .contribution-list .description { color: var(--color-text-muted); }

    /* Method Overview */
    .method-overview { display: flex; flex-direction: column; gap: 1rem; }
    .method-step {
      display: flex;
      align-items: flex-start;
      gap: 1rem;
    }
    .method-step .number {
      width: 2rem;
      height: 2rem;
      border-radius: 50%;
      background: var(--color-primary);
      color: white;
      display: flex;
      align-items: center;
      justify-content: center;
      font-weight: bold;
      flex-shrink: 0;
    }
    .method-step .content strong { display: block; }
    .method-step .content p { margin: 0.25rem 0 0; color: var(--color-text-muted); }

    /* Highlight */
    .highlight {
      padding: 1rem;
      margin: 1rem 0;
      border-radius: 0 4px 4px 0;
    }
    .highlight.quote { border-left: 4px solid var(--color-primary); background: #eff6ff; }
    .highlight.important { border-left: 4px solid var(--color-warning); background: #fffbeb; }
    .highlight.warning { border-left: 4px solid var(--color-danger); background: #fef2f2; }
    .highlight.code { border-left: 4px solid var(--color-success); background: #ecfdf5; font-family: monospace; }
    .highlight .source { margin-top: 0.5rem; font-size: 0.875rem; color: var(--color-text-muted); }

    /* Metrics Grid */
    .metrics-grid {
      display: grid;
      gap: 1rem;
    }
    .metric {
      padding: 1rem;
      background: var(--color-bg-muted);
      border-radius: 8px;
      text-align: center;
    }
    .metric .value {
      font-size: 1.5rem;
      font-weight: bold;
    }
    .metric .value .suffix { font-size: 1rem; color: var(--color-text-muted); }
    .metric .value .trend-up { color: var(--color-success); }
    .metric .value .trend-down { color: var(--color-danger); }
    .metric .label { color: var(--color-text-muted); font-size: 0.875rem; }

    /* Link Group */
    .link-group {
      display: flex;
      gap: 0.75rem;
      flex-wrap: wrap;
    }
    .link-button {
      display: inline-flex;
      align-items: center;
      gap: 0.5rem;
      padding: 0.5rem 1rem;
      background: var(--color-primary);
      color: white;
      border-radius: 6px;
      text-decoration: none;
      font-size: 0.875rem;
    }
    .link-button:hover { opacity: 0.9; }

    /* Brand Footer */
    .brand-footer {
      margin-top: 2rem;
      padding-top: 1rem;
      border-top: 1px solid var(--color-border);
      color: var(--color-text-muted);
      font-size: 0.875rem;
    }

    /* Grid */
    .grid {
      display: grid;
      gap: 1rem;
    }

    /* Card */
    .card {
      background: var(--color-bg);
      border: 1px solid var(--color-border);
      border-radius: 8px;
      overflow: hidden;
    }
    .card.shadow { box-shadow: 0 1px 3px rgba(0,0,0,0.1); }
    .card.padding-sm { padding: 0.5rem; }
    .card.padding-md { padding: 1rem; }
    .card.padding-lg { padding: 1.5rem; }

    /* Figure / Image */
    .figure {
      margin: 1.5rem 0;
      text-align: center;
    }
    .figure img {
      max-width: 100%;
      height: auto;
      border-radius: 4px;
      border: 1px solid var(--color-border);
    }
    .figure .images {
      display: flex;
      gap: 1rem;
      justify-content: center;
      flex-wrap: wrap;
    }
    .figure figcaption {
      margin-top: 0.75rem;
      color: var(--color-text-muted);
      font-size: 0.875rem;
    }
    .figure .label {
      font-weight: bold;
      color: var(--color-text);
    }

    .image {
      margin: 1rem 0;
      text-align: center;
    }
    .image img {
      max-width: 100%;
      height: auto;
      border-radius: 4px;
    }
    .image .caption {
      margin-top: 0.5rem;
      color: var(--color-text-muted);
      font-size: 0.875rem;
    }

    /* Formula (LaTeX) */
    .formula {
      margin: 1rem 0;
      text-align: center;
    }
    .formula.block {
      padding: 1rem;
      background: var(--color-bg-muted);
      border-radius: 4px;
      overflow-x: auto;
    }
    .formula .label {
      float: right;
      color: var(--color-text-muted);
      font-size: 0.875rem;
    }
    .formula code {
      font-family: 'Computer Modern', 'Latin Modern Math', serif;
      font-size: 1.1em;
    }

    /* Prose (Markdown) */
    .prose {
      line-height: 1.75;
    }
    .prose p { margin-bottom: 1rem; }
    .prose h3 { margin: 1.5rem 0 0.75rem; font-size: 1.1rem; }
    .prose h4 { margin: 1.25rem 0 0.5rem; font-size: 1rem; }
    .prose ul, .prose ol { padding-left: 1.5rem; margin-bottom: 1rem; }
    .prose li { margin-bottom: 0.25rem; }
    .prose code {
      background: var(--color-bg-muted);
      padding: 0.125rem 0.375rem;
      border-radius: 3px;
      font-size: 0.9em;
    }
    .prose strong { font-weight: 600; }
    .prose em { font-style: italic; }

    /* Callout */
    .callout {
      padding: 1rem;
      margin: 1rem 0;
      border-radius: 8px;
      border-left: 4px solid;
    }
    .callout.info { border-color: var(--color-primary); background: #eff6ff; }
    .callout.tip { border-color: var(--color-success); background: #ecfdf5; }
    .callout.warning { border-color: var(--color-warning); background: #fffbeb; }
    .callout.important { border-color: var(--color-danger); background: #fef2f2; }
    .callout.note { border-color: #8b5cf6; background: #f5f3ff; }
    @media (prefers-color-scheme: dark) {
      .callout.info { background: #1e3a5f; }
      .callout.tip { background: #1a3d2e; }
      .callout.warning { background: #3d3219; }
      .callout.important { background: #3d1f1f; }
      .callout.note { background: #2d2350; }
    }
    .callout .callout-title {
      font-weight: bold;
      margin-bottom: 0.5rem;
      display: flex;
      align-items: center;
      gap: 0.5rem;
    }
    .callout .callout-title::before {
      content: 'ℹ️';
    }
    .callout.tip .callout-title::before { content: '💡'; }
    .callout.warning .callout-title::before { content: '⚠️'; }
    .callout.important .callout-title::before { content: '🔴'; }
    .callout.note .callout-title::before { content: '📝'; }

    /* Definition List */
    .definition-list {
      margin: 1rem 0;
    }
    .definition-list dl {
      display: grid;
      gap: 0.75rem;
    }
    .definition-list dt {
      font-weight: bold;
      color: var(--color-primary);
    }
    .definition-list dd {
      margin-left: 1rem;
      color: var(--color-text);
    }

    /* Theorem */
    .theorem {
      margin: 1.5rem 0;
      padding: 1rem 1.25rem;
      background: var(--color-bg-muted);
      border-radius: 8px;
      border-left: 4px solid var(--color-primary);
    }
    .theorem .theorem-header {
      font-weight: bold;
      margin-bottom: 0.5rem;
      color: var(--color-primary);
    }
    .theorem.lemma { border-color: #8b5cf6; }
    .theorem.lemma .theorem-header { color: #8b5cf6; }
    .theorem.proposition { border-color: var(--color-success); }
    .theorem.proposition .theorem-header { color: var(--color-success); }
    .theorem.definition { border-color: var(--color-warning); }
    .theorem.definition .theorem-header { color: var(--color-warning); }

    /* Algorithm */
    .algorithm {
      margin: 1.5rem 0;
      background: var(--color-bg-muted);
      border-radius: 8px;
      overflow: hidden;
    }
    .algorithm .algorithm-title {
      background: var(--color-primary);
      color: white;
      padding: 0.5rem 1rem;
      font-weight: bold;
    }
    .algorithm .algorithm-body {
      padding: 1rem;
      font-family: 'Consolas', 'Monaco', monospace;
      font-size: 0.9rem;
    }
    .algorithm .line {
      display: flex;
      gap: 0.5rem;
    }
    .algorithm .line-number {
      color: var(--color-text-muted);
      user-select: none;
      width: 2rem;
      text-align: right;
    }
    .algorithm .line-code {
      flex: 1;
    }
    .algorithm .indent-1 { padding-left: 1.5rem; }
    .algorithm .indent-2 { padding-left: 3rem; }
    .algorithm .indent-3 { padding-left: 4.5rem; }
    .algorithm .algorithm-caption {
      padding: 0.5rem 1rem;
      font-size: 0.875rem;
      color: var(--color-text-muted);
      border-top: 1px solid var(--color-border);
    }

    /* Results Table */
    .results-table {
      margin: 1.5rem 0;
      overflow-x: auto;
    }
    .results-table table {
      width: 100%;
      border-collapse: collapse;
      font-size: 0.9rem;
    }
    .results-table th,
    .results-table td {
      padding: 0.75rem;
      text-align: left;
      border-bottom: 1px solid var(--color-border);
    }
    .results-table th {
      background: var(--color-bg-muted);
      font-weight: bold;
    }
    .results-table th.highlight {
      background: var(--color-primary);
      color: white;
    }
    .results-table td.highlight {
      background: #fef08a;
      font-weight: bold;
    }
    @media (prefers-color-scheme: dark) {
      .results-table td.highlight {
        background: #854d0e;
      }
    }
    .results-table caption {
      margin-bottom: 0.5rem;
      font-size: 0.875rem;
      color: var(--color-text-muted);
      text-align: left;
    }

    /* Code Block */
    .code-block {
      margin: 1rem 0;
      background: #1f2937;
      border-radius: 8px;
      overflow: hidden;
    }
    .code-block .code-title {
      background: #111827;
      color: #9ca3af;
      padding: 0.5rem 1rem;
      font-size: 0.875rem;
      border-bottom: 1px solid #374151;
    }
    .code-block pre {
      padding: 1rem;
      overflow-x: auto;
      color: #e5e7eb;
      font-family: 'Consolas', 'Monaco', monospace;
      font-size: 0.875rem;
      line-height: 1.5;
    }
    .code-block .line-numbers {
      display: inline-block;
      margin-right: 1rem;
      color: #6b7280;
      user-select: none;
      text-align: right;
    }

    /* Table (generic) */
    .table-wrapper {
      margin: 1rem 0;
      overflow-x: auto;
    }
    .table-wrapper table {
      width: 100%;
      border-collapse: collapse;
    }
    .table-wrapper th,
    .table-wrapper td {
      padding: 0.75rem;
      text-align: left;
      border-bottom: 1px solid var(--color-border);
    }
    .table-wrapper th {
      background: var(--color-bg-muted);
      font-weight: bold;
    }
    .table-wrapper.striped tr:nth-child(even) td {
      background: var(--color-bg-muted);
    }
    .table-wrapper.compact th,
    .table-wrapper.compact td {
      padding: 0.375rem 0.5rem;
    }

    /* i18n language switching */
    html[lang="en"] .i18n-zh { display: none; }
    html[lang="zh"] .i18n-en { display: none; }

    .lang-switcher {
      position: fixed;
      top: 1rem;
      right: 1rem;
      z-index: 1000;
      display: flex;
      gap: 0;
      border-radius: 6px;
      overflow: hidden;
      border: 1px solid var(--color-border);
      background: var(--color-bg);
      box-shadow: 0 2px 8px rgba(0,0,0,0.1);
    }
    .lang-switcher button {
      padding: 0.375rem 0.75rem;
      border: none;
      background: var(--color-bg);
      color: var(--color-text-muted);
      cursor: pointer;
      font-size: 0.8rem;
      font-weight: 500;
      transition: background 0.2s, color 0.2s;
    }
    .lang-switcher button:hover {
      background: var(--color-bg-muted);
    }
    .lang-switcher button.active {
      background: var(--color-primary);
      color: white;
    }

    /* Print styles */
    @media print {
      body { padding: 0; }
      .link-group { display: none; }
      .lang-switcher { display: none; }
    }
    /* Broken image fallback */
    .image img[data-failed],
    .figure img[data-failed] {
      display: none;
    }
    .img-fallback {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      background: var(--color-bg-muted);
      border: 2px dashed var(--color-border);
      border-radius: 8px;
      padding: 2rem;
      color: var(--color-text-muted);
      font-size: 0.9rem;
      min-height: 200px;
      width: 100%;
      max-width: 600px;
    }

    /* 2026 visual refresh */
    :root {
      --color-primary: #0f766e;
      --color-primary-strong: #115e59;
      --color-success: #059669;
      --color-warning: #d97706;
      --color-danger: #dc2626;
      --color-text: #0f172a;
      --color-text-muted: #475569;
      --color-bg: #f3f6fb;
      --color-bg-muted: #e9eef6;
      --color-surface: #ffffff;
      --color-surface-alt: #f8fbff;
      --color-border: rgba(15, 23, 42, 0.12);
      --color-highlight: #fef08a;
      --color-shadow: rgba(15, 23, 42, 0.14);
      --color-shadow-soft: rgba(15, 23, 42, 0.08);
      --color-code-bg: #0f172a;
      --color-code-surface: #111b2f;
      --color-code-text: #dbe7ff;
    }

    @media (prefers-color-scheme: dark) {
      :root {
        --color-primary: #22c1aa;
        --color-primary-strong: #34d3bd;
        --color-success: #34d399;
        --color-warning: #fbbf24;
        --color-danger: #f87171;
        --color-text: #e5edf7;
        --color-text-muted: #9db0c8;
        --color-bg: #050912;
        --color-bg-muted: #0f1727;
        --color-surface: #0b1322;
        --color-surface-alt: #101b30;
        --color-border: rgba(148, 163, 184, 0.3);
        --color-highlight: #854d0e;
        --color-shadow: rgba(2, 6, 23, 0.62);
        --color-shadow-soft: rgba(2, 6, 23, 0.42);
        --color-code-bg: #020712;
        --color-code-surface: #0d172c;
        --color-code-text: #dbe7ff;
      }
    }

    body {
      font-family: 'Manrope', 'Avenir Next', 'Segoe UI', sans-serif;
      line-height: 1.68;
      letter-spacing: 0.01em;
      color: var(--color-text);
      background:
        radial-gradient(1200px 520px at 8% -14%, rgba(14, 165, 233, 0.2), transparent 60%),
        radial-gradient(1000px 520px at 95% -10%, rgba(16, 185, 129, 0.18), transparent 58%),
        var(--color-bg);
      min-height: 100vh;
      padding: clamp(1rem, 2vw, 2.5rem);
      -webkit-font-smoothing: antialiased;
      text-rendering: optimizeLegibility;
    }

    @media (prefers-color-scheme: dark) {
      body {
        background:
          radial-gradient(1200px 540px at 10% -14%, rgba(14, 165, 233, 0.14), transparent 64%),
          radial-gradient(1000px 520px at 90% -8%, rgba(34, 197, 170, 0.14), transparent 62%),
          var(--color-bg);
      }
    }

    a {
      color: var(--color-primary);
      text-underline-offset: 0.14em;
    }

    a:hover {
      color: var(--color-primary-strong);
    }

    .report {
      max-width: 920px;
      margin: 0 auto;
      padding: clamp(1.25rem, 2.4vw, 2.75rem);
      background: linear-gradient(160deg, var(--color-surface) 0%, var(--color-surface-alt) 100%);
      border: 1px solid var(--color-border);
      border-radius: 24px;
      box-shadow: 0 30px 70px var(--color-shadow), 0 8px 24px var(--color-shadow-soft);
    }

    .brand-header {
      background: linear-gradient(125deg, rgba(15, 118, 110, 0.12), rgba(14, 165, 233, 0.08));
      border: 1px solid var(--color-border);
      border-radius: 16px;
      padding: 0.85rem 1rem;
      margin-bottom: 1.5rem;
      display: flex;
      justify-content: space-between;
      align-items: center;
      gap: 0.8rem;
      flex-wrap: wrap;
    }

    .brand-header > span:first-child {
      background: var(--color-surface);
      border: 1px solid var(--color-border);
      border-radius: 999px;
      padding: 0.25rem 0.6rem;
      font-size: 0.74rem;
      font-weight: 700;
      letter-spacing: 0.08em;
      text-transform: uppercase;
    }

    .brand-header .powered-by {
      color: var(--color-text-muted);
      font-size: 0.84rem;
    }

    .paper-header {
      margin-bottom: 1.6rem;
    }

    .paper-header h1 {
      font-size: clamp(1.6rem, 2.6vw, 2.35rem);
      line-height: 1.24;
      letter-spacing: -0.01em;
      margin-bottom: 0.75rem;
    }

    .paper-header .meta {
      gap: 0.65rem;
      flex-wrap: wrap;
      font-size: 0.84rem;
    }

    .paper-header .meta span {
      background: var(--color-bg-muted);
      border: 1px solid var(--color-border);
      border-radius: 999px;
      padding: 0.25rem 0.65rem;
    }

    .paper-header .categories,
    .categories {
      margin-top: 0.85rem;
      display: flex;
      gap: 0.5rem;
      flex-wrap: wrap;
    }

    .paper-header .category,
    .categories .category {
      background: var(--color-bg-muted);
      border: 1px solid var(--color-border);
      border-radius: 999px;
      padding: 0.2rem 0.62rem;
      font-size: 0.75rem;
      color: var(--color-text);
      text-decoration: none;
    }

    .authors {
      margin-bottom: 1.2rem;
      color: var(--color-text);
      background: var(--color-bg-muted);
      border: 1px solid var(--color-border);
      border-radius: 12px;
      padding: 0.72rem 0.9rem;
    }

    .section {
      margin-bottom: 1.8rem;
    }

    .section h2 {
      border-bottom: 1px solid var(--color-border);
      padding-bottom: 0.7rem;
      margin-bottom: 1rem;
      font-size: 1.08rem;
      font-weight: 700;
      letter-spacing: 0.02em;
    }

    .section h2 > span:first-child {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      width: 1.7rem;
      height: 1.7rem;
      border-radius: 999px;
      background: var(--color-bg-muted);
      border: 1px solid var(--color-border);
      font-size: 0.85rem;
    }

    .abstract {
      color: var(--color-text);
      font-size: 1.01rem;
    }

    .abstract mark {
      background: var(--color-highlight);
      border-radius: 4px;
      padding: 0 0.2rem;
    }

    .contribution-list {
      list-style: decimal;
      padding-left: 1.25rem;
      display: flex;
      flex-direction: column;
      gap: 0.7rem;
    }

    .contribution-list li {
      margin-bottom: 0;
      background: var(--color-bg-muted);
      border: 1px solid var(--color-border);
      border-radius: 12px;
      padding: 0.8rem 0.95rem;
    }

    .contribution-list li::marker {
      color: var(--color-primary);
      font-weight: 700;
    }

    .contribution-list .badge {
      background: linear-gradient(135deg, var(--color-primary), var(--color-primary-strong));
      border-radius: 999px;
      padding: 0.15rem 0.55rem;
      font-size: 0.72rem;
      font-weight: 700;
    }

    .method-overview {
      gap: 0.85rem;
    }

    .method-step {
      background: var(--color-bg-muted);
      border: 1px solid var(--color-border);
      border-radius: 12px;
      padding: 0.75rem 0.9rem;
    }

    .method-step .number {
      width: 1.9rem;
      height: 1.9rem;
      background: linear-gradient(135deg, var(--color-primary), var(--color-primary-strong));
      font-size: 0.84rem;
      box-shadow: 0 6px 14px rgba(15, 118, 110, 0.22);
    }

    .highlight {
      border-left: none;
      border: 1px solid var(--color-border);
      border-radius: 12px;
      padding: 0.9rem 1rem;
      background: var(--color-bg-muted);
    }

    .highlight.quote {
      background: rgba(14, 165, 233, 0.1);
      border-color: rgba(14, 165, 233, 0.28);
    }

    .highlight.important {
      background: rgba(245, 158, 11, 0.12);
      border-color: rgba(245, 158, 11, 0.3);
    }

    .highlight.warning {
      background: rgba(239, 68, 68, 0.11);
      border-color: rgba(239, 68, 68, 0.3);
    }

    .highlight.code {
      background: rgba(16, 185, 129, 0.11);
      border-color: rgba(16, 185, 129, 0.32);
      font-family: 'JetBrains Mono', 'SF Mono', 'Consolas', monospace;
    }

    .metrics-grid {
      margin: 1.1rem 0;
      gap: 0.8rem;
    }

    .metric {
      background: var(--color-surface);
      border: 1px solid var(--color-border);
      border-radius: 14px;
      padding: 1rem 0.8rem;
      box-shadow: 0 8px 18px var(--color-shadow-soft);
    }

    .metric .value {
      font-size: 1.68rem;
      letter-spacing: -0.02em;
      font-weight: 800;
    }

    .link-group {
      gap: 0.6rem;
      align-items: flex-start;
      margin: 1rem 0;
    }

    .link-button {
      background: linear-gradient(135deg, var(--color-primary), var(--color-primary-strong));
      border: 1px solid rgba(255, 255, 255, 0.18);
      border-radius: 999px;
      padding: 0.5rem 0.92rem;
      font-weight: 600;
      box-shadow: 0 10px 22px rgba(15, 118, 110, 0.25);
      transition: transform 0.15s ease, box-shadow 0.15s ease, opacity 0.15s ease;
    }

    .link-button:hover {
      opacity: 1;
      transform: translateY(-1px);
      box-shadow: 0 14px 24px rgba(15, 118, 110, 0.28);
    }

    .brand-footer {
      margin-top: 2.2rem;
      padding-top: 1.1rem;
      border-top: 1px solid var(--color-border);
      color: var(--color-text-muted);
      font-size: 0.84rem;
      display: flex;
      flex-direction: column;
      gap: 0.45rem;
    }

    .brand-footer p:first-child {
      border: 1px solid var(--color-border);
      background: var(--color-bg-muted);
      border-radius: 10px;
      padding: 0.55rem 0.72rem;
      color: var(--color-text);
    }

    .grid {
      gap: 0.85rem;
    }

    .card {
      background: var(--color-surface);
      border: 1px solid var(--color-border);
      border-radius: 14px;
    }

    .card.shadow {
      box-shadow: 0 10px 26px var(--color-shadow-soft);
    }

    .figure img,
    .image img {
      border-radius: 12px;
      border: 1px solid var(--color-border);
      background: var(--color-surface-alt);
      box-shadow: 0 8px 20px var(--color-shadow-soft);
    }

    .figure .images {
      gap: 0.85rem;
    }

    .figure figcaption,
    .image .caption {
      color: var(--color-text-muted);
      font-size: 0.82rem;
    }

    .formula {
      margin: 1.15rem 0;
    }

    .formula.block {
      background: var(--color-bg-muted);
      border: 1px solid var(--color-border);
      border-radius: 12px;
      padding: 1rem;
    }

    .formula .label {
      color: var(--color-text-muted);
      font-size: 0.8rem;
    }

    .formula code {
      font-size: 1.05rem;
    }

    .prose {
      color: var(--color-text);
      line-height: 1.8;
    }

    .prose code {
      font-family: 'JetBrains Mono', 'SF Mono', 'Consolas', monospace;
      background: var(--color-bg-muted);
      border: 1px solid var(--color-border);
      border-radius: 6px;
      padding: 0.12rem 0.42rem;
      font-size: 0.86em;
    }

    .callout {
      border-left: none;
      border: 1px solid var(--color-border);
      border-radius: 12px;
      padding: 0.9rem 1rem;
      background: var(--color-bg-muted);
    }

    .callout.info {
      border-color: rgba(14, 165, 233, 0.3);
      background: rgba(14, 165, 233, 0.1);
    }

    .callout.tip {
      border-color: rgba(16, 185, 129, 0.3);
      background: rgba(16, 185, 129, 0.1);
    }

    .callout.warning {
      border-color: rgba(245, 158, 11, 0.3);
      background: rgba(245, 158, 11, 0.1);
    }

    .callout.important {
      border-color: rgba(239, 68, 68, 0.3);
      background: rgba(239, 68, 68, 0.1);
    }

    .callout.note {
      border-color: rgba(148, 163, 184, 0.45);
      background: rgba(148, 163, 184, 0.14);
    }

    .definition-list dt {
      color: var(--color-text);
      font-weight: 700;
    }

    .definition-list dd {
      margin-left: 0;
      padding-left: 0.75rem;
      border-left: 2px solid var(--color-border);
      color: var(--color-text-muted);
    }

    .theorem {
      border: 1px solid var(--color-border);
      border-left: 4px solid var(--color-primary);
      border-radius: 12px;
      background: var(--color-bg-muted);
      padding: 0.95rem 1.15rem;
    }

    .theorem .theorem-header {
      color: var(--color-primary-strong);
    }

    .theorem.lemma {
      border-left-color: #0284c7;
    }

    .theorem.lemma .theorem-header {
      color: #0284c7;
    }

    .theorem.proposition {
      border-left-color: var(--color-success);
    }

    .theorem.proposition .theorem-header {
      color: var(--color-success);
    }

    .theorem.definition {
      border-left-color: var(--color-warning);
    }

    .theorem.definition .theorem-header {
      color: var(--color-warning);
    }

    .algorithm {
      border: 1px solid var(--color-border);
      border-radius: 14px;
      overflow: hidden;
      background: var(--color-bg-muted);
    }

    .algorithm .algorithm-title {
      background: linear-gradient(135deg, var(--color-primary), var(--color-primary-strong));
    }

    .algorithm .algorithm-body {
      font-family: 'JetBrains Mono', 'SF Mono', 'Consolas', monospace;
      background: var(--color-surface);
    }

    .results-table,
    .table-wrapper {
      border: 1px solid var(--color-border);
      border-radius: 14px;
      background: var(--color-surface);
      overflow-x: auto;
    }

    .results-table table,
    .table-wrapper table {
      width: 100%;
      border-collapse: collapse;
      min-width: 640px;
    }

    .results-table th,
    .results-table td,
    .table-wrapper th,
    .table-wrapper td {
      border-bottom: 1px solid var(--color-border);
      padding: 0.72rem 0.8rem;
    }

    .results-table th,
    .table-wrapper th {
      background: var(--color-bg-muted);
      font-weight: 700;
      font-size: 0.84rem;
      letter-spacing: 0.01em;
    }

    .results-table td.highlight {
      background: rgba(14, 165, 233, 0.16);
    }

    .results-table caption,
    .table-wrapper caption {
      display: block;
      margin: 0.6rem 0 0.2rem;
      font-size: 0.82rem;
      color: var(--color-text-muted);
      padding-left: 0.15rem;
      text-align: left;
    }

    .code-block {
      background: var(--color-code-bg);
      border: 1px solid rgba(148, 163, 184, 0.32);
      border-radius: 14px;
      box-shadow: 0 16px 28px var(--color-shadow-soft);
    }

    .code-block .code-title {
      background: var(--color-code-surface);
      color: #b7c8e7;
      border-bottom: 1px solid rgba(148, 163, 184, 0.28);
      font-weight: 600;
    }

    .code-block pre {
      color: var(--color-code-text);
      font-family: 'JetBrains Mono', 'SF Mono', 'Consolas', monospace;
      font-size: 0.84rem;
    }

    .code-block .line-numbers {
      color: #7487a6;
    }

    .lang-switcher {
      border-radius: 999px;
      border: 1px solid var(--color-border);
      background: rgba(255, 255, 255, 0.86);
      backdrop-filter: blur(10px);
      box-shadow: 0 14px 26px var(--color-shadow-soft);
      padding: 2px;
      gap: 2px;
    }

    @media (prefers-color-scheme: dark) {
      .lang-switcher {
        background: rgba(11, 19, 34, 0.86);
      }
    }

    .lang-switcher button {
      border-radius: 999px;
      background: transparent;
      font-size: 0.78rem;
      font-weight: 700;
      padding: 0.34rem 0.72rem;
      min-width: 2.65rem;
      color: var(--color-text-muted);
      transition: transform 0.15s ease, background 0.2s ease, color 0.2s ease;
    }

    .lang-switcher button:hover {
      background: var(--color-bg-muted);
      transform: translateY(-1px);
    }

    .lang-switcher button.active {
      background: linear-gradient(135deg, var(--color-primary), var(--color-primary-strong));
      color: #ffffff;
    }

    .img-fallback {
      border: 1px dashed var(--color-border);
      border-radius: 12px;
      background: var(--color-bg-muted);
      color: var(--color-text-muted);
    }

    @media (max-width: 860px) {
      body {
        padding: 0.9rem;
      }

      .report {
        border-radius: 18px;
        padding: 1.05rem;
      }

      .lang-switcher {
        position: sticky;
        top: 0.6rem;
        right: auto;
        margin: 0 0 0.7rem auto;
        width: max-content;
      }

      .metrics-grid {
        grid-template-columns: repeat(2, minmax(0, 1fr)) !important;
      }

      .grid {
        grid-template-columns: 1fr !important;
      }
    }

    @media (max-width: 560px) {
      .brand-header {
        align-items: flex-start;
      }

      .paper-header .meta span {
        width: 100%;
      }

      .link-group {
        flex-direction: column;
      }

      .link-button {
        width: 100%;
        justify-content: center;
      }

      .method-step {
        flex-direction: column;
        gap: 0.6rem;
      }

      .results-table table,
      .table-wrapper table {
        min-width: 520px;
      }
    }

    /* 2026 v2 editorial refresh */
    :root {
      --color-primary: #0b57d0;
      --color-primary-strong: #003eaa;
      --color-success: #1b8f5c;
      --color-warning: #b16a1b;
      --color-danger: #c23a3a;
      --color-text: #10131a;
      --color-text-muted: #565f6f;
      --color-bg: #f3f3ef;
      --color-bg-muted: #e9e9e2;
      --color-surface: #fcfcf8;
      --color-surface-alt: #f7f7f2;
      --color-border: #d4d6cf;
      --color-highlight: #f2dc72;
      --color-shadow: rgba(16, 19, 26, 0.12);
      --color-shadow-soft: rgba(16, 19, 26, 0.06);
      --color-code-bg: #0c1220;
      --color-code-surface: #121a2e;
      --color-code-text: #dbe4ff;
    }

    @media (prefers-color-scheme: dark) {
      :root {
        --color-primary: #8ab4ff;
        --color-primary-strong: #aac7ff;
        --color-success: #4ecb92;
        --color-warning: #f2be6d;
        --color-danger: #ff8b8b;
        --color-text: #ebf0fb;
        --color-text-muted: #9ea9be;
        --color-bg: #0b0e13;
        --color-bg-muted: #151a22;
        --color-surface: #0f141d;
        --color-surface-alt: #141b26;
        --color-border: #2a313d;
        --color-highlight: #6b5316;
        --color-shadow: rgba(0, 0, 0, 0.55);
        --color-shadow-soft: rgba(0, 0, 0, 0.35);
        --color-code-bg: #040913;
        --color-code-surface: #0c1424;
        --color-code-text: #dbe4ff;
      }
    }

    body {
      font-family: 'Space Grotesk', 'Avenir Next', 'Segoe UI', sans-serif;
      line-height: 1.72;
      letter-spacing: 0.005em;
      background: var(--color-bg);
      padding: clamp(1rem, 2.3vw, 2.8rem);
      position: relative;
      overflow-x: hidden;
    }

    body::before {
      content: '';
      position: fixed;
      inset: 0;
      pointer-events: none;
      background-image:
        linear-gradient(rgba(16, 19, 26, 0.03) 1px, transparent 1px),
        linear-gradient(90deg, rgba(16, 19, 26, 0.03) 1px, transparent 1px);
      background-size: 36px 36px;
      opacity: 0.35;
      z-index: 0;
    }

    @media (prefers-color-scheme: dark) {
      body::before {
        background-image:
          linear-gradient(rgba(235, 240, 251, 0.04) 1px, transparent 1px),
          linear-gradient(90deg, rgba(235, 240, 251, 0.04) 1px, transparent 1px);
        opacity: 0.22;
      }
    }

    .report {
      position: relative;
      z-index: 1;
      max-width: 960px;
      padding: clamp(1.3rem, 2.5vw, 2.9rem);
      border-radius: 14px;
      border: 1px solid var(--color-border);
      background: var(--color-surface);
      box-shadow: 0 22px 50px var(--color-shadow);
    }

    .brand-header {
      border: 1px solid var(--color-border);
      border-radius: 10px;
      background: var(--color-surface-alt);
      padding: 0.75rem 0.85rem;
    }

    .brand-header > span:first-child {
      font-family: 'IBM Plex Mono', 'SF Mono', 'Consolas', monospace;
      font-size: 0.73rem;
      letter-spacing: 0.06em;
      text-transform: none;
      border-radius: 6px;
      padding: 0.2rem 0.46rem;
      background: transparent;
    }

    .brand-header .powered-by {
      font-family: 'IBM Plex Mono', 'SF Mono', 'Consolas', monospace;
      font-size: 0.72rem;
      letter-spacing: 0.02em;
    }

    .brand-header .powered-by a,
    .brand-header .powered-by a:visited {
      color: inherit;
      text-decoration: none;
      font-weight: 600;
    }

    .brand-header .powered-by a:hover {
      text-decoration: underline;
      text-underline-offset: 0.14em;
    }

    .paper-header h1 {
      font-size: clamp(1.75rem, 3vw, 2.55rem);
      line-height: 1.16;
      letter-spacing: -0.03em;
      max-width: 20ch;
    }

    .paper-header .meta span {
      font-family: 'IBM Plex Mono', 'SF Mono', 'Consolas', monospace;
      font-size: 0.72rem;
      border-radius: 6px;
    }

    .paper-header .categories,
    .categories {
      gap: 0.4rem;
    }

    .paper-header .category,
    .categories .category {
      border-radius: 6px;
      padding: 0.19rem 0.45rem;
      font-size: 0.7rem;
      font-family: 'IBM Plex Mono', 'SF Mono', 'Consolas', monospace;
      letter-spacing: 0.02em;
    }

    .authors {
      border-radius: 10px;
      font-size: 0.92rem;
    }

    .section h2 {
      border-bottom: none;
      font-size: 0.76rem;
      letter-spacing: 0.16em;
      text-transform: uppercase;
      color: var(--color-text-muted);
      gap: 0.55rem;
      margin-bottom: 0.85rem;
    }

    .section h2::after {
      content: '';
      flex: 1 1 auto;
      height: 1px;
      background: var(--color-border);
    }

    .section h2 > span:first-child {
      width: auto;
      height: auto;
      min-width: 1.7rem;
      border-radius: 6px;
      padding: 0.16rem 0.36rem;
      font-family: 'IBM Plex Mono', 'SF Mono', 'Consolas', monospace;
      font-size: 0.66rem;
      line-height: 1.2;
      text-transform: uppercase;
      background: transparent;
      border: 1px solid var(--color-border);
    }

    .contribution-list {
      list-style: none;
      padding-left: 0;
      counter-reset: contribution;
      gap: 0;
    }

    .contribution-list li {
      position: relative;
      counter-increment: contribution;
      border: none;
      border-bottom: 1px dashed var(--color-border);
      border-radius: 0;
      background: transparent;
      padding: 0.86rem 0.1rem 0.9rem 2.5rem;
    }

    .contribution-list li::before {
      content: counter(contribution, decimal-leading-zero);
      position: absolute;
      left: 0;
      top: 0.88rem;
      font-family: 'IBM Plex Mono', 'SF Mono', 'Consolas', monospace;
      font-size: 0.75rem;
      color: var(--color-primary);
      letter-spacing: 0.04em;
    }

    .contribution-list li::marker {
      content: '';
    }

    .contribution-list .badge {
      font-family: 'IBM Plex Mono', 'SF Mono', 'Consolas', monospace;
      border-radius: 5px;
      padding: 0.1rem 0.38rem;
      font-size: 0.66rem;
      box-shadow: none;
    }

    .method-step {
      border: none;
      border-left: 2px solid var(--color-border);
      border-radius: 0;
      background: transparent;
      padding: 0.28rem 0 0.4rem 0.85rem;
      gap: 0.7rem;
    }

    .method-step .number {
      width: 1.6rem;
      height: 1.6rem;
      border-radius: 6px;
      font-family: 'IBM Plex Mono', 'SF Mono', 'Consolas', monospace;
      font-size: 0.72rem;
      box-shadow: none;
    }

    .highlight,
    .callout,
    .theorem,
    .authors {
      border-radius: 10px;
    }

    .highlight,
    .callout {
      border-left: 1px solid var(--color-border);
    }

    .highlight.code,
    .prose code,
    .formula code,
    .algorithm .algorithm-body,
    .algorithm .line-number,
    .algorithm .line-code,
    .code-block pre,
    .code-block .line-numbers,
    .definition-list dt,
    .definition-list dd {
      font-family: 'IBM Plex Mono', 'SF Mono', 'Consolas', monospace;
    }

    .metrics-grid {
      gap: 0.55rem;
    }

    .metric {
      border-radius: 8px;
      box-shadow: none;
      padding: 0.82rem 0.64rem;
    }

    .metric .value {
      font-size: 1.55rem;
    }

    .metric .label {
      font-family: 'IBM Plex Mono', 'SF Mono', 'Consolas', monospace;
      font-size: 0.7rem;
      letter-spacing: 0.03em;
      text-transform: uppercase;
    }

    .link-button {
      border-radius: 8px;
      border: 1px solid var(--color-border);
      background: transparent;
      color: var(--color-text);
      box-shadow: none;
      font-size: 0.8rem;
      padding: 0.5rem 0.72rem;
      font-weight: 600;
    }

    .link-button:hover {
      transform: translateY(-1px);
      background: var(--color-primary);
      color: #ffffff;
      border-color: var(--color-primary);
      box-shadow: none;
    }

    .brand-footer {
      border-top: 1px dashed var(--color-border);
      gap: 0.35rem;
    }

    .brand-footer p:first-child {
      border-style: dashed;
      background: transparent;
    }

    .card,
    .results-table,
    .table-wrapper,
    .figure img,
    .image img,
    .formula.block,
    .algorithm,
    .code-block,
    .img-fallback {
      border-radius: 10px;
    }

    .code-block {
      box-shadow: none;
    }

    .code-block .code-title {
      font-family: 'IBM Plex Mono', 'SF Mono', 'Consolas', monospace;
      font-size: 0.76rem;
      letter-spacing: 0.04em;
      text-transform: uppercase;
    }

    .lang-switcher {
      border-radius: 9px;
      padding: 1px;
      gap: 1px;
      box-shadow: none;
      backdrop-filter: none;
    }

    .lang-switcher button {
      border-radius: 7px;
      min-width: 2.45rem;
      font-size: 0.72rem;
      font-family: 'IBM Plex Mono', 'SF Mono', 'Consolas', monospace;
      letter-spacing: 0.03em;
      transform: none;
    }

    .lang-switcher button:hover {
      transform: none;
    }

    .lang-switcher button.active {
      background: var(--color-text);
      color: var(--color-surface);
    }

    .callout .callout-title::before {
      content: '';
      display: none;
    }

    @media (max-width: 860px) {
      body {
        padding: 0.85rem;
      }

      .report {
        border-radius: 10px;
        padding: 1.05rem;
      }

      .lang-switcher {
        position: sticky;
        top: 0.52rem;
        margin-bottom: 0.7rem;
      }
    }

    @media (max-width: 560px) {
      .paper-header h1 {
        max-width: none;
      }

      .section h2 {
        letter-spacing: 0.12em;
      }

      .results-table table,
      .table-wrapper table {
        min-width: 500px;
      }
    }

    /* 2026 v4 wireframe style */
    :root {
      --color-primary: #ff4020;
      --color-primary-strong: #ff4020;
      --color-success: #2db37a;
      --color-warning: #d08b3a;
      --color-danger: #d65b5b;
      --color-text: #f2f3f5;
      --color-text-muted: #9ea3ad;
      --color-bg: #050608;
      --color-bg-muted: #0a0d12;
      --color-surface: #050608;
      --color-surface-alt: #050608;
      --color-border: rgba(255, 255, 255, 0.14);
      --color-highlight: rgba(255, 208, 97, 0.28);
      --color-shadow: transparent;
      --color-shadow-soft: transparent;
      --color-code-bg: #070b14;
      --color-code-surface: #0d1220;
      --color-code-text: #d8e2ff;
    }

    @media (prefers-color-scheme: light) {
      :root {
        --color-primary: #e5371a;
        --color-primary-strong: #e5371a;
        --color-success: #1b8f5c;
        --color-warning: #a96a24;
        --color-danger: #bf4444;
        --color-text: #151820;
        --color-text-muted: #5f6572;
        --color-bg: #f6f7f9;
        --color-bg-muted: #eceef2;
        --color-surface: #f6f7f9;
        --color-surface-alt: #f6f7f9;
        --color-border: rgba(21, 24, 32, 0.18);
        --color-highlight: rgba(229, 55, 26, 0.16);
        --color-code-bg: #101724;
        --color-code-surface: #161f2f;
        --color-code-text: #e1ebff;
      }
    }

    body {
      font-family: 'Space Grotesk', 'Avenir Next', 'Segoe UI', sans-serif;
      background: var(--color-bg);
      color: var(--color-text);
      padding: 0;
      overflow-x: hidden;
    }

    body::before {
      display: none;
    }

    .report {
      max-width: 100%;
      margin: 0;
      padding: 0;
      border: none;
      border-radius: 0;
      background: transparent;
      box-shadow: none;
    }

    .brand-header {
      margin: 0;
      min-height: 68px;
      padding: 0.9rem clamp(1rem, 10vw, 180px);
      border-top: 1px solid var(--color-border);
      border-bottom: 1px solid var(--color-border);
      border-left: none;
      border-right: none;
      border-radius: 0;
      background: transparent;
      display: flex;
      justify-content: space-between;
      align-items: center;
      gap: 0.85rem;
      flex-wrap: nowrap;
    }

    .brand-header > span:first-child {
      display: inline-flex;
      align-items: center;
      gap: 0.55rem;
      border: none;
      border-radius: 0;
      padding: 0;
      font-family: 'IBM Plex Mono', 'SF Mono', 'Consolas', monospace;
      font-size: 0.9rem;
      letter-spacing: 0.03em;
      text-transform: uppercase;
      background: transparent;
    }

    .brand-header > span:first-child::before {
      content: '';
      width: 12px;
      height: 12px;
      background: var(--color-primary);
      display: inline-block;
      flex-shrink: 0;
    }

    .brand-header .powered-by {
      font-size: 0.8rem;
      font-family: 'IBM Plex Mono', 'SF Mono', 'Consolas', monospace;
      letter-spacing: 0.02em;
      color: var(--color-text-muted);
    }

    .fallback-lang-row {
      margin: 0;
      padding: 0.9rem clamp(1rem, 10vw, 180px);
      border-top: 1px solid var(--color-border);
      border-bottom: 1px solid var(--color-border);
      display: flex;
      justify-content: flex-end;
      align-items: center;
      background: transparent;
    }

    .fallback-lang-row .lang-switcher {
      margin-left: 0;
    }

    .report-hero {
      margin: 0;
      padding: clamp(1.2rem, 3.8vw, 3rem) clamp(1rem, 10vw, 180px);
      border-top: 1px solid var(--color-border);
      border-bottom: 1px solid var(--color-border);
      background: transparent;
    }

    .hero-meta {
      display: flex;
      align-items: center;
      gap: 0.5rem;
      flex-wrap: wrap;
      margin-bottom: 0.9rem;
    }

    .hero-chip {
      display: inline-flex;
      align-items: center;
      min-height: 34px;
      border: 1px solid var(--color-border);
      border-radius: 0;
      padding: 0.32rem 0.74rem;
      font-family: 'IBM Plex Mono', 'SF Mono', 'Consolas', monospace;
      font-size: 0.74rem;
      letter-spacing: 0.03em;
      text-transform: uppercase;
      color: var(--color-text);
      background: transparent;
    }

    .hero-chip-primary {
      background: var(--color-primary);
      border-color: var(--color-primary);
      color: #ffffff;
    }

    .hero-title {
      margin: 0;
      font-size: clamp(2.2rem, 6.3vw, 5.4rem);
      line-height: 0.97;
      letter-spacing: -0.04em;
      max-width: 16ch;
    }

    .hero-subtitle {
      margin-top: 1rem;
      max-width: 58ch;
      color: var(--color-text-muted);
      font-size: 1.02rem;
      line-height: 1.58;
    }

    .section {
      margin: 0;
      padding: clamp(1.25rem, 4vw, 3.2rem) clamp(1rem, 10vw, 180px);
      border-top: 1px solid var(--color-border);
      border-radius: 0;
    }

    .section:last-of-type {
      border-bottom: 1px solid var(--color-border);
    }

    .section h2 {
      margin-bottom: 1.25rem;
      border: none;
      border-radius: 0;
      padding: 0;
      font-family: 'IBM Plex Mono', 'SF Mono', 'Consolas', monospace;
      font-size: 0.78rem;
      letter-spacing: 0.14em;
      text-transform: uppercase;
      color: var(--color-text-muted);
      gap: 0.6rem;
    }

    .section h2::after {
      display: none;
    }

    .section h2 > span:first-child {
      width: auto;
      height: auto;
      min-width: 1.8rem;
      border: 1px solid var(--color-border);
      border-radius: 0;
      padding: 0.13rem 0.35rem;
      font-family: 'IBM Plex Mono', 'SF Mono', 'Consolas', monospace;
      font-size: 0.66rem;
      background: transparent;
      text-transform: uppercase;
    }

    .paper-header,
    .authors,
    .highlight,
    .callout,
    .metric,
    .card,
    .table-wrapper,
    .results-table,
    .formula.block,
    .algorithm,
    .code-block,
    .img-fallback,
    .brand-footer p:first-child,
    .paper-header .meta span,
    .paper-header .category,
    .categories .category,
    .link-button,
    .lang-switcher,
    .lang-switcher button {
      border-radius: 0;
    }

    .paper-header {
      margin: 0;
      padding: clamp(1.8rem, 5vw, 3.8rem) clamp(1rem, 10vw, 180px);
      border-top: 1px solid var(--color-border);
      border-bottom: 1px solid var(--color-border);
      background: transparent;
    }

    .paper-header h1 {
      font-size: clamp(2.15rem, 5.8vw, 5.1rem);
      line-height: 0.98;
      letter-spacing: -0.04em;
      margin-bottom: 1rem;
      max-width: 16ch;
    }

    .paper-header .meta span,
    .paper-header .category,
    .categories .category {
      font-family: 'IBM Plex Mono', 'SF Mono', 'Consolas', monospace;
      font-size: 0.76rem;
      letter-spacing: 0.03em;
    }

    .contribution-list li,
    .method-step,
    .metric,
    .callout,
    .highlight,
    .authors,
    .definition-list dd,
    .theorem,
    .algorithm,
    .table-wrapper,
    .results-table {
      border-color: var(--color-border);
      background: transparent;
      box-shadow: none;
    }

    .contribution-list li {
      border-left: 1px solid var(--color-border);
      padding-left: 2.2rem;
    }

    .metric .label,
    .algorithm .algorithm-title,
    .code-block .code-title {
      font-family: 'IBM Plex Mono', 'SF Mono', 'Consolas', monospace;
    }

    .link-button {
      background: transparent;
      border: 1px solid var(--color-border);
      color: var(--color-text);
      box-shadow: none;
      font-family: 'IBM Plex Mono', 'SF Mono', 'Consolas', monospace;
      font-size: 0.75rem;
      letter-spacing: 0.04em;
      text-transform: uppercase;
    }

    .link-button:hover {
      background: var(--color-primary);
      border-color: var(--color-primary);
      color: #ffffff;
      box-shadow: none;
      transform: none;
    }

    .brand-footer {
      margin: 0;
      padding: 1.4rem clamp(1rem, 10vw, 180px) 3.4rem;
      border-top: 1px solid var(--color-border);
      border-bottom: 1px solid var(--color-border);
      font-size: 0.82rem;
      gap: 0.45rem;
    }

    .brand-footer p:first-child {
      border-style: solid;
      background: transparent;
      padding: 0.6rem 0.75rem;
    }

    .lang-switcher {
      position: static;
      top: auto;
      right: auto;
      z-index: auto;
      margin-left: auto;
      border: none;
      background: transparent;
      box-shadow: none;
      backdrop-filter: none;
      padding: 0;
      gap: 0.9rem;
    }

    .lang-switcher button {
      border: none;
      padding: 0;
      min-width: 0;
      background: transparent;
      color: var(--color-text-muted);
      font-family: 'IBM Plex Mono', 'SF Mono', 'Consolas', monospace;
      font-size: 0.9rem;
      letter-spacing: 0.02em;
    }

    .lang-switcher button:hover {
      transform: none;
      background: transparent;
      color: var(--color-text);
    }

    .lang-switcher button.active {
      background: transparent;
      color: var(--color-text);
      text-decoration: underline;
      text-underline-offset: 0.2em;
    }

    .corner-powered {
      position: fixed;
      right: clamp(0.8rem, 2vw, 1.4rem);
      bottom: clamp(0.8rem, 2vw, 1.35rem);
      z-index: 1200;
      display: inline-flex;
      flex-direction: column;
      gap: 0.12rem;
      min-width: 160px;
      border: 1px solid var(--color-border);
      border-radius: 0;
      background: rgba(10, 13, 18, 0.72);
      padding: 0.7rem 0.78rem;
      color: var(--color-text-muted);
      font-family: 'IBM Plex Mono', 'SF Mono', 'Consolas', monospace;
      font-size: 0.72rem;
      text-decoration: none;
    }

    .corner-powered strong {
      color: var(--color-text);
      font-family: 'Space Grotesk', 'Avenir Next', 'Segoe UI', sans-serif;
      font-size: 0.98rem;
      line-height: 1.12;
      letter-spacing: 0.01em;
    }

    .corner-powered:hover {
      border-color: var(--color-primary);
      color: var(--color-text);
    }

    @media (prefers-color-scheme: light) {
      .corner-powered {
        background: rgba(246, 247, 249, 0.9);
      }
    }

    @media (max-width: 860px) {
      .brand-header,
      .fallback-lang-row,
      .section,
      .paper-header,
      .brand-footer {
        padding-left: 1rem;
        padding-right: 1rem;
      }

      .lang-switcher {
        margin-left: auto;
      }

      .corner-powered {
        right: 0.7rem;
        bottom: 0.7rem;
        min-width: 136px;
      }
    }

    @media (max-width: 560px) {
      .paper-header h1 {
        font-size: clamp(1.85rem, 11vw, 2.8rem);
        line-height: 1.02;
      }

      .hero-chip {
        min-height: 30px;
        padding: 0.26rem 0.56rem;
        font-size: 0.66rem;
      }

      .hero-title {
        font-size: clamp(1.85rem, 11vw, 2.8rem);
        line-height: 1.02;
      }

      .section {
        padding-top: 1rem;
        padding-bottom: 1.35rem;
      }
    }

    @media print {
      body {
        padding: 0;
        background: #ffffff;
      }

      .report {
        border: none;
        box-shadow: none;
        border-radius: 0;
        padding: 0;
        background: #ffffff;
      }

      .corner-powered {
        display: none;
      }

      .fallback-lang-row {
        display: none;
      }
    }
  </style>
  <!-- KaTeX for LaTeX rendering -->
  <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/katex@0.16.11/dist/katex.min.css">
  <script defer src="https://cdn.jsdelivr.net/npm/katex@0.16.11/dist/katex.min.js"></script>
  <script>
    document.addEventListener('DOMContentLoaded', function() {
      // Render LaTeX formulas with KaTeX
      document.querySelectorAll('.formula[data-latex]').forEach(function(el) {
        var latex = el.getAttribute('data-latex');
        var isBlock = el.classList.contains('block');
        var label = el.querySelector('.label');
        try {
          var rendered = katex.renderToString(latex, {
            displayMode: isBlock,
            throwOnError: false,
            trust: true,
          });
          var container = el.querySelector('.formula-content');
          if (container) container.innerHTML = rendered;
        } catch(e) {
          // Keep raw LaTeX on error
        }
      });

      // i18n language switcher
      (function() {
        var saved = localStorage.getItem('json-ui-lang');
        if (saved && (saved === 'en' || saved === 'zh')) {
          document.documentElement.lang = saved;
        }
        var buttons = document.querySelectorAll('.lang-switcher button');
        function updateActive() {
          var lang = document.documentElement.lang || 'en';
          buttons.forEach(function(btn) {
            btn.classList.toggle('active', btn.getAttribute('data-lang') === lang);
          });
        }
        buttons.forEach(function(btn) {
          btn.addEventListener('click', function() {
            var lang = btn.getAttribute('data-lang');
            document.documentElement.lang = lang;
            localStorage.setItem('json-ui-lang', lang);
            updateActive();
          });
        });
        updateActive();
      })();

      // Handle broken images
      document.querySelectorAll('.image img, .figure img').forEach(function(img) {
        img.addEventListener('error', function() {
          img.setAttribute('data-failed', 'true');
          var fallback = document.createElement('div');
          fallback.className = 'img-fallback';
          fallback.textContent = 'Image: ' + (img.alt || 'unavailable');
          img.parentNode.insertBefore(fallback, img.nextSibling);
        });
      });
    });
  </script>
</head>
<body>
  <article class="report">
    ${renderNode(json)}
  </article>
  <a class="corner-powered" href="https://actionbook.dev" target="_blank" rel="noopener noreferrer">
    <span>Powered by</span>
    <strong>Actionbook</strong>
  </a>
</body>
</html>`;
}

// ============================================
// JSON Types
// ============================================

interface ReportJSON {
  type: string;
  props?: Record<string, unknown>;
  children?: ReportJSON[];
}

// ============================================
// Renderers
// ============================================

const iconMap: Record<string, string> = {
  paper: '::', user: 'o', calendar: '[]', tag: '#', link: '->', code: '&lt;/&gt;',
  chart: '||', bulb: '*', check: 'v', star: '*', warning: '!', info: 'i',
  github: 'gh', arxiv: 'arx', pdf: 'pdf', copy: 'cp', expand: '+', collapse: '-',
};

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function isGenericBadgeText(text: string): boolean {
  const raw = text.trim();
  if (!raw) return true;
  const normalized = raw.toLowerCase().replace(/\s+/g, ' ');
  return normalized.includes('deep research report')
    || normalized.includes('ai generated')
    || normalized.includes('ai-generated')
    || normalized.includes('paper summary')
    || normalized === 'research report'
    || raw.includes('深度研究报告')
    || raw.includes('ai 生成')
    || raw.includes('AI 生成')
    || raw.includes('论文解读');
}

function resolveBrandBadge(value: unknown): I18nValue {
  const fallback = 'ACTIVE_RESEARCH_REPORT.ARX';
  if (isI18n(value)) {
    const en = value.en?.trim() ?? '';
    const zh = value.zh?.trim() ?? '';
    const enGeneric = isGenericBadgeText(en);
    const zhGeneric = isGenericBadgeText(zh);
    if ((enGeneric || !en) && (zhGeneric || !zh)) return fallback;
    return {
      en: enGeneric || !en ? fallback : en,
      zh: zhGeneric || !zh ? fallback : zh,
    };
  }
  const text = String(value ?? '').trim();
  if (!text || isGenericBadgeText(text)) return fallback;
  return text;
}

function stripMarkdown(text: string): string {
  return text
    .replace(/`+/g, '')
    .replace(/\*\*/g, '')
    .replace(/\*/g, '')
    .replace(/^#+\s*/gm, '')
    .replace(/\[(.*?)\]\((.*?)\)/g, '$1')
    .replace(/\s+/g, ' ')
    .trim();
}

function firstSentence(text: string, maxLength = 64): string {
  const headlineMatch = text.match(/\*\*([^*]{3,120})\*\*/);
  if (headlineMatch) {
    const headline = stripMarkdown(headlineMatch[1]);
    if (headline) {
      if (headline.length <= maxLength) return headline;
      return `${headline.slice(0, maxLength).trimEnd()}…`;
    }
  }

  const plain = stripMarkdown(text);
  if (!plain) return '';
  const sentence = plain.split(/[。！？.!?]/)[0]?.trim() || plain;
  if (sentence.length <= maxLength) return sentence;
  return `${sentence.slice(0, maxLength).trimEnd()}…`;
}

function isGenericSectionTitle(value: unknown): boolean {
  const en = resolveI18n(value, 'en').trim().toLowerCase();
  const zh = resolveI18n(value, 'zh').trim();
  const genericEn = new Set([
    'overview',
    'summary',
    'abstract',
    'key findings',
    'detailed analysis',
    'sources',
    'key metrics',
    'introduction',
  ]);
  const genericZh = new Set([
    '概述',
    '摘要',
    '核心发现',
    '详细分析',
    '信息来源',
    '关键指标',
    '简介',
  ]);
  return genericEn.has(en) || genericZh.has(zh);
}

function extractHeroTitle(report: ReportJSON): I18nValue | null {
  const reportTitle = report.props?.title;
  if (reportTitle) return reportTitle as I18nValue;

  const brandTitle = report.children?.find((c) => c.type === 'BrandHeader' && c.props?.title)?.props?.title;
  if (brandTitle) return brandTitle as I18nValue;

  for (const child of report.children || []) {
    if (child.type !== 'Section') continue;
    for (const block of child.children || []) {
      if (block.type === 'Prose' && block.props?.content) {
        const content = block.props.content;
        if (isI18n(content)) {
          const en = firstSentence(content.en);
          const zh = firstSentence(content.zh);
          if (en || zh) return { en: en || 'Research Report', zh: zh || '研究报告' };
        }
        const text = firstSentence(String(content));
        if (text) return text;
      }
      if (block.type === 'Abstract' && block.props?.text) {
        const text = block.props.text;
        if (isI18n(text)) {
          const en = firstSentence(text.en);
          const zh = firstSentence(text.zh);
          if (en || zh) return { en: en || 'Research Report', zh: zh || '研究报告' };
        }
        const oneLine = firstSentence(String(text));
        if (oneLine) return oneLine;
      }
    }
  }

  for (const child of report.children || []) {
    if (child.type === 'Section' && child.props?.title && !isGenericSectionTitle(child.props.title)) {
      return child.props.title as I18nValue;
    }
  }

  return null;
}

function formatHeroDate(value: unknown): string | null {
  const raw = String(value ?? '').trim();
  if (!raw) return null;
  const parsed = new Date(raw);
  if (Number.isNaN(parsed.getTime())) return raw;
  const months = ['JAN', 'FEB', 'MAR', 'APR', 'MAY', 'JUN', 'JUL', 'AUG', 'SEP', 'OCT', 'NOV', 'DEC'];
  return `${months[parsed.getUTCMonth()]} ${String(parsed.getUTCDate()).padStart(2, '0')}, ${parsed.getUTCFullYear()}`;
}

function extractHeroDate(report: ReportJSON): string | null {
  if (report.props?.date) return formatHeroDate(report.props.date);
  for (const child of report.children || []) {
    if (child.type === 'BrandFooter' && child.props?.timestamp) {
      return formatHeroDate(child.props.timestamp);
    }
  }
  return null;
}

function extractHeroConfidence(report: ReportJSON): string | null {
  const value = report.props?.confidence ?? report.props?.conf;
  if (value == null) return null;
  if (typeof value === 'number') {
    const percentage = value <= 1 ? value * 100 : value;
    return `${percentage.toFixed(1)}%`;
  }
  const text = String(value).trim();
  return text || null;
}

function extractHeroId(report: ReportJSON): string | null {
  const id = report.props?.id
    ?? report.props?.reportId
    ?? report.props?.identifier
    ?? report.props?.refId;
  if (id == null) return null;
  const text = String(id).trim();
  return text || null;
}

function renderHero(report: ReportJSON, title: I18nValue): string {
  const label = (report.props?.heroLabel || 'ACTIVE RESEARCH') as I18nValue;
  const date = extractHeroDate(report);
  const confidence = extractHeroConfidence(report);
  const id = extractHeroId(report);
  const subtitle = report.props?.subtitle as I18nValue | undefined;

  const chips: string[] = [
    `<span class="hero-chip hero-chip-primary">${renderI18n(label)}</span>`,
  ];
  if (date) chips.push(`<span class="hero-chip">${escapeHtml(date)}</span>`);
  if (confidence) chips.push(`<span class="hero-chip">CONF: ${escapeHtml(confidence)}</span>`);
  if (id) chips.push(`<span class="hero-chip">ID: ${escapeHtml(id)}</span>`);

  return `<section class="report-hero">
    <div class="hero-meta">${chips.join('')}</div>
    <h1 class="hero-title">${renderI18n(title)}</h1>
    ${subtitle ? `<p class="hero-subtitle">${renderI18n(subtitle)}</p>` : ''}
  </section>`;
}

type RenderOptions = {
  showLanguageSwitcher: boolean;
};

function hasBilingualContent(value: unknown): boolean {
  if (isI18n(value)) {
    return Boolean(String(value.zh ?? '').trim());
  }

  if (Array.isArray(value)) {
    return value.some((item) => hasBilingualContent(item));
  }

  if (value && typeof value === 'object') {
    return Object.values(value as Record<string, unknown>).some((entry) => hasBilingualContent(entry));
  }

  return false;
}

function getRenderOptions(report: ReportJSON): RenderOptions {
  return {
    showLanguageSwitcher: hasBilingualContent(report),
  };
}

function renderLanguageSwitcher(extraClass = ''): string {
  const cls = extraClass ? `lang-switcher ${extraClass}` : 'lang-switcher';
  return `<div class="${cls}">
    <button data-lang="en" class="active">EN</button>
    <button data-lang="zh">中文</button>
  </div>`;
}

function renderNode(node: ReportJSON, options: RenderOptions = { showLanguageSwitcher: true }): string {
  const { type, props = {}, children = [] } = node;
  const childrenHtml = children.map((child) => renderNode(child, options)).join('\n');

  switch (type) {
    case 'Report': {
      const reportOptions = getRenderOptions(node);
      const hasPaperHeader = children.some((child) => child.type === 'PaperHeader');
      const hasBrandHeader = children.some((child) => child.type === 'BrandHeader');
      const renderedChildren = children.map((child) => renderNode(child, reportOptions));

      if (!hasBrandHeader && reportOptions.showLanguageSwitcher) {
        renderedChildren.unshift(`<div class="fallback-lang-row">${renderLanguageSwitcher('lang-switcher-fallback')}</div>`);
      }

      if (hasPaperHeader) return renderedChildren.join('\n');

      const heroTitle = extractHeroTitle(node);
      if (!heroTitle) return renderedChildren.join('\n');

      const heroHtml = renderHero(node, heroTitle);
      if (hasBrandHeader) {
        const brandHeaderIndex = children.findIndex((child) => child.type === 'BrandHeader');
        renderedChildren.splice(brandHeaderIndex + 1, 0, heroHtml);
      } else {
        renderedChildren.splice(1, 0, heroHtml);
      }
      return renderedChildren.join('\n');
    }

    case 'BrandHeader': {
      const badge = resolveBrandBadge(props.badge);
      return `<div class="brand-header">
        <span>${renderI18n(badge)}</span>
        ${options.showLanguageSwitcher ? renderLanguageSwitcher() : ''}
      </div>`;
    }

    case 'PaperHeader': {
      const categories = (props.categories as string[]) || [];
      return `<header class="paper-header">
        <h1>${renderI18n(props.title)}</h1>
        <div class="meta">
          <span><strong>arXiv:</strong> ${escapeHtml(String(props.arxivId))}${props.version ? ` (${escapeHtml(String(props.version))})` : ''}</span>
          <span><strong>Date:</strong> ${escapeHtml(String(props.date))}</span>
        </div>
        ${categories.length > 0 ? `<div class="categories">${categories.map(c => `<span class="category">${escapeHtml(c)}</span>`).join('')}</div>` : ''}
      </header>`;
    }

    case 'AuthorList': {
      const authors = (props.authors as Array<{ name: string; affiliation?: string }>) || [];
      const maxVisible = props.maxVisible as number | undefined;
      const visible = maxVisible ? authors.slice(0, maxVisible) : authors;
      const hidden = maxVisible ? Math.max(0, authors.length - maxVisible) : 0;
      return `<div class="authors">
        <strong>Authors: </strong>
        ${visible.map((a, i) => `${escapeHtml(a.name)}${a.affiliation ? ` <span class="affiliation">(${escapeHtml(a.affiliation)})</span>` : ''}${i < visible.length - 1 ? ', ' : ''}`).join('')}
        ${hidden > 0 ? ` <span class="affiliation">+${hidden} more</span>` : ''}
      </div>`;
    }

    case 'Section': {
      const icon = props.icon ? iconMap[props.icon as string] || '' : '';
      return `<section class="section">
        <h2>${icon ? `<span>${icon}</span>` : ''}${renderI18n(props.title)}</h2>
        ${childrenHtml}
      </section>`;
    }

    case 'Abstract': {
      if (isI18n(props.text)) {
        let enText = escapeHtml(props.text.en);
        let zhText = escapeHtml(props.text.zh);
        const highlights = (props.highlights as string[]) || [];
        highlights.forEach(h => {
          const escaped = h.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
          enText = enText.replace(new RegExp(`(${escaped})`, 'gi'), '<mark>$1</mark>');
          zhText = zhText.replace(new RegExp(`(${escaped})`, 'gi'), '<mark>$1</mark>');
        });
        return `<p class="abstract"><span class="i18n-en">${enText}</span><span class="i18n-zh">${zhText}</span></p>`;
      }
      let text = escapeHtml(String(props.text));
      const highlights = (props.highlights as string[]) || [];
      highlights.forEach(h => {
        const escaped = h.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
        text = text.replace(new RegExp(`(${escaped})`, 'gi'), '<mark>$1</mark>');
      });
      return `<p class="abstract">${text}</p>`;
    }

    case 'ContributionList': {
      const items = (props.items as Array<{ title: I18nValue; description?: I18nValue; badge?: I18nValue }>) || [];
      return `<ol class="contribution-list">
        ${items.map(item => `<li>
          ${item.badge ? `<span class="badge">${renderI18n(item.badge)}</span>` : ''}
          <strong>${renderI18n(item.title)}</strong>
          ${item.description ? `<span class="description"> — ${renderI18n(item.description)}</span>` : ''}
        </li>`).join('')}
      </ol>`;
    }

    case 'MethodOverview': {
      const steps = (props.steps as Array<{ step: number; title: I18nValue; description: I18nValue }>) || [];
      return `<div class="method-overview">
        ${steps.map(s => `<div class="method-step">
          <div class="number">${s.step}</div>
          <div class="content">
            <strong>${renderI18n(s.title)}</strong>
            <p>${renderI18n(s.description)}</p>
          </div>
        </div>`).join('')}
      </div>`;
    }

    case 'Highlight': {
      const highlightType = (props.type as string) || 'quote';
      return `<blockquote class="highlight ${highlightType}">
        <p>${renderI18n(props.text)}</p>
        ${props.source ? `<footer class="source">— ${renderI18n(props.source)}</footer>` : ''}
      </blockquote>`;
    }

    case 'MetricsGrid': {
      const metrics = (props.metrics as Array<{ label: I18nValue; value: string | number; trend?: string; suffix?: string; icon?: string }>) || [];
      const cols = (props.cols as number) || 4;
      return `<div class="metrics-grid" style="grid-template-columns: repeat(${cols}, 1fr)">
        ${metrics.map(m => `<div class="metric">
          ${m.icon ? `<span>${iconMap[m.icon] || ''}</span>` : ''}
          <div class="value">
            ${m.value}${m.suffix ? `<span class="suffix">${escapeHtml(m.suffix)}</span>` : ''}
            ${m.trend === 'up' ? '<span class="trend-up"> ↑</span>' : ''}
            ${m.trend === 'down' ? '<span class="trend-down"> ↓</span>' : ''}
          </div>
          <div class="label">${renderI18n(m.label)}</div>
        </div>`).join('')}
      </div>`;
    }

    case 'LinkGroup': {
      const links = (props.links as Array<{ href: string; label: I18nValue; icon?: string; external?: boolean }>) || [];
      return `<div class="link-group">
        ${links.map(l => `<a href="${escapeHtml(l.href)}" class="link-button" ${l.external !== false ? 'target="_blank" rel="noopener"' : ''}>
          ${l.icon ? `<span>${iconMap[l.icon] || ''}</span>` : ''}${renderI18n(l.label)}
        </a>`).join('')}
      </div>`;
    }

    case 'BrandFooter':
      return `<footer class="brand-footer">
        ${props.disclaimer ? `<p>${renderI18n(props.disclaimer)}</p>` : ''}
        <p><strong>${renderI18n(props.attribution || 'Powered by Actionbook')}</strong> | Generated: ${escapeHtml(String(props.timestamp))}</p>
      </footer>`;

    case 'Grid': {
      const cols = props.cols as number || 1;
      return `<div class="grid" style="grid-template-columns: repeat(${cols}, 1fr)">
        ${childrenHtml}
      </div>`;
    }

    case 'Card': {
      const padding = (props.padding as string) || 'md';
      const shadow = props.shadow !== false;
      return `<div class="card padding-${padding}${shadow ? ' shadow' : ''}">
        ${childrenHtml}
      </div>`;
    }

    case 'Image': {
      const width = props.width ? ` style="width: ${escapeHtml(String(props.width))}"` : '';
      return `<div class="image">
        <img src="${escapeHtml(String(props.src))}" alt="${escapeHtml(resolveI18n(props.alt || '', 'en'))}" referrerpolicy="no-referrer"${width}>
        ${props.caption ? `<div class="caption">${renderI18n(props.caption)}</div>` : ''}
      </div>`;
    }

    case 'Figure': {
      const images = (props.images as Array<{ src: string; alt?: I18nValue; caption?: I18nValue; width?: string }>) || [];
      return `<figure class="figure">
        <div class="images">
          ${images.map(img => {
            const width = img.width ? ` style="width: ${escapeHtml(img.width)}"` : '';
            return `<img src="${escapeHtml(img.src)}" alt="${escapeHtml(resolveI18n(img.alt || '', 'en'))}" referrerpolicy="no-referrer"${width}>`;
          }).join('')}
        </div>
        ${props.label || props.caption ? `<figcaption>
          ${props.label ? `<span class="label">${renderI18n(props.label)}:</span> ` : ''}
          ${props.caption ? renderI18n(props.caption) : ''}
        </figcaption>` : ''}
      </figure>`;
    }

    case 'Formula': {
      const isBlock = props.block === true;
      // Store raw LaTeX in data attribute for KaTeX to render
      const latexStr = String(props.latex);
      const escapedAttr = latexStr.replace(/"/g, '&quot;').replace(/&/g, '&amp;');
      return `<div class="formula${isBlock ? ' block' : ''}" data-latex="${escapedAttr}">
        ${props.label ? `<span class="label">(${escapeHtml(String(props.label))})</span>` : ''}
        <span class="formula-content"><code>${escapeHtml(latexStr)}</code></span>
      </div>`;
    }

    case 'Prose': {
      // Simple markdown-like rendering
      function renderMarkdown(raw: string): string {
        let content = escapeHtml(raw);
        content = content.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
        content = content.replace(/\*([^*]+)\*/g, '<em>$1</em>');
        content = content.replace(/`([^`]+)`/g, '<code>$1</code>');
        content = content.replace(/^### (.+)$/gm, '<h4>$1</h4>');
        content = content.replace(/^## (.+)$/gm, '<h3>$1</h3>');
        content = content.replace(/\n\n/g, '</p><p>');
        content = content.replace(/^- (.+)$/gm, '<li>$1</li>');
        content = content.replace(/(<li>.*<\/li>)+/gs, '<ul>$&</ul>');
        return content;
      }
      if (isI18n(props.content)) {
        return `<div class="prose"><p><span class="i18n-en">${renderMarkdown(props.content.en)}</span><span class="i18n-zh">${renderMarkdown(props.content.zh)}</span></p></div>`;
      }
      return `<div class="prose"><p>${renderMarkdown(String(props.content))}</p></div>`;
    }

    case 'Callout': {
      const calloutType = (props.type as string) || 'info';
      return `<div class="callout ${calloutType}">
        ${props.title ? `<div class="callout-title">${renderI18n(props.title)}</div>` : ''}
        <div>${renderI18n(props.content)}</div>
      </div>`;
    }

    case 'DefinitionList': {
      const items = (props.items as Array<{ term: I18nValue; definition: I18nValue }>) || [];
      return `<div class="definition-list">
        <dl>
          ${items.map(item => `
            <div>
              <dt>${renderI18n(item.term)}</dt>
              <dd>${renderI18n(item.definition)}</dd>
            </div>
          `).join('')}
        </dl>
      </div>`;
    }

    case 'Theorem': {
      const theoremType = (props.type as string) || 'theorem';
      const typeLabels: Record<string, string> = {
        theorem: 'Theorem', lemma: 'Lemma', proposition: 'Proposition',
        corollary: 'Corollary', definition: 'Definition', remark: 'Remark'
      };
      const label = typeLabels[theoremType] || 'Theorem';
      return `<div class="theorem ${theoremType}">
        <div class="theorem-header">
          ${label}${props.number ? ` ${escapeHtml(String(props.number))}` : ''}${props.title ? ` (${renderI18n(props.title)})` : ''}
        </div>
        <div class="theorem-content">${renderI18n(props.content)}</div>
      </div>`;
    }

    case 'Algorithm': {
      const steps = (props.steps as Array<{ line: number; code: string; indent?: number }>) || [];
      return `<div class="algorithm">
        <div class="algorithm-title">Algorithm: ${renderI18n(props.title)}</div>
        <div class="algorithm-body">
          ${steps.map(s => `
            <div class="line">
              <span class="line-number">${s.line}</span>
              <span class="line-code${s.indent ? ` indent-${s.indent}` : ''}">${escapeHtml(s.code)}</span>
            </div>
          `).join('')}
        </div>
        ${props.caption ? `<div class="algorithm-caption">${renderI18n(props.caption)}</div>` : ''}
      </div>`;
    }

    case 'ResultsTable': {
      const columns = (props.columns as Array<{ key: string; label: I18nValue; highlight?: boolean }>) || [];
      const rows = (props.rows as Array<Record<string, unknown>>) || [];
      const highlights = (props.highlights as Array<{ row: number; col: string }>) || [];
      const isHighlighted = (row: number, col: string) =>
        highlights.some(h => h.row === row && h.col === col);

      return `<div class="results-table">
        ${props.caption ? `<caption>${renderI18n(props.caption)}</caption>` : ''}
        <table>
          <thead>
            <tr>
              ${columns.map(c => `<th${c.highlight ? ' class="highlight"' : ''}>${renderI18n(c.label)}</th>`).join('')}
            </tr>
          </thead>
          <tbody>
            ${rows.map((row, rowIdx) => `
              <tr>
                ${columns.map(c => `<td${isHighlighted(rowIdx, c.key) ? ' class="highlight"' : ''}>${renderI18n(row[c.key])}</td>`).join('')}
              </tr>
            `).join('')}
          </tbody>
        </table>
      </div>`;
    }

    case 'CodeBlock': {
      const lines = String(props.code).split('\n');
      const showLineNumbers = props.showLineNumbers === true;
      return `<div class="code-block">
        ${props.title ? `<div class="code-title">${renderI18n(props.title)} (${escapeHtml(String(props.language || 'text'))})</div>` : ''}
        <pre>${showLineNumbers ? `<span class="line-numbers">${lines.map((_, i) => i + 1).join('\n')}</span>` : ''}${escapeHtml(String(props.code))}</pre>
      </div>`;
    }

    case 'Table': {
      const columns = (props.columns as Array<{ key: string; label: I18nValue; align?: string; width?: string }>) || [];
      const rows = (props.rows as Array<Record<string, unknown>>) || [];
      const striped = props.striped !== false;
      const compact = props.compact === true;

      return `<div class="table-wrapper${striped ? ' striped' : ''}${compact ? ' compact' : ''}">
        ${props.caption ? `<caption>${renderI18n(props.caption)}</caption>` : ''}
        <table>
          <thead>
            <tr>
              ${columns.map(c => {
                const align = c.align ? ` style="text-align: ${c.align}"` : '';
                return `<th${align}>${renderI18n(c.label)}</th>`;
              }).join('')}
            </tr>
          </thead>
          <tbody>
            ${rows.map(row => `
              <tr>
                ${columns.map(c => {
                  const align = c.align ? ` style="text-align: ${c.align}"` : '';
                  return `<td${align}>${renderI18n(row[c.key])}</td>`;
                }).join('')}
              </tr>
            `).join('')}
          </tbody>
        </table>
      </div>`;
    }

    case 'TagList': {
      const tags = (props.tags as Array<{ label: I18nValue; color?: string; href?: string }>) || [];
      return `<div class="categories">
        ${tags.map(t => {
          const style = t.color ? ` style="background: ${escapeHtml(t.color)}"` : '';
          if (t.href) {
            return `<a href="${escapeHtml(t.href)}" class="category"${style}>${renderI18n(t.label)}</a>`;
          }
          return `<span class="category"${style}>${renderI18n(t.label)}</span>`;
        }).join('')}
      </div>`;
    }

    case 'KeyPoint': {
      const icon = props.icon ? iconMap[props.icon as string] || '💡' : '💡';
      return `<div class="highlight ${(props.variant as string) || 'quote'}">
        <p><strong>${icon} ${renderI18n(props.title)}</strong></p>
        <p>${renderI18n(props.description)}</p>
      </div>`;
    }

    default:
      return childrenHtml;
  }
}

// ============================================
// CLI
// ============================================

async function main() {
  const args = process.argv.slice(2);

  if (args.length === 0 || args[0] === '--help' || args[0] === '-h') {
    console.log(`
json-ui - Render JSON report to HTML

Usage:
  json-ui render <input.json>              Render and open in browser
  json-ui render <input.json> -o out.html  Render to file
  json-ui render <input.json> --no-open    Don't open browser
  json-ui render -                         Read from stdin

Options:
  -o, --output <file>   Output HTML file path
  --no-open             Don't open browser after rendering
  -h, --help            Show this help

Examples:
  json-ui render report.json
  json-ui render report.json -o paper-report.html
  cat report.json | json-ui render - --no-open
`);
    process.exit(0);
  }

  const command = args[0];
  if (command !== 'render') {
    console.error(`Unknown command: ${command}`);
    process.exit(1);
  }

  const inputFile = args[1];
  if (!inputFile) {
    console.error('Error: Input file required');
    process.exit(1);
  }

  // Parse options
  let outputFile: string | undefined;
  let openBrowser = true;

  for (let i = 2; i < args.length; i++) {
    if (args[i] === '-o' || args[i] === '--output') {
      outputFile = args[++i];
    } else if (args[i] === '--no-open') {
      openBrowser = false;
    }
  }

  // Read input
  let jsonContent: string;
  if (inputFile === '-') {
    // Read from stdin
    const chunks: Buffer[] = [];
    for await (const chunk of process.stdin) {
      chunks.push(chunk);
    }
    jsonContent = Buffer.concat(chunks).toString('utf-8');
  } else {
    jsonContent = await fs.readFile(inputFile, 'utf-8');
  }

  // Parse JSON
  let json: ReportJSON;
  try {
    json = JSON.parse(jsonContent);
  } catch {
    console.error('Error: Invalid JSON');
    process.exit(1);
  }

  // Generate HTML
  const html = generateHTML(json);

  // Determine output path
  if (!outputFile) {
    // Use temp file
    const tmpDir = os.tmpdir();
    const timestamp = Date.now();
    outputFile = path.join(tmpDir, `json-ui-report-${timestamp}.html`);
  }

  // Write HTML
  await fs.writeFile(outputFile, html, 'utf-8');
  console.log(`✅ HTML generated: ${outputFile}`);

  // Open in browser
  if (openBrowser) {
    const platform = os.platform();
    try {
      if (platform === 'darwin') {
        execSync(`open "${outputFile}"`);
      } else if (platform === 'win32') {
        execSync(`start "" "${outputFile}"`);
      } else {
        execSync(`xdg-open "${outputFile}"`);
      }
      console.log('🌐 Opened in browser');
    } catch {
      console.log(`Open manually: file://${outputFile}`);
    }
  }
}

main().catch((err) => {
  console.error('Error:', err.message);
  process.exit(1);
});
