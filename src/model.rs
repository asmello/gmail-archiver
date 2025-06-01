use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    id: String,
    thread_id: String,
    label_ids: Vec<String>,
    snippet: String,
    history_id: String,
    internal_date: String,
    size_estimate: usize,
    raw: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UserProfile {
    email_address: String,
    messages_total: usize,
    threads_total: usize,
    history_id: String,
}
