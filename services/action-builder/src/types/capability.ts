// Re-export types from db package for consistency
export type {
  SelectorType,
  SelectorItem,
  TemplateParam,
} from '@actionbookdev/db'

/**
 * Element operation argument definition
 */
export interface ArgumentDef {
  name: string
  type: 'string' | 'number' | 'boolean' | 'enum'
  description: string
  required?: boolean
  enum_values?: string[]
}

/**
 * Page module type for categorizing elements by their location on the page
 * Used for organizing and displaying elements in a structured way
 */
export type PageModule =
  | 'header'
  | 'footer'
  | 'sidebar'
  | 'navibar'
  | 'main'
  | 'modal'
  | 'breadcrumb'
  | 'tab'
  | 'unknown'

/**
 * Element type enumeration
 * - Interactive types: button, input, link, select, checkbox, radio
 * - Data display types: text, data_field, container, list, list_item
 */
export type ElementType =
  | 'button'
  | 'input'
  | 'link'
  | 'select'
  | 'checkbox'
  | 'radio'
  | 'text' // Static text element for reading
  | 'data_field' // Data field that contains extractable value
  | 'container' // Container element that groups other elements
  | 'list' // List container with repeating items
  | 'list_item' // Single item in a list
  | 'other'

/**
 * Allowed interaction methods
 */
export type AllowMethod =
  | 'click'
  | 'type'
  | 'clear'
  | 'scroll'
  | 'hover'
  | 'select'
  | 'extract' // For data extraction from text/data_field elements

/**
 * Action method types for Stagehand actions
 */
export type ActionMethod =
  | 'click'
  | 'fill'
  | 'type'
  | 'press'
  | 'scroll'
  | 'select'
  | 'hover'

/**
 * Action object for direct Stagehand execution (bypasses AI inference)
 * This is the format Stagehand accepts for selector-based actions
 *
 * @example
 * const action: ActionObject = {
 *   selector: "#search-btn",
 *   description: "Click the search button",
 *   method: "click"
 * };
 *
 * @example
 * const action: ActionObject = {
 *   selector: "#email-input",
 *   description: "Fill email field",
 *   method: "fill",
 *   arguments: ["user@example.com"]
 * };
 */
export interface ActionObject {
  /** CSS selector or XPath to target the element */
  selector: string
  /** Human-readable description of the action (helps with debugging) */
  description: string
  /** The method to execute on the element */
  method: ActionMethod
  /** Optional arguments for the method (e.g., text to fill) */
  arguments?: string[]
}

// Import SelectorItem for use in ElementCapability
import type { SelectorItem } from '@actionbookdev/db'

/**
 * Single UI element capability definition
 */
export interface ElementCapability {
  id: string
  /** Multi-selector format with template support */
  selectors: SelectorItem[]
  description: string
  element_type: ElementType
  allow_methods: AllowMethod[]
  arguments?: ArgumentDef[]
  leads_to?: string
  wait_after?: number
  confidence?: number
  discovered_at: string

  // === New fields for element relationships and data extraction ===

  /** Parent element ID - indicates this element is a child of another */
  parent?: string

  /** Element ID that must be interacted with before this element becomes visible/accessible */
  depends_on?: string

  /** Condition for element visibility (e.g., "parent_expanded", "after_click:element_id") */
  visibility_condition?: string

  /** For list_item: indicates this is a repeating element pattern */
  is_repeating?: boolean

  /** For data_field: the data key name for extraction (e.g., "founders", "categories") */
  data_key?: string

  /** For container/list: IDs of child elements */
  children?: string[]

  /** Element's page module location (LLM inferred) */
  module?: PageModule

  // Input-specific attributes
  /** For input elements: the input type (text, email, password, number, etc.) */
  input_type?: string

  /** For input elements: the name attribute */
  input_name?: string

  /** For input elements: the default/current value */
  input_value?: string

  // Link-specific attributes
  /** For link elements: the href URL or pattern */
  href?: string
}

/**
 * Page capability definition
 */
export interface PageCapability {
  page_type: string
  name: string
  description: string
  url_patterns: string[]
  wait_for?: string
  elements: Record<string, ElementCapability>
}

/**
 * Site capability definition (root aggregate)
 */
export interface SiteCapability {
  domain: string
  name: string
  description: string
  version: string
  recorded_at: string
  scenario: string
  health_score?: number
  global_elements: Record<string, ElementCapability>
  pages: Record<string, PageCapability>
}

/**
 * Observe result item from Stagehand
 */
export interface ObserveResultItem {
  description?: string
  selector?: string
  method?: string
  arguments?: unknown[]
}
