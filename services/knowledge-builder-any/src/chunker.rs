//! Document chunker for splitting markdown content into smaller pieces
//!
//! Ported from TypeScript implementation in services/knowledge-builder

use crate::db::models::HeadingItem;
use regex::Regex;
use sha2::{Digest, Sha256};

/// Chunker configuration options
#[derive(Debug, Clone)]
pub struct ChunkerOptions {
    /// Target tokens per chunk
    pub chunk_size: usize,
    /// Overlap tokens between chunks
    pub chunk_overlap: usize,
    /// Minimum chunk size
    pub min_chunk_size: usize,
    /// Only split at this heading level (1=H1, 2=H2, etc.)
    pub split_heading_level: usize,
}

impl Default for ChunkerOptions {
    fn default() -> Self {
        Self {
            chunk_size: 2000,
            chunk_overlap: 50,
            min_chunk_size: 100,
            split_heading_level: 2,
        }
    }
}

/// Data for a single chunk
#[derive(Debug, Clone)]
pub struct ChunkData {
    pub content: String,
    pub chunk_index: i32,
    pub start_char: i32,
    pub end_char: i32,
    pub heading: Option<String>,
    pub heading_hierarchy: Vec<HeadingItem>,
    pub token_count: i32,
}

/// Internal section representation
#[derive(Debug)]
struct Section {
    content: String,
    heading: String,
    heading_hierarchy: Vec<HeadingItem>,
    start_char: usize,
}

/// Headings to skip (noise sections)
const SKIP_HEADINGS: &[&str] = &[
    "Related articles",
    "Related topics",
    "Site Footer",
    "Support",
    "Hosting",
    "Airbnb",
];

/// Document chunker
pub struct DocumentChunker {
    options: ChunkerOptions,
}

impl DocumentChunker {
    /// Create a new chunker with options
    pub fn new(options: ChunkerOptions) -> Self {
        Self { options }
    }

    /// Split a markdown document into chunks
    pub fn chunk(&self, content: &str) -> Vec<ChunkData> {
        let sections = self.split_by_headings(content);
        let mut chunks = Vec::new();
        let mut chunk_index = 0;

        for section in sections {
            // Skip noise sections
            if SKIP_HEADINGS.contains(&section.heading.as_str()) {
                continue;
            }

            let section_chunks = self.chunk_section(&section, chunk_index);
            chunk_index += section_chunks.len() as i32;
            chunks.extend(section_chunks);
        }

        chunks
    }

    /// Split content by markdown headings at the specified level
    fn split_by_headings(&self, content: &str) -> Vec<Section> {
        let split_level = self.options.split_heading_level;

        // Pattern to match headings up to split_level
        let pattern = format!(r"(?m)^(#{{1,{}}})[\t ]+(.+)$", split_level);
        let heading_re = Regex::new(&pattern).unwrap();

        // Find all split positions (where heading level equals split_level)
        let mut split_positions: Vec<usize> = Vec::new();

        for cap in heading_re.captures_iter(content) {
            let hashes = cap.get(1).unwrap().as_str();
            if hashes.len() == split_level {
                split_positions.push(cap.get(0).unwrap().start());
            }
        }
        split_positions.push(content.len());

        let mut sections = Vec::new();
        let mut heading_stack: Vec<(usize, String)> = Vec::new();

        // Handle content before first split-level heading
        if !split_positions.is_empty() && split_positions[0] > 0 {
            let intro_content = &content[0..split_positions[0]];
            if !intro_content.trim().is_empty() {
                let h1_re = Regex::new(r"(?m)^#[\t ]+(.+)$").unwrap();
                let (heading, hierarchy) = if let Some(cap) = h1_re.captures(intro_content) {
                    let text = cap.get(1).unwrap().as_str().trim().to_string();
                    (text.clone(), vec![HeadingItem { level: 1, text }])
                } else {
                    (String::new(), vec![])
                };

                sections.push(Section {
                    content: intro_content.to_string(),
                    heading,
                    heading_hierarchy: hierarchy,
                    start_char: 0,
                });
            }
        }

        // Process each section
        for i in 0..split_positions.len().saturating_sub(1) {
            let start = split_positions[i];
            let end = split_positions[i + 1];
            let section_content = &content[start..end];

            // Extract heading from this section
            let first_line_re = Regex::new(r"(?m)^(#{1,6})[\t ]+(.+)$").unwrap();
            let (heading, level) = if let Some(cap) = first_line_re.captures(section_content) {
                let hashes = cap.get(1).unwrap().as_str();
                let text = cap.get(2).unwrap().as_str().trim().to_string();
                (text, hashes.len())
            } else {
                (String::new(), split_level)
            };

            // Update heading stack
            while !heading_stack.is_empty() && heading_stack.last().unwrap().0 >= level {
                heading_stack.pop();
            }
            if !heading.is_empty() {
                heading_stack.push((level, heading.clone()));
            }

            sections.push(Section {
                content: section_content.to_string(),
                heading,
                heading_hierarchy: heading_stack
                    .iter()
                    .map(|(l, t)| HeadingItem {
                        level: *l as i32,
                        text: t.clone(),
                    })
                    .collect(),
                start_char: start,
            });
        }

        // If no sections found, treat entire content as one section
        if sections.is_empty() {
            let h1_re = Regex::new(r"(?m)^#[\t ]+(.+)$").unwrap();
            let (heading, hierarchy) = if let Some(cap) = h1_re.captures(content) {
                let text = cap.get(1).unwrap().as_str().trim().to_string();
                (text.clone(), vec![HeadingItem { level: 1, text }])
            } else {
                (String::new(), vec![])
            };

            sections.push(Section {
                content: content.to_string(),
                heading,
                heading_hierarchy: hierarchy,
                start_char: 0,
            });
        }

        sections
    }

    /// Chunk a single section
    fn chunk_section(&self, section: &Section, start_index: i32) -> Vec<ChunkData> {
        let tokens = self.estimate_tokens(&section.content);

        // If section is small enough, return as single chunk
        if tokens <= self.options.chunk_size {
            return vec![ChunkData {
                content: section.content.trim().to_string(),
                chunk_index: start_index,
                start_char: section.start_char as i32,
                end_char: (section.start_char + section.content.len()) as i32,
                heading: if section.heading.is_empty() {
                    None
                } else {
                    Some(section.heading.clone())
                },
                heading_hierarchy: section.heading_hierarchy.clone(),
                token_count: tokens as i32,
            }];
        }

        // Split by paragraphs
        let paragraphs = self.split_paragraphs(&section.content);
        let mut chunks = Vec::new();

        let mut current_chunk = String::new();
        let mut current_tokens = 0;
        let mut chunk_start_char = section.start_char;
        let mut char_offset = 0;

        for paragraph in &paragraphs {
            let para_tokens = self.estimate_tokens(paragraph);

            // If adding this paragraph exceeds chunk size
            if current_tokens + para_tokens > self.options.chunk_size && !current_chunk.is_empty() {
                // Save current chunk
                chunks.push(ChunkData {
                    content: current_chunk.trim().to_string(),
                    chunk_index: start_index + chunks.len() as i32,
                    start_char: chunk_start_char as i32,
                    end_char: (section.start_char + char_offset) as i32,
                    heading: if section.heading.is_empty() {
                        None
                    } else {
                        Some(section.heading.clone())
                    },
                    heading_hierarchy: section.heading_hierarchy.clone(),
                    token_count: current_tokens as i32,
                });

                // Start new chunk with overlap
                let overlap = self.get_overlap_text(&current_chunk);
                current_chunk = format!("{}{}", overlap, paragraph);
                current_tokens = self.estimate_tokens(&current_chunk);
                chunk_start_char = section.start_char + char_offset - overlap.len();
            } else {
                current_chunk.push_str(paragraph);
                current_tokens += para_tokens;
            }

            char_offset += paragraph.len();
        }

        // Add final chunk
        if !current_chunk.trim().is_empty() {
            let final_tokens = self.estimate_tokens(&current_chunk);

            if final_tokens >= self.options.min_chunk_size || chunks.is_empty() {
                chunks.push(ChunkData {
                    content: current_chunk.trim().to_string(),
                    chunk_index: start_index + chunks.len() as i32,
                    start_char: chunk_start_char as i32,
                    end_char: (section.start_char + section.content.len()) as i32,
                    heading: if section.heading.is_empty() {
                        None
                    } else {
                        Some(section.heading.clone())
                    },
                    heading_hierarchy: section.heading_hierarchy.clone(),
                    token_count: final_tokens as i32,
                });
            } else if let Some(last_chunk) = chunks.last_mut() {
                // Merge small final chunk into previous
                last_chunk.content = format!("{}\n\n{}", last_chunk.content, current_chunk.trim());
                last_chunk.end_char = (section.start_char + section.content.len()) as i32;
                last_chunk.token_count = self.estimate_tokens(&last_chunk.content) as i32;
            }
        }

        chunks
    }

    /// Split content into paragraphs while preserving code blocks
    fn split_paragraphs(&self, content: &str) -> Vec<String> {
        // Protect code blocks
        let code_block_re = Regex::new(r"```[\s\S]*?```").unwrap();
        let mut code_blocks = Vec::new();
        let protected_content = code_block_re
            .replace_all(content, |caps: &regex::Captures| {
                code_blocks.push(caps[0].to_string());
                format!("__CODE_BLOCK_{}__", code_blocks.len() - 1)
            })
            .to_string();

        // Split by double newlines
        let paragraphs: Vec<String> = protected_content
            .split("\n\n")
            .map(|p| format!("{}\n\n", p))
            .collect();

        // Restore code blocks
        paragraphs
            .into_iter()
            .map(|para| {
                let code_block_placeholder_re = Regex::new(r"__CODE_BLOCK_(\d+)__").unwrap();
                code_block_placeholder_re
                    .replace_all(&para, |caps: &regex::Captures| {
                        let index: usize = caps[1].parse().unwrap();
                        code_blocks.get(index).cloned().unwrap_or_default()
                    })
                    .to_string()
            })
            .collect()
    }

    /// Get overlap text from the end of a chunk
    fn get_overlap_text(&self, text: &str) -> String {
        let words: Vec<&str> = text.trim().split_whitespace().collect();
        let start = words.len().saturating_sub(self.options.chunk_overlap);
        let overlap_words: Vec<&str> = words[start..].to_vec();
        format!("{} ", overlap_words.join(" "))
    }

    /// Estimate token count (rough approximation: ~4 chars per token)
    fn estimate_tokens(&self, text: &str) -> usize {
        (text.len() + 3) / 4 // Ceiling division
    }
}

impl Default for DocumentChunker {
    fn default() -> Self {
        Self::new(ChunkerOptions::default())
    }
}

/// Generate a hash for content (SHA256, first 16 chars)
pub fn hash_content(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())[..16].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_chunk() {
        let chunker = DocumentChunker::default();
        let content = "# Hello\n\nThis is a test document.";
        let chunks = chunker.chunk(content);

        assert!(!chunks.is_empty());
        assert!(chunks[0].content.contains("Hello"));
    }

    #[test]
    fn test_hash_content() {
        let hash = hash_content("Hello, World!");
        assert_eq!(hash.len(), 16);

        // Same content should produce same hash
        let hash2 = hash_content("Hello, World!");
        assert_eq!(hash, hash2);
    }

    #[test]
    fn test_heading_extraction() {
        let chunker = DocumentChunker::default();
        let content = "# Main Title\n\n## Section 1\n\nContent 1\n\n## Section 2\n\nContent 2";
        let chunks = chunker.chunk(content);

        assert!(chunks.len() >= 2);
    }
}
