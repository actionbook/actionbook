/**
 * json-ui React Components
 */

import React from 'react';
import type {
  ReportProps,
  SectionProps,
  PaperHeaderProps,
  AuthorListProps,
  AbstractProps,
  ContributionListProps,
  MethodOverviewProps,
  MetricProps,
  MetricsGridProps,
  LinkGroupProps,
  BrandHeaderProps,
  BrandFooterProps,
  HighlightSchema,
  Icon,
  I18nStringType,
} from '../catalog';
import { z } from 'zod';

// ============================================
// Utility Components
// ============================================

const iconMap: Record<Icon, string> = {
  paper: '📄',
  user: '👤',
  calendar: '📅',
  tag: '🏷️',
  link: '🔗',
  code: '💻',
  chart: '📊',
  bulb: '💡',
  check: '✅',
  star: '⭐',
  warning: '⚠️',
  info: 'ℹ️',
  github: '🐙',
  arxiv: '📚',
  pdf: '📕',
  copy: '📋',
  expand: '➕',
  collapse: '➖',
};

const IconComponent: React.FC<{ icon: Icon; className?: string }> = ({ icon, className }) => (
  <span className={className} role="img" aria-label={icon}>
    {iconMap[icon]}
  </span>
);

/** Resolve I18nString to a plain string (defaults to 'en') */
function resolveI18nStr(value: I18nStringType | undefined, lang: 'en' | 'zh' = 'en'): string {
  if (value == null) return '';
  if (typeof value === 'string') return value;
  return value[lang];
}

/** Render I18nString as React node with dual spans for client-side switching */
const I18nText: React.FC<{ value: I18nStringType | undefined }> = ({ value }) => {
  if (value == null) return null;
  if (typeof value === 'string') return <>{value}</>;
  return (
    <>
      <span className="i18n-en">{value.en}</span>
      <span className="i18n-zh">{value.zh}</span>
    </>
  );
};

// ============================================
// Layout Components
// ============================================

export const Report: React.FC<ReportProps & { children?: React.ReactNode }> = ({
  title,
  theme = 'auto',
  children,
}) => (
  <article
    className="json-ui-report"
    data-theme={theme}
    style={{
      fontFamily: 'system-ui, -apple-system, sans-serif',
      maxWidth: '800px',
      margin: '0 auto',
      padding: '2rem',
      lineHeight: 1.6,
    }}
  >
    {title && <h1 style={{ marginBottom: '2rem' }}><I18nText value={title} /></h1>}
    {children}
  </article>
);

export const Section: React.FC<SectionProps & { children?: React.ReactNode }> = ({
  title,
  icon,
  collapsible = false,
  defaultExpanded = true,
  children,
}) => {
  const [expanded, setExpanded] = React.useState(defaultExpanded);

  return (
    <section style={{ marginBottom: '1.5rem' }}>
      <h2
        onClick={collapsible ? () => setExpanded(!expanded) : undefined}
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: '0.5rem',
          cursor: collapsible ? 'pointer' : 'default',
          borderBottom: '2px solid #e5e7eb',
          paddingBottom: '0.5rem',
          marginBottom: '1rem',
        }}
      >
        {icon && <IconComponent icon={icon} />}
        <I18nText value={title} />
        {collapsible && (
          <IconComponent icon={expanded ? 'collapse' : 'expand'} />
        )}
      </h2>
      {(!collapsible || expanded) && children}
    </section>
  );
};

// ============================================
// Paper Info Components
// ============================================

export const PaperHeader: React.FC<PaperHeaderProps> = ({
  title,
  arxivId,
  date,
  categories = [],
  version,
}) => (
  <header style={{ marginBottom: '1.5rem' }}>
    <h1 style={{ fontSize: '1.75rem', marginBottom: '0.5rem' }}><I18nText value={title} /></h1>
    <div style={{ display: 'flex', gap: '1rem', color: '#6b7280', fontSize: '0.875rem' }}>
      <span>
        <strong>arXiv:</strong> {arxivId}
        {version && <span style={{ color: '#9ca3af' }}> ({version})</span>}
      </span>
      <span>
        <strong>Date:</strong> {date}
      </span>
    </div>
    {categories.length > 0 && (
      <div style={{ marginTop: '0.5rem', display: 'flex', gap: '0.5rem' }}>
        {categories.map((cat) => (
          <span
            key={cat}
            style={{
              background: '#e5e7eb',
              padding: '0.125rem 0.5rem',
              borderRadius: '9999px',
              fontSize: '0.75rem',
            }}
          >
            {cat}
          </span>
        ))}
      </div>
    )}
  </header>
);

export const AuthorList: React.FC<AuthorListProps> = ({
  authors,
  layout = 'inline',
  showAffiliations = true,
  maxVisible,
}) => {
  const visibleAuthors = maxVisible ? authors.slice(0, maxVisible) : authors;
  const hiddenCount = maxVisible ? Math.max(0, authors.length - maxVisible) : 0;

  if (layout === 'inline') {
    return (
      <div style={{ marginBottom: '1rem', color: '#374151' }}>
        <strong>Authors: </strong>
        {visibleAuthors.map((a, i) => (
          <span key={a.name}>
            {a.name}
            {showAffiliations && a.affiliation && (
              <span style={{ color: '#6b7280' }}> ({a.affiliation})</span>
            )}
            {i < visibleAuthors.length - 1 && ', '}
          </span>
        ))}
        {hiddenCount > 0 && (
          <span style={{ color: '#6b7280' }}> +{hiddenCount} more</span>
        )}
      </div>
    );
  }

  return (
    <ul style={{ listStyle: 'none', padding: 0, marginBottom: '1rem' }}>
      {visibleAuthors.map((a) => (
        <li key={a.name} style={{ marginBottom: '0.25rem' }}>
          <strong>{a.name}</strong>
          {showAffiliations && a.affiliation && (
            <span style={{ color: '#6b7280' }}> — {a.affiliation}</span>
          )}
        </li>
      ))}
      {hiddenCount > 0 && (
        <li style={{ color: '#6b7280' }}>+{hiddenCount} more authors</li>
      )}
    </ul>
  );
};

export const Abstract: React.FC<AbstractProps> = ({
  text,
  highlights = [],
  maxLength,
}) => {
  function processText(raw: string): string {
    let displayText = raw;
    if (maxLength && raw.length > maxLength) {
      displayText = raw.slice(0, maxLength) + '...';
    }
    let highlightedText = displayText;
    highlights.forEach((h) => {
      const escaped = h.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
      highlightedText = highlightedText.replace(
        new RegExp(`(${escaped})`, 'gi'),
        '<mark style="background:#fef08a;padding:0 2px">$1</mark>'
      );
    });
    return highlightedText;
  }

  if (typeof text === 'object' && 'en' in text && 'zh' in text) {
    return (
      <p style={{ color: '#374151', textAlign: 'justify' }}>
        <span className="i18n-en" dangerouslySetInnerHTML={{ __html: processText(text.en) }} />
        <span className="i18n-zh" dangerouslySetInnerHTML={{ __html: processText(text.zh) }} />
      </p>
    );
  }

  return (
    <p
      style={{ color: '#374151', textAlign: 'justify' }}
      dangerouslySetInnerHTML={{ __html: processText(String(text)) }}
    />
  );
};

// ============================================
// Content Components
// ============================================

export const ContributionList: React.FC<ContributionListProps> = ({
  items,
  numbered = true,
}) => (
  <ol
    style={{
      listStyleType: numbered ? 'decimal' : 'disc',
      paddingLeft: '1.5rem',
    }}
  >
    {items.map((item, i) => (
      <li key={i} style={{ marginBottom: '0.75rem' }}>
        {item.badge && (
          <span
            style={{
              background: '#3b82f6',
              color: 'white',
              padding: '0.125rem 0.5rem',
              borderRadius: '4px',
              fontSize: '0.75rem',
              marginRight: '0.5rem',
            }}
          >
            <I18nText value={item.badge} />
          </span>
        )}
        <strong><I18nText value={item.title} /></strong>
        {item.description && (
          <span style={{ color: '#6b7280' }}> — <I18nText value={item.description} /></span>
        )}
      </li>
    ))}
  </ol>
);

export const MethodOverview: React.FC<MethodOverviewProps> = ({
  steps,
  showConnectors = true,
}) => (
  <div style={{ display: 'flex', flexDirection: 'column', gap: '1rem' }}>
    {steps.map((step, i) => (
      <div
        key={step.step}
        style={{
          display: 'flex',
          alignItems: 'flex-start',
          gap: '1rem',
        }}
      >
        <div
          style={{
            width: '2rem',
            height: '2rem',
            borderRadius: '50%',
            background: '#3b82f6',
            color: 'white',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            fontWeight: 'bold',
            flexShrink: 0,
          }}
        >
          {step.step}
        </div>
        <div>
          <strong><I18nText value={step.title} /></strong>
          <p style={{ margin: '0.25rem 0 0', color: '#6b7280' }}>
            <I18nText value={step.description} />
          </p>
        </div>
        {showConnectors && i < steps.length - 1 && (
          <div
            style={{
              position: 'absolute',
              left: '1rem',
              top: '2.5rem',
              width: '2px',
              height: '1rem',
              background: '#e5e7eb',
            }}
          />
        )}
      </div>
    ))}
  </div>
);

export const Highlight: React.FC<z.infer<typeof HighlightSchema>> = ({
  text,
  type = 'quote',
  source,
}) => {
  const styles: Record<string, React.CSSProperties> = {
    quote: { borderLeft: '4px solid #3b82f6', background: '#eff6ff' },
    important: { borderLeft: '4px solid #f59e0b', background: '#fffbeb' },
    warning: { borderLeft: '4px solid #ef4444', background: '#fef2f2' },
    code: { borderLeft: '4px solid #10b981', background: '#ecfdf5', fontFamily: 'monospace' },
  };

  return (
    <blockquote
      style={{
        ...styles[type],
        padding: '1rem',
        margin: '1rem 0',
        borderRadius: '0 4px 4px 0',
      }}
    >
      <p style={{ margin: 0 }}><I18nText value={text} /></p>
      {source && (
        <footer style={{ marginTop: '0.5rem', fontSize: '0.875rem', color: '#6b7280' }}>
          — <I18nText value={source} />
        </footer>
      )}
    </blockquote>
  );
};

// ============================================
// Data Display Components
// ============================================

export const Metric: React.FC<MetricProps> = ({
  label,
  value,
  trend,
  icon,
  suffix,
}) => (
  <div
    style={{
      padding: '1rem',
      background: '#f9fafb',
      borderRadius: '8px',
      textAlign: 'center',
    }}
  >
    {icon && <IconComponent icon={icon} />}
    <div style={{ fontSize: '1.5rem', fontWeight: 'bold' }}>
      {value}
      {suffix && <span style={{ fontSize: '1rem', color: '#6b7280' }}>{suffix}</span>}
      {trend === 'up' && <span style={{ color: '#10b981' }}> ↑</span>}
      {trend === 'down' && <span style={{ color: '#ef4444' }}> ↓</span>}
    </div>
    <div style={{ color: '#6b7280', fontSize: '0.875rem' }}><I18nText value={label} /></div>
  </div>
);

export const MetricsGrid: React.FC<MetricsGridProps> = ({ metrics, cols = 4 }) => (
  <div
    style={{
      display: 'grid',
      gridTemplateColumns: `repeat(${cols}, 1fr)`,
      gap: '1rem',
    }}
  >
    {metrics.map((m, i) => (
      <Metric key={i} {...m} />
    ))}
  </div>
);

// ============================================
// Interactive Components
// ============================================

export const LinkGroup: React.FC<LinkGroupProps> = ({
  links,
  layout = 'horizontal',
}) => (
  <div
    style={{
      display: 'flex',
      flexDirection: layout === 'horizontal' ? 'row' : 'column',
      gap: '0.75rem',
      flexWrap: 'wrap',
    }}
  >
    {links.map((link) => (
      <a
        key={link.href}
        href={link.href}
        target={link.external ? '_blank' : undefined}
        rel={link.external ? 'noopener noreferrer' : undefined}
        style={{
          display: 'inline-flex',
          alignItems: 'center',
          gap: '0.5rem',
          padding: '0.5rem 1rem',
          background: '#3b82f6',
          color: 'white',
          borderRadius: '6px',
          textDecoration: 'none',
          fontSize: '0.875rem',
        }}
      >
        {link.icon && <IconComponent icon={link.icon} />}
        <I18nText value={link.label} />
      </a>
    ))}
  </div>
);

// ============================================
// Brand Components
// ============================================

export const BrandHeader: React.FC<BrandHeaderProps> = ({
  badge = '🤖 AI Generated Content',
  poweredBy = 'ActionBook',
  showBadge = true,
}) => (
  <div
    style={{
      background: '#f3f4f6',
      padding: '0.75rem 1rem',
      borderRadius: '8px',
      marginBottom: '1.5rem',
      display: 'flex',
      justifyContent: 'space-between',
      alignItems: 'center',
    }}
  >
    {showBadge && <span><I18nText value={badge} /></span>}
    <span style={{ color: '#6b7280', fontSize: '0.875rem' }}>
      Powered by <strong><I18nText value={poweredBy} /></strong>
    </span>
  </div>
);

export const BrandFooter: React.FC<BrandFooterProps> = ({
  timestamp,
  attribution = 'Powered by ActionBook',
  disclaimer,
}) => (
  <footer
    style={{
      marginTop: '2rem',
      paddingTop: '1rem',
      borderTop: '1px solid #e5e7eb',
      color: '#6b7280',
      fontSize: '0.875rem',
    }}
  >
    {disclaimer && <p style={{ marginBottom: '0.5rem' }}>📝 <I18nText value={disclaimer} /></p>}
    <p>
      <strong><I18nText value={attribution} /></strong> | Generated: {timestamp}
    </p>
  </footer>
);
