import fs from 'fs/promises';
import path from 'path';
import { ReportCard } from '@/components/ReportCard';
import type { ReportMeta } from '@/lib/types';

const categories = [
  { key: 'ai', labelEn: 'AI', labelZh: 'AI' },
  { key: 'paper', labelEn: 'Papers', labelZh: '论文' },
  { key: 'rust', labelEn: 'Rust', labelZh: 'Rust' },
] as const;

async function getReports(): Promise<ReportMeta[]> {
  const indexPath = path.join(process.cwd(), 'public', 'reports', 'index.json');
  const raw = await fs.readFile(indexPath, 'utf-8');
  return JSON.parse(raw) as ReportMeta[];
}

export default async function HomePage() {
  const reports = await getReports();

  const grouped: Record<string, ReportMeta[]> = {};
  for (const r of reports) {
    const key = r.category || 'ai';
    (grouped[key] ??= []).push(r);
  }

  return (
    <div className="home-page">
      <div className="home-header">
        <h1>
          <span className="i18n-en">Reports</span>
          <span className="i18n-zh">报告列表</span>
        </h1>
        <p className="home-subtitle">
          <span className="i18n-en">AI-generated reports rendered with json-ui components</span>
          <span className="i18n-zh">使用 json-ui 组件渲染的 AI 生成报告</span>
        </p>
      </div>
      {categories.map((cat) => {
        const items = grouped[cat.key];
        if (!items || items.length === 0) return null;
        return (
          <section key={cat.key} className="category-section">
            <h2 className="category-title">
              <span className="i18n-en">{cat.labelEn}</span>
              <span className="i18n-zh">{cat.labelZh}</span>
            </h2>
            <div className="report-grid">
              {items.map((report) => (
                <ReportCard key={report.slug} report={report} />
              ))}
            </div>
          </section>
        );
      })}
    </div>
  );
}
