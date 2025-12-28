/**
 * Prompt Builder Unit Tests
 */

import { describe, it, expect } from 'vitest';
import {
  buildTaskDrivenPrompt,
  buildExploratoryPrompt,
  buildPrompt,
} from '../../../src/task-worker/utils/prompt-builder';
import type { ChunkData } from '../../../src/task-worker/types';

describe('Prompt Builder', () => {
  const mockChunkTaskDriven: ChunkData = {
    id: 'chunk-001',
    source_id: 'source-airbnb',
    document_url: 'https://www.airbnb.com/',
    document_title: 'Airbnb Homepage',
    source_domain: 'airbnb.com',
    chunk_content: `
## Task: Search for hotels
**Steps:**
1. Click search box
2. Type "Tokyo"
3. Select dates
4. Click search button
    `,
    chunk_index: 0,
    created_at: new Date(),
  };

  const mockChunkExploratory: ChunkData = {
    id: 'chunk-002',
    source_id: 'source-airbnb',
    document_url: 'https://www.airbnb.com/search',
    document_title: 'Search Results',
    source_domain: 'airbnb.com',
    chunk_content: `
# Search Results Page
- Navigation bar
- Filter sidebar
- Listing grid
- Map view
    `,
    chunk_index: 1,
    created_at: new Date(),
  };

  // UT-PB-01: Build task-driven Prompt
  it('Build task-driven Prompt', () => {
    const result = buildTaskDrivenPrompt(mockChunkTaskDriven, mockChunkTaskDriven.chunk_content);

    expect(result.chunkType).toBe('task_driven');
    expect(result.systemPrompt).toContain('interact');
    expect(result.systemPrompt).toContain('observe_page');
    expect(result.systemPrompt).toContain('snake_case');
    expect(result.userPrompt).toContain('Record UI Element Patterns');
    expect(result.userPrompt).toContain('Task: Search for hotels');
    expect(result.userPrompt).toContain(mockChunkTaskDriven.document_url);
    expect(result.userPrompt).toContain(mockChunkTaskDriven.document_title);
    expect(result.userPrompt).toContain("Today's date:");
  });

  // UT-PB-02: Build exploratory Prompt
  it('Build exploratory Prompt', () => {
    const result = buildExploratoryPrompt(mockChunkExploratory, mockChunkExploratory.chunk_content);

    expect(result.chunkType).toBe('exploratory');
    expect(result.systemPrompt).toContain('register_element');
    expect(result.systemPrompt).toContain('snake_case');
    expect(result.userPrompt).toContain('Page Content');
    expect(result.userPrompt).toContain('https://www.airbnb.com/search');
    expect(result.userPrompt).toContain('Search Results');
    expect(result.userPrompt).toContain("Today's date:");
  });

  // UT-PB-03: Token limit truncation
  it('Auto-truncate oversized chunk', () => {
    const largeContent = 'A'.repeat(30000);
    const mockChunk: ChunkData = {
      ...mockChunkTaskDriven,
      chunk_content: largeContent,
    };

    const result = buildPrompt(mockChunk, 'task_driven');

    expect(result.userPrompt.length).toBeLessThan(30000);
    expect(result.userPrompt).toContain('[... content truncated ...]');
  });
});
