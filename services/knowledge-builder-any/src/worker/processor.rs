//! Task processor for handling individual build tasks

use crate::chunker::{hash_content, ChunkerOptions, DocumentChunker};
use crate::db::models::{BuildTask, NewChunk, NewDocument, NewSource, NewSourceVersion};
use crate::db::{chunks, documents, source_versions, sources, DbPool};
use crate::embedding::{EmbeddingClient, OptionalEmbeddingClient};
use crate::error::Result;
use crate::worker::WorkerConfig;
use crate::{build_handbook_simple, HandbookOutput};
use tracing::{info, warn};
use url::Url;

/// Task processor that handles handbook generation and storage
pub struct TaskProcessor {
    config: WorkerConfig,
    embedding_client: OptionalEmbeddingClient,
}

impl TaskProcessor {
    /// Create a new task processor
    pub fn new(config: WorkerConfig, openai_api_key: Option<&str>) -> Self {
        let embedding_client = if config.enable_embeddings {
            match openai_api_key {
                Some(key) => OptionalEmbeddingClient::with_client(EmbeddingClient::new(
                    key,
                    &config.embedding_model,
                )),
                None => OptionalEmbeddingClient::from_env(),
            }
        } else {
            OptionalEmbeddingClient::none()
        };

        if !embedding_client.is_enabled() && config.enable_embeddings {
            warn!("Embeddings enabled but OPENAI_API_KEY not set - embeddings will be skipped");
        }

        Self {
            config,
            embedding_client,
        }
    }

    /// Process a single build task
    ///
    /// Returns the source_id on success
    pub async fn process(&self, pool: &DbPool, task: &BuildTask) -> Result<i32> {
        let url = &task.source_url;

        // Step 1: Build handbook (simple mode - no custom prompts)
        info!("Building handbook for: {}", url);
        let handbook = build_handbook_simple(url).await?;

        // Step 2: Create or get source
        let source_id = self.ensure_source(pool, url, &handbook).await?;
        info!("Source ID: {}", source_id);

        // Step 3: Create new version for Blue/Green deployment
        let version = source_versions::create_version(
            pool,
            &NewSourceVersion {
                source_id,
                commit_message: Some(format!("Build at {}", chrono::Utc::now().to_rfc3339())),
                created_by: Some("knowledge-builder-any".to_string()),
            },
        )
        .await?;
        let version_id = version.id;
        info!(
            "Created version: v{} (ID: {})",
            version.version_number, version_id
        );

        // Step 4: Store action.md as document
        let action_md = handbook.action.to_markdown();
        let action_doc_id = self
            .store_document(
                pool,
                source_id,
                Some(version_id),
                url,
                "action.md",
                "Action Handbook",
                &action_md,
            )
            .await?;
        info!("Created action.md document: {}", action_doc_id);

        // Step 5: Store overview.md as document
        let overview_md = handbook.overview.to_markdown();
        let overview_doc_id = self
            .store_document(
                pool,
                source_id,
                Some(version_id),
                url,
                "overview.md",
                "Overview",
                &overview_md,
            )
            .await?;
        info!("Created overview.md document: {}", overview_doc_id);

        // Step 6: Chunk and embed documents
        self.chunk_and_embed(pool, action_doc_id, Some(version_id), &action_md)
            .await?;
        self.chunk_and_embed(pool, overview_doc_id, Some(version_id), &overview_md)
            .await?;

        // Step 7: Update source last_crawled_at
        sources::update_last_crawled(pool, source_id).await?;

        // Note: Version is NOT published here - it stays in 'building' status
        // The version should be published after action_build stage completes
        // (handled by action-builder or API)

        info!("Task completed successfully for: {}", url);
        Ok(source_id)
    }

    /// Ensure source exists, create if not
    async fn ensure_source(
        &self,
        pool: &DbPool,
        url: &str,
        handbook: &HandbookOutput,
    ) -> Result<i32> {
        let parsed = Url::parse(url)?;
        let base_url = format!(
            "{}://{}",
            parsed.scheme(),
            parsed.host_str().unwrap_or("unknown")
        );
        let domain = parsed.host_str().map(|s| s.to_string());

        // Check if source already exists
        if let Some(source) = sources::get_source_by_url(pool, &base_url).await? {
            info!("Found existing source: {} (id={})", source.name, source.id);
            return Ok(source.id);
        }

        // Create new source
        let new_source = NewSource {
            name: handbook.site_name.clone(),
            base_url,
            description: Some(handbook.overview.overview.clone()),
            domain,
        };

        let source_id = sources::create_source(pool, &new_source).await?;
        info!("Created new source: {} (id={})", handbook.site_name, source_id);

        Ok(source_id)
    }

    /// Store a document
    async fn store_document(
        &self,
        pool: &DbPool,
        source_id: i32,
        source_version_id: Option<i32>,
        base_url: &str,
        doc_name: &str,
        title: &str,
        content: &str,
    ) -> Result<i32> {
        // Use fragment identifier to distinguish different handbook documents
        // while keeping the base URL valid and accessible.
        // The fragment is ignored by browsers when accessing the URL.
        // Example: https://dev.to#handbook-action, https://dev.to#handbook-overview
        let handbook_type = doc_name
            .trim_end_matches(".md")
            .to_lowercase()
            .replace(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_', "-");
        let doc_url = format!(
            "{}#handbook-{}",
            base_url.trim_end_matches('/'),
            handbook_type
        );
        let url_hash = documents::generate_url_hash(&doc_url);
        let content_hash = documents::generate_content_hash(content);

        // Check if document already exists
        if let Some(existing) =
            documents::get_document_by_url_hash(pool, source_id, &url_hash).await?
        {
            // Update if content changed
            if existing.content_hash.as_deref() != Some(&content_hash) {
                info!(
                    "Updating existing document {} (content changed)",
                    existing.id
                );
                documents::update_document_content(pool, existing.id, content, &content_hash)
                    .await?;

                // Delete old chunks
                chunks::delete_chunks_by_document(pool, existing.id).await?;
            } else {
                info!("Document {} unchanged, skipping update", existing.id);
            }
            return Ok(existing.id);
        }

        // Create new document
        let new_doc = NewDocument {
            source_id,
            source_version_id,
            url: doc_url,
            url_hash,
            title: Some(title.to_string()),
            description: None,
            content_md: Some(content.to_string()),
            content_hash: Some(content_hash),
            depth: 0,
        };

        documents::insert_document(pool, &new_doc).await
    }

    /// Chunk document and generate embeddings
    async fn chunk_and_embed(
        &self,
        pool: &DbPool,
        document_id: i32,
        source_version_id: Option<i32>,
        content: &str,
    ) -> Result<()> {
        let chunker = DocumentChunker::new(ChunkerOptions::default());
        let chunk_data = chunker.chunk(content);

        info!(
            "Generated {} chunks for document {}",
            chunk_data.len(),
            document_id
        );

        if chunk_data.is_empty() {
            warn!("No chunks generated for document {}", document_id);
            return Ok(());
        }

        // Generate embeddings if enabled
        let mut new_chunks: Vec<NewChunk> = Vec::new();

        for chunk in chunk_data {
            let embedding = self.embedding_client.embed(&chunk.content).await;

            new_chunks.push(NewChunk {
                document_id,
                source_version_id,
                content: chunk.content.clone(),
                content_hash: hash_content(&chunk.content),
                chunk_index: chunk.chunk_index,
                start_char: chunk.start_char,
                end_char: chunk.end_char,
                heading: chunk.heading,
                heading_hierarchy: chunk.heading_hierarchy,
                token_count: chunk.token_count,
                embedding,
                embedding_model: if self.embedding_client.is_enabled() {
                    Some(self.config.embedding_model.clone())
                } else {
                    None
                },
            });
        }

        chunks::insert_chunks(pool, &new_chunks).await?;
        info!(
            "Inserted {} chunks for document {}",
            new_chunks.len(),
            document_id
        );

        Ok(())
    }
}
