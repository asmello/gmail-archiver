use std::sync::Arc;

use crate::oauth::impl_as_str;
use serde::Deserialize;

impl_as_str!(PageToken, MessageId);

pub struct PageParts<T> {
    pub next_page_token: Option<PageToken>,
    pub items: Vec<T>,
}

pub trait Page<T> {
    fn decompose(self) -> PageParts<T>;
}

#[derive(Debug, Deserialize)]
pub struct PageToken(String);

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UserProfile {
    email_address: String,
    messages_total: usize,
    threads_total: usize,
    history_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ThreadId(String);

#[derive(Debug, Deserialize)]
pub struct HistoryId(String);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    id: ThreadId,
    snippet: String,
    history_id: HistoryId,
    messages: Vec<MinimalMessage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MinimalMessage {
    pub id: MessageId,
    pub thread_id: ThreadId,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MinimalThread {
    id: ThreadId,
    snippet: String,
    history_id: HistoryId,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadsPage {
    pub threads: Vec<MinimalThread>,
    pub next_page_token: Option<PageToken>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MessageId(Arc<str>);

#[derive(Debug, Deserialize)]
pub struct LabelId(String);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FullMessage {
    id: MessageId,
    thread_id: ThreadId,
    label_ids: Vec<LabelId>,
    snippet: String,
    history_id: HistoryId,
    internal_date: String,
    size_estimate: usize,
    payload: MessagePart,
}

#[derive(Debug, Deserialize)]
pub struct PartId(String);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessagePart {
    part_id: PartId,
    mime_type: String,
    filename: String,
    headers: Vec<Header>,
    body: MessagePartBody,
    parts: Option<Vec<MessagePart>>,
}

#[derive(Debug, Deserialize)]
pub struct AttachmentId(String);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessagePartBody {
    size: usize,
    attachment_id: Option<AttachmentId>,
    data: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Header {
    name: String,
    value: String,
}
