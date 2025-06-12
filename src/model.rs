#![allow(dead_code)]

use crate::macros::{impl_as_str, impl_display};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use std::sync::Arc;
use strum::IntoStaticStr;

impl_as_str!(
    PageToken,
    MessageId,
    LabelId,
    ThreadId,
    HistoryId,
    PartId,
    AttachmentId
);
impl_display!(LabelId, ThreadId, MessageId, PartId, AttachmentId);

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
    pub email_address: String,
    pub messages_total: usize,
    pub threads_total: usize,
    pub history_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ThreadId(String);

#[derive(Debug, Deserialize)]
pub struct HistoryId(String);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    pub id: ThreadId,
    pub history_id: HistoryId,
    pub messages: Vec<MinimalMessage>,
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
    pub id: ThreadId,
    pub history_id: HistoryId,
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
    pub id: MessageId,
    pub thread_id: ThreadId,
    pub label_ids: Vec<LabelId>,
    pub snippet: String,
    pub history_id: HistoryId,
    #[serde(deserialize_with = "deserialize_unix_ts_str")]
    pub internal_date: DateTime<Utc>,
    pub size_estimate: usize,
    pub payload: MessagePart,
}

fn deserialize_unix_ts_str<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = <&str>::deserialize(deserializer)?;
    let millis: i64 = s.parse().map_err(serde::de::Error::custom)?;
    let dt = DateTime::from_timestamp_millis(millis)
        .ok_or_else(|| serde::de::Error::custom("invalid range"))?;
    Ok(dt)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawMessage {
    pub id: MessageId,
    pub thread_id: ThreadId,
    #[serde(default)]
    pub label_ids: Vec<LabelId>,
    pub snippet: String,
    pub history_id: HistoryId,
    #[serde(deserialize_with = "deserialize_unix_ts_str")]
    pub internal_date: DateTime<Utc>,
    pub size_estimate: usize,
    #[serde(deserialize_with = "deserialize_base64")]
    pub raw: Vec<u8>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PartId(String);

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessagePart {
    pub part_id: PartId,
    pub mime_type: String,
    pub filename: String,
    #[serde(default)]
    pub headers: Vec<Header>,
    pub body: MessagePartBody,
    #[serde(default)]
    pub parts: Vec<MessagePart>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AttachmentId(String);

impl From<String> for AttachmentId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessagePartBody {
    pub size: usize,
    pub attachment_id: Option<AttachmentId>,
    #[serde(default, deserialize_with = "deserialize_optional_base64")]
    pub data: Option<Vec<u8>>,
}

fn deserialize_optional_base64<'de, D>(deserializer: D) -> Result<Option<Vec<u8>>, D::Error>
where
    D: Deserializer<'de>,
{
    use base64::{Engine as _, engine::general_purpose::URL_SAFE};
    let Some(s) = <Option<&str>>::deserialize(deserializer)? else {
        return Ok(None);
    };
    let data = URL_SAFE.decode(s).map_err(serde::de::Error::custom)?;
    Ok(Some(data))
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub size: usize,
    #[serde(deserialize_with = "deserialize_base64")]
    pub data: Vec<u8>,
}

fn deserialize_base64<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    use base64::{Engine as _, engine::general_purpose::URL_SAFE};
    let s = <&str>::deserialize(deserializer)?;
    let data = URL_SAFE.decode(s).map_err(serde::de::Error::custom)?;
    Ok(data)
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Header {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MinimalLabel {
    pub id: LabelId,
    pub name: String,
    pub message_list_visibility: Option<MessageListVisibility>,
    pub label_list_visibility: Option<LabelListVisibility>,
    pub r#type: LabelType,
}

#[derive(Debug, Deserialize)]
pub struct LabelList {
    pub labels: Vec<MinimalLabel>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Label {
    pub id: LabelId,
    pub name: String,
    pub message_list_visibility: Option<MessageListVisibility>,
    pub label_list_visibility: Option<LabelListVisibility>,
    pub r#type: LabelType,
    pub color: Option<LabelColor>,
}

#[derive(Debug, Deserialize, PartialEq, Eq, Clone, Copy, IntoStaticStr)]
#[serde(rename_all = "camelCase")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum MessageListVisibility {
    Show,
    Hide,
}

#[derive(Debug, Deserialize, PartialEq, Eq, Clone, Copy, IntoStaticStr)]
#[serde(rename_all = "camelCase")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum LabelListVisibility {
    #[serde(rename = "labelShow")]
    Show,
    #[serde(rename = "labelShowIfUnread")]
    ShowIfUnread,
    #[serde(rename = "labelHide")]
    Hide,
}

#[derive(Debug, Deserialize, PartialEq, Eq, Clone, Copy, IntoStaticStr)]
#[serde(rename_all = "camelCase")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum LabelType {
    System,
    User,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelColor {
    pub text_color: String,
    pub background_color: String,
}
