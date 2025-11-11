use actix_web::{web, App, HttpServer, Responder, HttpResponse, HttpRequest};
use log::{info, error, debug};
use serde::Deserialize;
use crate::{TransmissionResponse, TransmissionResponseArgs};

#[derive(Deserialize)]
struct QueryParams {
    url: Option<String>,
    username: Option<String>,
    password: Option<String>,
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
        "arguments": { "fields": ["percentDone", "status", "rateDownload", "rateUpload"] }
    });

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(reqwest::header::CONTENT_TYPE, "application/json".parse().unwrap());

    let auth = if let (Some(u), Some(p)) = (params.username, params.password) {
        Some((u, p))
    } else {
        None
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
    let rate_dl: u64 = torrents.iter().map(|t| t.rateDownload).sum();
    let rate_ul: u64 = torrents.iter().map(|t| t.rateUpload).sum();
    let completed = torrents.iter().filter(|t| (t.percentDone - 1.0).abs() < std::f64::EPSILON).count();
    let leech = torrents.len().saturating_sub(completed);

    // Build simple HTML fragment similar to the widget blocks
    let html = format!(
        "<div class=\"glance-transmission\">\n  <div class=\"row\"><strong>Leech</strong>: {} </div>\n  <div class=\"row\"><strong>Download</strong>: {} B/s</div>\n  <div class=\"row\"><strong>Seed</strong>: {} </div>\n  <div class=\"row\"><strong>Upload</strong>: {} B/s</div>\n</div>",
        leech,
        rate_dl,
        completed,
        rate_ul
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
