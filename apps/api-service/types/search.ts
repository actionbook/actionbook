/**
 * Search result item
 */
export interface SearchResult {
  documentId: number;
  chunkId: number;
  title: string;
  content: string;
  url: string;
  score: number;
  headingHierarchy: string[];
  breadcrumb: BreadcrumbItem[];
  createdAt: Date;
}

export interface BreadcrumbItem {
  url: string;
  title: string;
}

/**
 * Search request body (POST)
 */
export interface SearchRequestBody {
  query: string;
  type?: 'vector' | 'fulltext' | 'hybrid';
  limit?: number;
  sourceId?: number;
  minScore?: number;
  context?: boolean;
  maxTokens?: number;
}

/**
 * Search response
 */
export interface SearchResponse {
  success: boolean;
  query: string;
  results?: SearchResult[];
  context?: string;
  count?: number;
  total?: number;
  hasMore?: boolean;
  error?: string;
}

export type SearchType = 'vector' | 'fulltext' | 'hybrid';
