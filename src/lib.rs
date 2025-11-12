pub mod api;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct TransmissionTorrent {
    #[serde(rename = "name")]
    pub name: Option<String>,

    #[serde(rename = "percentDone")]
    pub percent_done: f64,

    #[serde(rename = "eta")]
    pub eta: Option<i64>,

    #[serde(rename = "rateDownload")]
    pub rate_download: u64,

    #[serde(rename = "leftUntilDone")]
    pub left_until_done: u64,

    #[serde(rename = "status")]
    pub status: i32,

    #[serde(rename = "rateUpload")]
    #[serde(default)]
    pub rate_upload: u64,
}

#[derive(Debug, serde::Deserialize)]
pub struct TransmissionResponseArgs {
    pub torrents: Vec<TransmissionTorrent>,
}

#[derive(Debug, serde::Deserialize)]
pub struct TransmissionResponse {
    pub result: String,
    pub arguments: TransmissionResponseArgs,
}
