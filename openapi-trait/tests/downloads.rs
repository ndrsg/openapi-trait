//! Integration tests for binary downloads/uploads and response headers.

#[openapi_trait::axum("assets/testdata/downloads.openapi.yaml")]
mod downloads_server {}

#[cfg(feature = "reqwest-client")]
#[openapi_trait::client("assets/testdata/downloads.openapi.yaml")]
pub mod downloads {}

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use downloads_server::DownloadsServerApi as _;
use http_body_util::BodyExt;
use tokio::io::AsyncWriteExt;

#[derive(Clone)]
struct AppState {
    upload_dest: Arc<PathBuf>,
    download_source: Arc<PathBuf>,
}

#[derive(Clone)]
struct MockDownloads;

async fn stream_body_to_file(mut body: Body, path: &Path) -> std::io::Result<usize> {
    let mut file = tokio::fs::File::create(path).await?;
    let mut total = 0usize;
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(std::io::Error::other)?;
        if let Ok(data) = frame.into_data() {
            total += data.len();
            file.write_all(&data).await?;
        }
    }
    file.flush().await?;
    Ok(total)
}

async fn file_to_body(path: &Path) -> std::io::Result<Body> {
    let file = tokio::fs::File::open(path).await?;
    let stream = tokio_util::io::ReaderStream::new(file);
    Ok(Body::from_stream(stream))
}

impl downloads_server::DownloadsServerApi<AppState> for MockDownloads {
    type Error = downloads_server::NotImplemented;

    async fn download_file(
        &self,
        req: downloads_server::DownloadFileRequest,
        state: axum::extract::State<AppState>,
        _headers: axum::http::HeaderMap,
    ) -> Result<downloads_server::DownloadFileResponse, Self::Error> {
        if req.id == "found" {
            let body = file_to_body(&state.download_source)
                .await
                .expect("download source readable");
            Ok(downloads_server::DownloadFileResponse::Status200(
                downloads_server::DownloadFileStatus200 {
                    body,
                    content_disposition: Some("attachment; filename=\"report.pdf\"".to_string()),
                    e_tag: Some("\"abc123\"".to_string()),
                },
            ))
        } else {
            Ok(downloads_server::DownloadFileResponse::Status404)
        }
    }

    async fn upload_file(
        &self,
        req: downloads_server::UploadFileRequest,
        state: axum::extract::State<AppState>,
        _headers: axum::http::HeaderMap,
    ) -> Result<downloads_server::UploadFileResponse, Self::Error> {
        let nbytes = stream_body_to_file(req.body, &state.upload_dest)
            .await
            .expect("upload destination writable");
        Ok(downloads_server::UploadFileResponse::Status201(
            serde_json::json!({ "id": format!("uploaded-{nbytes}") }),
        ))
    }
}

async fn spawn_server(upload_dest: PathBuf, download_source: PathBuf) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let state = AppState {
        upload_dest: Arc::new(upload_dest),
        download_source: Arc::new(download_source),
    };

    tokio::spawn(async move {
        axum::serve(listener, MockDownloads.router().with_state(state))
            .await
            .unwrap();
    });

    format!("http://{addr}")
}

#[tokio::test]
async fn axum_download_streams_from_disk() {
    let dir = std::env::temp_dir().join("openapi-trait-downloads-axum");
    let _ = tokio::fs::remove_dir_all(&dir).await;
    tokio::fs::create_dir_all(&dir).await.unwrap();
    let download_source = dir.join("source.bin");
    tokio::fs::write(&download_source, b"hello-download")
        .await
        .unwrap();
    let upload_dest = dir.join("upload.bin");

    let base_url = spawn_server(upload_dest, download_source).await;

    let response = reqwest::Client::new()
        .get(format!("{base_url}/files/found"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("application/octet-stream")
    );
    assert_eq!(
        response
            .headers()
            .get("content-disposition")
            .and_then(|v| v.to_str().ok()),
        Some("attachment; filename=\"report.pdf\"")
    );
    assert_eq!(
        response.headers().get("etag").and_then(|v| v.to_str().ok()),
        Some("\"abc123\"")
    );
    assert_eq!(response.bytes().await.unwrap().as_ref(), b"hello-download");
}

#[cfg(feature = "reqwest-client")]
mod client_tests {
    use super::*;
    use downloads::DownloadsClient as _;
    use futures_util::StreamExt;

    #[derive(Clone, openapi_trait::ReqwestClient)]
    struct DerivedDownloadsClient {
        #[openapi_trait(client)]
        http: ::reqwest::Client,
        #[openapi_trait(base_url)]
        endpoint: String,
    }

    async fn stream_to_file(
        mut stream: openapi_trait::ByteStream,
        path: &Path,
    ) -> std::io::Result<()> {
        let mut file = tokio::fs::File::create(path).await?;
        while let Some(chunk) = stream.next().await {
            file.write_all(&chunk.map_err(std::io::Error::other)?)
                .await?;
        }
        file.flush().await
    }

    #[tokio::test]
    async fn reqwest_client_download_streams_to_disk() {
        let dir = std::env::temp_dir().join("openapi-trait-downloads-client-dl");
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let download_source = dir.join("source.bin");
        let upload_dest = dir.join("upload.bin");
        let output = dir.join("received.bin");
        tokio::fs::write(&download_source, b"hello-download")
            .await
            .unwrap();

        let base_url = spawn_server(upload_dest, download_source).await;
        let client = DerivedDownloadsClient {
            http: openapi_trait::reqwest::Client::new(),
            endpoint: base_url,
        };

        let response = client
            .download_file(
                downloads::DownloadFileRequest {
                    id: "found".to_string(),
                },
                None,
            )
            .await
            .unwrap();

        match response {
            downloads::DownloadFileResponse::Status200(payload) => {
                stream_to_file(payload.body, &output)
                    .await
                    .expect("write streamed download");
                assert_eq!(tokio::fs::read(&output).await.unwrap(), b"hello-download");
                assert_eq!(
                    payload.content_disposition.as_deref(),
                    Some("attachment; filename=\"report.pdf\"")
                );
                assert_eq!(payload.e_tag.as_deref(), Some("\"abc123\""));
            }
            downloads::DownloadFileResponse::Status404 => panic!("expected 200 response"),
        }
    }

    #[tokio::test]
    async fn reqwest_client_upload_streams_from_disk() {
        let dir = std::env::temp_dir().join("openapi-trait-downloads-client-ul");
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let upload_source = dir.join("upload-source.bin");
        let upload_dest = dir.join("upload-dest.bin");
        let download_source = dir.join("download-source.bin");
        tokio::fs::write(&upload_source, b"raw-upload-bytes")
            .await
            .unwrap();

        let base_url = spawn_server(upload_dest.clone(), download_source).await;
        let client = DerivedDownloadsClient {
            http: openapi_trait::reqwest::Client::new(),
            endpoint: base_url,
        };

        let file = tokio::fs::File::open(&upload_source).await.unwrap();
        let body =
            openapi_trait::reqwest::Body::wrap_stream(tokio_util::io::ReaderStream::new(file));

        let downloads::UploadFileResponse::Status201(resp) = client
            .upload_file(downloads::UploadFileRequest { body }, None)
            .await
            .unwrap();

        assert_eq!(resp["id"], "uploaded-16");
        assert_eq!(
            tokio::fs::read(&upload_dest).await.unwrap(),
            b"raw-upload-bytes"
        );
    }
}
