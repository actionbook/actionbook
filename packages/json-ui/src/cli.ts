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
          fallback.textContent = '📷 ' + (img.alt || 'Image unavailable');
          img.parentNode.insertBefore(fallback, img.nextSibling);
        });
      });
    });
  </script>
</head>
<body>
  <div class="lang-switcher">
    <button data-lang="en" class="active">EN</button>
    <button data-lang="zh">中文</button>
  </div>
  <article class="report">
    ${renderNode(json)}
  </article>
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
  paper: '📄', user: '👤', calendar: '📅', tag: '🏷️', link: '🔗', code: '💻',
  chart: '📊', bulb: '💡', check: '✅', star: '⭐', warning: '⚠️', info: 'ℹ️',
  github: '🐙', arxiv: '📚', pdf: '📕', copy: '📋', expand: '➕', collapse: '➖',
};

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function renderNode(node: ReportJSON): string {
  const { type, props = {}, children = [] } = node;
  const childrenHtml = children.map(renderNode).join('\n');

  switch (type) {
    case 'Report':
      return childrenHtml;

    case 'BrandHeader':
      return `<div class="brand-header">
        <span>${renderI18n(props.badge || '🤖 AI Generated Content')}</span>
        <span class="powered-by">Powered by <strong>${renderI18n(props.poweredBy || 'ActionBook')}</strong></span>
      </div>`;

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
        ${props.disclaimer ? `<p>📝 ${renderI18n(props.disclaimer)}</p>` : ''}
        <p><strong>${renderI18n(props.attribution || 'Powered by ActionBook')}</strong> | Generated: ${escapeHtml(String(props.timestamp))}</p>
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
