/// Parameters for searching actions (new text-based API)
#[derive(Debug, Default)]
pub struct SearchActionsParams {
    pub query: String,
    pub domain: Option<String>,
    pub background: Option<String>,
    pub url: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}
