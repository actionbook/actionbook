/**
 * Chunk Detector Unit Tests
 */

import { describe, it, expect } from 'vitest';
import { detectChunkType } from '../../../src/task-worker/utils/chunk-detector';

describe('detectChunkType', () => {
  // UT-TE-03: Identify task-driven (step numbers)
  it('Identify task-driven (step numbers)', () => {
    const content = `
## Task: Search for hotels
**Steps:**
1. Click search box
2. Type "Tokyo"
3. Select dates
4. Click search button
    `;

    expect(detectChunkType(content)).toBe('task_driven');
  });

  // UT-TE-04: Identify exploratory
  it('Identify exploratory', () => {
    const content = `
# Homepage
- Navigation bar
- Search area with filters
- Content grid showing listings
- Footer with links
    `;

    expect(detectChunkType(content)).toBe('exploratory');
  });

  // UT-TE-05: Edge case (keyword matching)
  it('Edge case: keyword matching', () => {
    const taskContent = 'Task: search for hotels in Tokyo';
    const scenarioContent = 'Scenario: User books a hotel';
    const normalContent = 'This is a normal page description';

    expect(detectChunkType(taskContent)).toBe('task_driven');
    expect(detectChunkType(scenarioContent)).toBe('task_driven');
    expect(detectChunkType(normalContent)).toBe('exploratory');
  });
});
