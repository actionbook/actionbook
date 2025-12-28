import { NextRequest, NextResponse } from 'next/server';
import { search } from '@/lib/search';

/**
 * Action search result - based on chunks table
 */
interface ActionSearchResult {
  action_id: number;  // chunk_id as action_id
  content: string;
  score: number;
  createdAt: string;
}

interface ActionSearchResponse {
  success: boolean;
  query: string;
  results?: ActionSearchResult[];
  count?: number;
  total?: number;
  hasMore?: boolean;
  error?: string;
}

/**
 * GET /api/actions/search?q=query&type=hybrid&limit=5
 *
 * Search actions using vector/fulltext/hybrid search on chunks table
 * Returns chunk_id as action_id along with content
 */
export async function GET(request: NextRequest): Promise<NextResponse<ActionSearchResponse>> {
  try {
    const { searchParams } = new URL(request.url);
    const query = searchParams.get('q');

    if (!query) {
      return NextResponse.json(
        {
          success: false,
          query: '',
          results: [],
          count: 0,
          total: 0,
          hasMore: false,
          error: 'q parameter is required'
        },
        { status: 400 }
      );
    }

    // Parse and validate parameters
    const limit = Math.min(Math.max(parseInt(searchParams.get('limit') || '5', 10), 1), 100);
    const type = searchParams.get('type') as 'vector' | 'fulltext' | 'hybrid' | null;
    const minScore = parseFloat(searchParams.get('minScore') || '0');

    // Parse sourceIds if provided (comma-separated)
    const sourceIdsParam = searchParams.get('sourceIds');
    const sourceIds = sourceIdsParam
      ? sourceIdsParam.split(',').map(id => parseInt(id.trim(), 10)).filter(id => !isNaN(id))
      : undefined;

    // Use existing search function from lib/search.ts
    const searchResults = await search(query, {
      searchType: type || 'hybrid',
      limit,
      sourceIds,
      minScore,
    });

    // Map to action search results
    const results: ActionSearchResult[] = searchResults.map((result) => ({
      action_id: result.chunkId,
      content: result.content,
      score: result.score,
      createdAt: result.createdAt.toISOString(),
    }));

    return NextResponse.json({
      success: true,
      query,
      results,
      count: results.length,
      total: results.length,
      hasMore: false,
    });
  } catch (error) {
    console.error('Search API error:', error);
    return NextResponse.json(
      {
        success: false,
        query: '',
        results: [],
        count: 0,
        total: 0,
        hasMore: false,
        error: error instanceof Error ? error.message : 'Internal server error',
      },
      { status: 500 }
    );
  }
}
