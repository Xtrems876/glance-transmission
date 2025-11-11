pub mod api;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct TransmissionTorrent {
    pub percentDone: f64,
    pub status: i32,
    pub rateDownload: u64,
    pub rateUpload: u64,
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
