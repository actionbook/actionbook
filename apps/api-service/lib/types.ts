/**
 * Actionbook API Types
 * Shared type definitions for API service
 */

// Element types
export type ElementType =
  | 'button'
  | 'input'
  | 'link'
  | 'text'
  | 'container'
  | 'form'
  | 'image'

// Action types
export type ActionType = 'site' | 'page' | 'element' | 'scenario'

// Element definition
export interface Element {
  id: string
  name: string
  type: ElementType
  selector: string
  description: string
  actions: string[]
  pageId: string
}

// Page definition
export interface Page {
  id: string
  name: string
  path: string
  description: string
  elements: Element[]
  siteId: string
}

// Site definition
export interface Site {
  id: string
  name: string
  domain: string
  description: string
  pages: Page[]
}

// Scenario step
export interface ScenarioStep {
  order: number
  action: string
  element: string
  selector: string
  value?: string
  description: string
}

// Scenario definition
export interface Scenario {
  id: string
  name: string
  description: string
  siteId: string
  siteName: string
  steps: ScenarioStep[]
  tags: string[]
}

// Search result item
export interface SearchResultItem {
  id: string
  type: ActionType
  name: string
  description: string
  site?: string
  page?: string
  relevance: number
}

// Search response
export interface SearchResponse {
  results: SearchResultItem[]
  total: number
  page: number
  hasMore: boolean
}

// Action detail response (union type)
export type ActionDetail = Site | Page | Element | Scenario

// Health check response
export interface HealthResponse {
  status: 'healthy' | 'degraded' | 'unhealthy'
  timestamp: string
  version: string
  services: {
    database: boolean
    cache: boolean
  }
}

// API error response
export interface ApiError {
  error: string
  code: string
  message: string
  suggestion?: string
}
