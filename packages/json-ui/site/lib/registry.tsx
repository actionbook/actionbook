'use client';

import React from 'react';
import katex from 'katex/dist/katex.mjs';

// ============================================
// i18n Types & Helpers
// ============================================

type I18nString = string | { en: string; zh: string };

function I18nText({ value }: { value: I18nString | undefined }) {
  if (value == null) return null;
  if (typeof value === 'string') return <>{value}</>;
  return (
    <>
      <span className="i18n-en">{value.en}</span>
      <span className="i18n-zh">{value.zh}</span>
    </>
  );
}

// ============================================
// Icon Map
// ============================================

const iconMap: Record<string, string> = {
  paper: '\u{1F4C4}', user: '\u{1F464}', calendar: '\u{1F4C5}',
  tag: '\u{1F3F7}\uFE0F', link: '\u{1F517}', code: '\u{1F4BB}',
  chart: '\u{1F4CA}', bulb: '\u{1F4A1}', check: '\u2705',
  star: '\u2B50', warning: '\u26A0\uFE0F', info: '\u2139\uFE0F',
  github: '\u{1F419}', arxiv: '\u{1F4DA}', pdf: '\u{1F4D5}',
  copy: '\u{1F4CB}', expand: '\u2795', collapse: '\u2796',
};

function Icon({ icon }: { icon: string }) {
  return <span role="img" aria-label={icon}>{iconMap[icon] || ''}</span>;
}

// ============================================
// Component Props Types (from json-ui catalog)
// ============================================

interface RegistryEntry {
  (props: { props: Record<string, any>; children?: React.ReactNode }): React.ReactNode;
}

// ============================================
// Components
// ============================================

function Report({ props: p, children }: { props: any; children?: React.ReactNode }) {
  return (
    <article className="json-ui-report" data-theme={p.theme || 'auto'}>
      {p.title && <h1 className="report-title"><I18nText value={p.title} /></h1>}
      {children}
    </article>
  );
}

function Section({ props: p, children }: { props: any; children?: React.ReactNode }) {
  const [expanded, setExpanded] = React.useState(p.defaultExpanded !== false);
  const collapsible = p.collapsible === true;

  return (
    <section className="section">
      <h2
        className={`section-title ${collapsible ? 'collapsible' : ''}`}
        onClick={collapsible ? () => setExpanded(!expanded) : undefined}
      >
        {p.icon && <Icon icon={p.icon} />}
        <I18nText value={p.title} />
        {collapsible && <span className="collapse-icon">{expanded ? '▲' : '▼'}</span>}
      </h2>
      {(!collapsible || expanded) && <div className="section-content">{children}</div>}
    </section>
  );
}

function Grid({ props: p, children }: { props: any; children?: React.ReactNode }) {
  const cols = typeof p.cols === 'number' ? p.cols : 1;
  return (
    <div className="grid" style={{ gridTemplateColumns: `repeat(${cols}, 1fr)` }}>
      {children}
    </div>
  );
}

function Card({ props: p, children }: { props: any; children?: React.ReactNode }) {
  return <div className={`card variant-${p.variant || 'default'}`}>{children}</div>;
}

function BrandHeader({ props: p }: { props: any }) {
  return (
    <div className="brand-header">
      {p.showBadge !== false && <span className="badge"><I18nText value={p.badge} /></span>}
      <span className="powered-by">
        Powered by <strong><I18nText value={p.poweredBy || 'Actionbook'} /></strong>
      </span>
    </div>
  );
}

function BrandFooter({ props: p }: { props: any }) {
  return (
    <footer className="brand-footer">
      {p.disclaimer && <p className="disclaimer"><I18nText value={p.disclaimer} /></p>}
      <p>
        <strong><I18nText value={p.attribution || 'Powered by Actionbook'} /></strong>
        {' | Generated: '}{p.timestamp}
      </p>
    </footer>
  );
}

function PaperHeader({ props: p }: { props: any }) {
  return (
    <header className="paper-header">
      <h1><I18nText value={p.title} /></h1>
      <div className="paper-meta">
        <span><strong>arXiv:</strong> {p.arxivId}{p.version && <span className="muted"> ({p.version})</span>}</span>
        <span><strong>Date:</strong> {p.date}</span>
      </div>
      {p.categories?.length > 0 && (
        <div className="category-tags">
          {p.categories.map((cat: string) => (
            <span key={cat} className="category-tag">{cat}</span>
          ))}
        </div>
      )}
    </header>
  );
}

function AuthorList({ props: p }: { props: any }) {
  const authors = p.authors || [];
  const maxVisible = p.maxVisible;
  const visible = maxVisible ? authors.slice(0, maxVisible) : authors;
  const hidden = maxVisible ? Math.max(0, authors.length - maxVisible) : 0;

  return (
    <div className="author-list">
      <strong>Authors: </strong>
      {visible.map((a: any, i: number) => (
        <span key={a.name}>
          {a.name}
          {p.showAffiliations !== false && a.affiliation && (
            <span className="muted"> ({a.affiliation})</span>
          )}
          {i < visible.length - 1 && ', '}
        </span>
      ))}
      {hidden > 0 && <span className="muted"> +{hidden} more</span>}
    </div>
  );
}

function Abstract({ props: p }: { props: any }) {
  function processText(raw: string): string {
    let text = p.maxLength && raw.length > p.maxLength ? raw.slice(0, p.maxLength) + '...' : raw;
    (p.highlights || []).forEach((h: string) => {
      const escaped = h.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
      text = text.replace(new RegExp(`(${escaped})`, 'gi'), '<mark>$1</mark>');
    });
    return text;
  }

  if (typeof p.text === 'object' && 'en' in p.text) {
    return (
      <p className="abstract">
        <span className="i18n-en" dangerouslySetInnerHTML={{ __html: processText(p.text.en) }} />
        <span className="i18n-zh" dangerouslySetInnerHTML={{ __html: processText(p.text.zh) }} />
      </p>
    );
  }
  return <p className="abstract" dangerouslySetInnerHTML={{ __html: processText(String(p.text)) }} />;
}

function TagList({ props: p }: { props: any }) {
  return (
    <div className="tag-list">
      {(p.tags || []).map((tag: any, i: number) => (
        <span key={i} className={`tag variant-${p.variant || 'subtle'}`}
          style={tag.color ? { borderColor: tag.color } : undefined}>
          {tag.href ? (
            <a href={tag.href} target="_blank" rel="noopener noreferrer"><I18nText value={tag.label} /></a>
          ) : (
            <I18nText value={tag.label} />
          )}
        </span>
      ))}
    </div>
  );
}

function KeyPoint({ props: p }: { props: any }) {
  return (
    <div className={`key-point variant-${p.variant || 'default'}`}>
      <div className="key-point-icon"><Icon icon={p.icon || 'bulb'} /></div>
      <div>
        <strong><I18nText value={p.title} /></strong>
        <p className="muted"><I18nText value={p.description} /></p>
      </div>
    </div>
  );
}

function ContributionList({ props: p }: { props: any }) {
  const Tag = p.numbered !== false ? 'ol' : 'ul';
  return (
    <Tag className="contribution-list">
      {(p.items || []).map((item: any, i: number) => (
        <li key={i}>
          {item.badge && <span className="badge-inline"><I18nText value={item.badge} /></span>}
          <strong><I18nText value={item.title} /></strong>
          {item.description && <span className="muted"> &mdash; <I18nText value={item.description} /></span>}
        </li>
      ))}
    </Tag>
  );
}

function MethodOverview({ props: p }: { props: any }) {
  return (
    <div className="method-overview">
      {(p.steps || []).map((step: any, i: number) => (
        <div key={step.step} className="method-step">
          <div className="step-number">{step.step}</div>
          <div>
            <strong><I18nText value={step.title} /></strong>
            <p className="muted"><I18nText value={step.description} /></p>
          </div>
        </div>
      ))}
    </div>
  );
}

function Highlight({ props: p }: { props: any }) {
  return (
    <blockquote className={`highlight highlight-${p.type || 'quote'}`}>
      <p><I18nText value={p.text} /></p>
      {p.source && <footer className="muted">&mdash; <I18nText value={p.source} /></footer>}
    </blockquote>
  );
}

function CodeBlock({ props: p }: { props: any }) {
  return (
    <div className="code-block">
      {p.title && <div className="code-title"><I18nText value={p.title} /></div>}
      <pre><code>{p.code}</code></pre>
    </div>
  );
}

function Metric({ props: p }: { props: any }) {
  return (
    <div className="metric">
      {p.icon && <Icon icon={p.icon} />}
      <div className="metric-value">
        {p.value}
        {p.suffix && <span className="metric-suffix">{p.suffix}</span>}
        {p.trend === 'up' && <span className="trend-up"> ↑</span>}
        {p.trend === 'down' && <span className="trend-down"> ↓</span>}
      </div>
      <div className="metric-label"><I18nText value={p.label} /></div>
    </div>
  );
}

function MetricsGrid({ props: p }: { props: any }) {
  const cols = p.cols || 4;
  return (
    <div className="metrics-grid" style={{ gridTemplateColumns: `repeat(${cols}, 1fr)` }}>
      {(p.metrics || []).map((m: any, i: number) => (
        <Metric key={i} props={m} />
      ))}
    </div>
  );
}

function Table({ props: p }: { props: any }) {
  return (
    <div className="table-wrapper">
      {p.caption && <div className="table-caption"><I18nText value={p.caption} /></div>}
      <table className={`data-table ${p.striped !== false ? 'striped' : ''} ${p.compact ? 'compact' : ''}`}>
        <thead>
          <tr>
            {(p.columns || []).map((col: any) => (
              <th key={col.key} style={{ textAlign: col.align || 'left', width: col.width }}>
                <I18nText value={col.label} />
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {(p.rows || []).map((row: any, ri: number) => (
            <tr key={ri}>
              {(p.columns || []).map((col: any) => (
                <td key={col.key} style={{ textAlign: col.align || 'left' }}>
                  {typeof row[col.key] === 'object' ? <I18nText value={row[col.key]} /> : String(row[col.key] ?? '')}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function ResultsTable({ props: p }: { props: any }) {
  const highlights = new Set((p.highlights || []).map((h: any) => `${h.row}-${h.col}`));
  return (
    <div className="table-wrapper">
      {p.caption && <div className="table-caption"><I18nText value={p.caption} /></div>}
      <table className="data-table results-table striped">
        <thead>
          <tr>
            {(p.columns || []).map((col: any) => (
              <th key={col.key} className={col.highlight ? 'col-highlight' : ''}>
                <I18nText value={col.label} />
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {(p.rows || []).map((row: any, ri: number) => (
            <tr key={ri}>
              {(p.columns || []).map((col: any) => (
                <td key={col.key} className={highlights.has(`${ri}-${col.key}`) ? 'cell-highlight' : ''}>
                  {typeof row[col.key] === 'object' ? <I18nText value={row[col.key]} /> : String(row[col.key] ?? '')}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function LinkGroup({ props: p }: { props: any }) {
  return (
    <div className={`link-group layout-${p.layout || 'horizontal'}`}>
      {(p.links || []).map((link: any) => (
        <a key={link.href} href={link.href}
          target={link.external !== false ? '_blank' : undefined}
          rel={link.external !== false ? 'noopener noreferrer' : undefined}
          className={`link-button variant-${link.variant || 'default'}`}>
          {link.icon && <Icon icon={link.icon} />}
          <I18nText value={link.label} />
        </a>
      ))}
    </div>
  );
}

function LinkButton({ props: p }: { props: any }) {
  return (
    <a href={p.href}
      target={p.external !== false ? '_blank' : undefined}
      rel={p.external !== false ? 'noopener noreferrer' : undefined}
      className={`link-button variant-${p.variant || 'default'}`}>
      {p.icon && <Icon icon={p.icon} />}
      <I18nText value={p.label} />
    </a>
  );
}

function Prose({ props: p }: { props: any }) {
  function renderMarkdown(text: string): string {
    return text
      .replace(/\*\*(.*?)\*\*/g, '<strong>$1</strong>')
      .replace(/\*(.*?)\*/g, '<em>$1</em>')
      .replace(/`(.*?)`/g, '<code>$1</code>')
      .replace(/\n\n/g, '</p><p>')
      .replace(/\n- /g, '</p><ul><li>')
      .replace(/\n/g, '<br/>');
  }

  if (typeof p.content === 'object' && 'en' in p.content) {
    return (
      <div className="prose">
        <div className="i18n-en" dangerouslySetInnerHTML={{ __html: `<p>${renderMarkdown(p.content.en)}</p>` }} />
        <div className="i18n-zh" dangerouslySetInnerHTML={{ __html: `<p>${renderMarkdown(p.content.zh)}</p>` }} />
      </div>
    );
  }
  return <div className="prose" dangerouslySetInnerHTML={{ __html: `<p>${renderMarkdown(String(p.content))}</p>` }} />;
}

function Callout({ props: p }: { props: any }) {
  return (
    <div className={`callout callout-${p.type || 'info'}`}>
      {p.title && <div className="callout-title"><I18nText value={p.title} /></div>}
      <div><I18nText value={p.content} /></div>
    </div>
  );
}

function DefinitionList({ props: p }: { props: any }) {
  return (
    <dl className="definition-list">
      {(p.items || []).map((item: any, i: number) => (
        <React.Fragment key={i}>
          <dt><I18nText value={item.term} /></dt>
          <dd><I18nText value={item.definition} /></dd>
        </React.Fragment>
      ))}
    </dl>
  );
}

function Theorem({ props: p }: { props: any }) {
  const typeLabel = (p.type || 'theorem').charAt(0).toUpperCase() + (p.type || 'theorem').slice(1);
  return (
    <div className={`theorem theorem-${p.type || 'theorem'}`}>
      <div className="theorem-header">
        <strong>{typeLabel}{p.number ? ` ${p.number}` : ''}</strong>
        {p.title && <span> (<I18nText value={p.title} />)</span>}
      </div>
      <div className="theorem-content"><I18nText value={p.content} /></div>
    </div>
  );
}

function Algorithm({ props: p }: { props: any }) {
  return (
    <div className="algorithm">
      <div className="algorithm-title"><I18nText value={p.title} /></div>
      <div className="algorithm-body">
        {(p.steps || []).map((step: any) => (
          <div key={step.line} className="algorithm-line" style={{ paddingLeft: `${(step.indent || 0) * 1.5}rem` }}>
            <span className="line-number">{step.line}</span>
            <span className="line-code">{step.code}</span>
          </div>
        ))}
      </div>
      {p.caption && <div className="algorithm-caption muted"><I18nText value={p.caption} /></div>}
    </div>
  );
}

function Formula({ props: p }: { props: any }) {
  const html = React.useMemo(() => {
    try {
      return katex.renderToString(p.latex, {
        displayMode: p.block !== false,
        throwOnError: false,
      });
    } catch {
      return p.latex;
    }
  }, [p.latex, p.block]);

  return (
    <div className={`formula ${p.block ? 'formula-block' : 'formula-inline'}`}>
      <span className="formula-katex" dangerouslySetInnerHTML={{ __html: html }} />
      {p.label && <span className="formula-label">({p.label})</span>}
    </div>
  );
}

function Figure({ props: p }: { props: any }) {
  return (
    <figure className="figure">
      {(p.images || []).map((img: any, i: number) => (
        <img key={i} src={img.src} alt={typeof img.alt === 'string' ? img.alt : img.alt?.en || ''}
          style={{ maxWidth: img.width || '100%', height: 'auto' }}
          onError={(e) => { (e.target as HTMLImageElement).style.display = 'none'; }} />
      ))}
      {(p.label || p.caption) && (
        <figcaption>
          {p.label && <strong><I18nText value={p.label} />: </strong>}
          {p.caption && <I18nText value={p.caption} />}
        </figcaption>
      )}
    </figure>
  );
}

function ImageComponent({ props: p }: { props: any }) {
  return (
    <figure className="image-component">
      <img src={p.src} alt={typeof p.alt === 'string' ? p.alt : p.alt?.en || ''}
        style={{ maxWidth: p.width || '100%', height: 'auto' }}
        onError={(e) => { (e.target as HTMLImageElement).style.display = 'none'; }} />
      {p.caption && <figcaption><I18nText value={p.caption} /></figcaption>}
    </figure>
  );
}

// ============================================
// Registry
// ============================================

export const registry: Record<string, RegistryEntry> = {
  Report,
  Section,
  Grid,
  Card,
  BrandHeader,
  BrandFooter,
  PaperHeader,
  AuthorList,
  Abstract,
  TagList,
  KeyPoint,
  ContributionList,
  MethodOverview,
  Highlight,
  CodeBlock,
  Metric,
  MetricsGrid,
  Table,
  ResultsTable,
  LinkGroup,
  LinkButton,
  Prose,
  Callout,
  DefinitionList,
  Theorem,
  Algorithm,
  Formula,
  Figure,
  Image: ImageComponent,
};
