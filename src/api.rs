use actix_web::{web, App, HttpServer, Responder, HttpResponse, HttpRequest};
use log::{info, error, debug};
use serde::Deserialize;
use crate::TransmissionResponse;

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
     .replace('"', "&quot;")
     .replace('\'' , "&#x27;")
}

fn human_bytes_per_sec(bps: u64) -> String {
    const K: f64 = 1024.0;
    let v = bps as f64;
    if v >= K * K {
        format!("{:.1} MiB/s", v / (K * K))
    } else if v >= K {
        format!("{:.1} KiB/s", v / K)
    } else {
        format!("{} B/s", bps)
    }
}

fn human_eta(opt: Option<i64>) -> String {
    match opt {
        Some(e) if e >= 0 => {
            let secs = e as u64;
            let hours = secs / 3600;
            let mins = (secs % 3600) / 60;
            if hours > 0 {
                format!("{}h {}m", hours, mins)
            } else if mins > 0 {
                format!("{}m", mins)
            } else {
                format!("{}s", secs % 60)
            }
        }
        _ => "—".to_string(),
    }
}
#[derive(Deserialize)]
struct QueryParams {
    url: Option<String>,
}

fn add_widget_headers(builder: &mut actix_web::HttpResponseBuilder) {
    builder.insert_header(("Widget-Title", "Transmission"));
    // Widget-Content-Type can be 'html' so Glance will render appropriately
    builder.insert_header(("Widget-Content-Type", "html"));
}

async fn transmission_handler(req: HttpRequest) -> impl Responder {
    // Accept query params: url (base), username, password
    let params: QueryParams = match web::Query::<QueryParams>::from_query(req.query_string()) {
        Ok(q) => q.into_inner(),
        Err(_) => return HttpResponse::BadRequest().json(serde_json::json!({"error": "Invalid query parameters"})),
    };

    let base_url = match params.url {
        Some(u) => u,
        None => return HttpResponse::BadRequest().json(serde_json::json!({"error": "Missing 'url' query parameter (e.g. http://host:9091/)"})),
    };

    // Build RPC endpoint
    let mut rpc = base_url.clone();
    if !rpc.ends_with('/') {
        rpc.push('/');
    }
    rpc.push_str("rpc");

    let client = reqwest::Client::builder().danger_accept_invalid_certs(false).build().unwrap();

    let body = serde_json::json!({
        "method": "torrent-get",
        "arguments": { "fields": ["name", "percentDone", "eta", "rateDownload", "leftUntilDone", "status", "rateUpload"] }
    });

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(reqwest::header::CONTENT_TYPE, "application/json".parse().unwrap());

    // Credentials MUST be supplied via headers for security.
    // Header names: X-Transmission-Username and X-Transmission-Password
    let header_user = req
        .headers()
        .get("X-Transmission-Username")
        .or_else(|| req.headers().get("x-transmission-username"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let header_pass = req
        .headers()
        .get("X-Transmission-Password")
        .or_else(|| req.headers().get("x-transmission-password"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let auth = if let (Some(u), Some(p)) = (header_user, header_pass) {
        Some((u, p))
    } else {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": "Missing credentials in headers: X-Transmission-Username and X-Transmission-Password"}));
    };

    // First request
    // build request, add basic auth only when provided
    let mut req_builder = client.post(&rpc).headers(headers.clone()).body(body.to_string());
    if let Some((u, p)) = auth.as_ref() {
        req_builder = req_builder.basic_auth(u.clone(), Some(p.clone()));
    }
    let mut resp = match req_builder.send().await {
        Ok(r) => r,
        Err(e) => {
            error!("HTTP error contacting Transmission RPC: {}", e);
            return HttpResponse::InternalServerError().body("Error contacting Transmission RPC");
        }
    };

    if resp.status() == reqwest::StatusCode::CONFLICT {
        // Transmission requires session id header; extract and retry
        debug!("Got 409 from Transmission, attempting to retry with session id");
        if let Some(session) = resp.headers().get("x-transmission-session-id") {
            headers.insert("x-transmission-session-id", session.clone());
            let mut retry_builder = client.post(&rpc).headers(headers.clone()).body(body.to_string());
            if let Some((u, p)) = auth.as_ref() {
                retry_builder = retry_builder.basic_auth(u.clone(), Some(p.clone()));
            }
            resp = match retry_builder.send().await {
                Ok(r) => r,
                Err(e) => {
                    error!("HTTP error contacting Transmission RPC on retry: {}", e);
                    return HttpResponse::InternalServerError().body("Error contacting Transmission RPC");
                }
            };
        }
    }

    let status = resp.status();
    let text = match resp.text().await {
        Ok(t) => t,
        Err(e) => {
            error!("Failed to read Transmission response body: {}", e);
            return HttpResponse::InternalServerError().body("Failed to read Transmission response");
        }
    };

    if status != reqwest::StatusCode::OK {
        error!("Transmission RPC returned non-200: {} body: {}", status, text);
        return HttpResponse::InternalServerError().body("Transmission RPC error");
    }

    let parsed: TransmissionResponse = match serde_json::from_str(&text) {
        Ok(p) => p,
        Err(e) => {
            error!("Failed to parse Transmission response JSON: {}\nBody: {}", e, text);
            return HttpResponse::InternalServerError().body("Invalid Transmission response");
        }
    };

    // Compute stats
    let torrents = parsed.arguments.torrents;
    let rate_dl: u64 = torrents.iter().map(|t| t.rate_download).sum();
    let rate_ul: u64 = torrents.iter().map(|t| t.rate_upload).sum();
    let completed = torrents.iter().filter(|t| (t.percent_done - 1.0).abs() < std::f64::EPSILON).count();
    let leech = torrents.len().saturating_sub(completed);

    // Show top N ongoing downloads sorted by percent_done (descending)
    let mut ongoing: Vec<_> = torrents
        .iter()
        .filter(|t| t.percent_done < 1.0 - std::f64::EPSILON)
        .cloned()
        .collect();
    ongoing.sort_by(|a, b| b.percent_done.partial_cmp(&a.percent_done).unwrap_or(std::cmp::Ordering::Equal));
    let max_show = 5usize;
    let mut list_items = String::new();
    for t in ongoing.iter().take(max_show) {
        let name = html_escape(&t.name.as_deref().unwrap_or("(unknown)"));
        let pct = (t.percent_done * 100.0).clamp(0.0, 100.0);
        let pct_str = format!("{:.1}", pct);
        let eta = human_eta(t.eta);
        let speed = human_bytes_per_sec(t.rate_download);

        // simple icon heuristic: downloading => ↓, paused/stalled => ❚❚, otherwise ?
        let icon = if t.percent_done >= 1.0 - std::f64::EPSILON {
            "✔"
        } else if t.rate_download > 0 {
            "↓"
        } else {
            "❚❚"
        };

        list_items.push_str(&format!(
            "<li class=\"flex items-center\" style=\"gap: 10px;\">\n  <div class=\"size-h4\" style=\"flex-shrink: 0;\">{}</div>\n  <div style=\"flex-grow: 1; min-width: 0;\">\n    <div class=\"text-truncate color-highlight\">{}</div>\n    <div title=\"{}%\" style=\"background: rgba(128, 128, 128, 0.2); border-radius: 5px; height: 6px; margin-top: 5px; overflow: hidden;\">\n      <div style=\"width: {}%; background-color: var(--color-positive); height: 100%; border-radius: 5px;\"></div>\n    </div>\n  </div>\n  <div style=\"flex-shrink: 0; text-align: right; width: 80px;\">\n    <div class=\"size-sm color-paragraph\">{} </div>\n    <div class=\"size-sm color-paragraph\">{}</div>\n  </div>\n</li>\n",
            icon,
            name,
            pct_str,
            pct, // width percent
            speed,
            eta
        ));
    }

    let html = format!(
        "<div class=\"list\" style=\"--list-gap: 15px;\">\n  <div class=\"flex justify-between text-center\">\n    <div>\n      <div class=\"color-highlight size-h3\">{}</div>\n      <div class=\"size-h6\">DOWNLOADING</div>\n    </div>\n    <div>\n      <div class=\"color-highlight size-h3\">{}</div>\n      <div class=\"size-h6\">UPLOADING</div>\n    </div>\n    <div>\n      <div class=\"color-highlight size-h3\">{}</div>\n      <div class=\"size-h6\">SEEDING</div>\n    </div>\n    <div>\n      <div class=\"color-highlight size-h3\">{}</div>\n      <div class=\"size-h6\">LEECHING</div>\n    </div>\n  </div>\n\n  <!-- Downloading list -->\n  <div style=\"margin-top: 15px;\">\n    <ul class=\"list collapsible-container\" data-collapse-after=\"0\" style=\"--list-gap: 15px;\">\n{}    </ul>\n  </div>\n</div>",
        human_bytes_per_sec(rate_dl),
        human_bytes_per_sec(rate_ul),
        completed,
        leech,
        list_items
    );

    let mut builder = HttpResponse::Ok();
    add_widget_headers(&mut builder);
    builder.content_type("text/html").body(html)
}

pub async fn run_api_server() -> std::io::Result<()> {
    info!("Starting glance-transmission on 0.0.0.0:8080");
    HttpServer::new(|| {
        App::new()
            .route("/transmission", web::get().to(transmission_handler))
    })
    .bind(("0.0.0.0", 8080))?
    .run()
    .await
}
