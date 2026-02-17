use anyhow::Result;
use serde::Deserialize;

const HF_API_BASE: &str = "https://huggingface.co/api/models";
const HF_DOWNLOAD_BASE: &str = "https://huggingface.co";

#[derive(Debug, Deserialize)]
pub struct HfModelInfo {
    pub id: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub siblings: Vec<HfFileSibling>,
}

#[derive(Debug, Deserialize)]
pub struct HfFileSibling {
    pub rfilename: String,
    #[serde(default)]
    pub size: Option<u64>,
}

pub async fn fetch_model_info(model_id: &str) -> Result<HfModelInfo> {
    let url = format!("{}/{}", HF_API_BASE, model_id);
    let client = reqwest::Client::new();
    let resp = client.get(&url)
        .header("User-Agent", "TalkToMe/0.1")
        .send()
        .await?
        .error_for_status()?;

    let info: HfModelInfo = resp.json().await?;
    Ok(info)
}

pub fn download_url(model_id: &str, filename: &str) -> String {
    format!("{}/{}/resolve/main/{}", HF_DOWNLOAD_BASE, model_id, filename)
}
