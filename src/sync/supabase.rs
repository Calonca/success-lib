//! [`RemoteStore`] implementation over Supabase's PostgREST API.
//!
//! Expects a `documents` table as created by `docs/supabase-setup.sql`:
//! `(archive_id text, path text, content text, revision bigint, updated_at
//! timestamptz, primary key (archive_id, path))`. Optimistic concurrency is
//! enforced by filtering updates on the expected `revision` and checking
//! whether a row was actually affected.

use serde::Deserialize;
use serde_json::json;

use crate::sync::remote::{RemoteDoc, RemoteEntry, RemoteStore, SyncError};

pub struct SupabaseStore {
    base_url: String,
    api_key: String,
    archive_id: String,
    client: reqwest::Client,
}

#[derive(Deserialize)]
struct EntryRow {
    path: String,
    revision: i64,
}

#[derive(Deserialize)]
struct DocRow {
    content: String,
    revision: i64,
}

#[derive(Deserialize)]
struct RevisionRow {
    revision: i64,
}

impl SupabaseStore {
    /// `base_url` is the project URL, e.g. `https://abc123.supabase.co`.
    pub fn new(base_url: &str, api_key: &str, archive_id: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            archive_id: archive_id.to_string(),
            client: reqwest::Client::new(),
        }
    }

    fn endpoint(&self) -> String {
        format!("{}/rest/v1/documents", self.base_url)
    }

    fn archive_filter(&self) -> (String, String) {
        ("archive_id".into(), format!("eq.{}", self.archive_id))
    }

    fn path_filter(path: &str) -> (String, String) {
        ("path".into(), format!("eq.{path}"))
    }

    fn request(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        builder
            .header("apikey", &self.api_key)
            .header("Authorization", format!("Bearer {}", self.api_key))
    }

    async fn send(
        &self,
        builder: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, SyncError> {
        let response = self
            .request(builder)
            .send()
            .await
            .map_err(|e| SyncError::Network {
                detail: e.to_string(),
            })?;
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        if status.as_u16() == 409 {
            return Err(SyncError::Conflict);
        }
        Err(SyncError::Http {
            status: status.as_u16(),
            detail: response.text().await.unwrap_or_default(),
        })
    }
}

async fn decode<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, SyncError> {
    response.json().await.map_err(|e| SyncError::Protocol {
        detail: e.to_string(),
    })
}

impl RemoteStore for SupabaseStore {
    async fn list(&self) -> Result<Vec<RemoteEntry>, SyncError> {
        let builder = self
            .client
            .get(self.endpoint())
            .query(&[("select".to_string(), "path,revision".to_string()), self.archive_filter()]);
        let rows: Vec<EntryRow> = decode(self.send(builder).await?).await?;
        Ok(rows
            .into_iter()
            .map(|r| RemoteEntry {
                path: r.path,
                revision: r.revision,
            })
            .collect())
    }

    async fn get(&self, path: &str) -> Result<Option<RemoteDoc>, SyncError> {
        let builder = self.client.get(self.endpoint()).query(&[
            ("select".to_string(), "content,revision".to_string()),
            self.archive_filter(),
            Self::path_filter(path),
        ]);
        let rows: Vec<DocRow> = decode(self.send(builder).await?).await?;
        Ok(rows.into_iter().next().map(|r| RemoteDoc {
            content: r.content,
            revision: r.revision,
        }))
    }

    async fn put(
        &self,
        path: &str,
        content: &str,
        base_revision: Option<i64>,
    ) -> Result<i64, SyncError> {
        match base_revision {
            None => {
                // New document; a duplicate key answers 409 → Conflict.
                let builder = self
                    .client
                    .post(self.endpoint())
                    .json(&json!({
                        "archive_id": self.archive_id,
                        "path": path,
                        "content": content,
                        "revision": 1,
                    }));
                self.send(builder).await?;
                Ok(1)
            }
            Some(base) => {
                let next = base + 1;
                let builder = self
                    .client
                    .patch(self.endpoint())
                    .query(&[
                        self.archive_filter(),
                        Self::path_filter(path),
                        ("revision".to_string(), format!("eq.{base}")),
                    ])
                    .header("Prefer", "return=representation")
                    .json(&json!({ "content": content, "revision": next }));
                let rows: Vec<RevisionRow> = decode(self.send(builder).await?).await?;
                match rows.first() {
                    // No row matched the expected revision: someone else won.
                    None => Err(SyncError::Conflict),
                    Some(row) => Ok(row.revision),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> SupabaseStore {
        SupabaseStore::new(
            "https://abc123.supabase.co/",
            "anon-key",
            "my archive",
        )
    }

    #[test]
    fn endpoint_trims_trailing_slash() {
        assert_eq!(
            store().endpoint(),
            "https://abc123.supabase.co/rest/v1/documents"
        );
    }

    #[test]
    fn list_request_url_and_headers() {
        let s = store();
        let request = s
            .request(s.client.get(s.endpoint()).query(&[
                ("select".to_string(), "path,revision".to_string()),
                s.archive_filter(),
            ]))
            .build()
            .unwrap();
        assert_eq!(
            request.url().as_str(),
            "https://abc123.supabase.co/rest/v1/documents?select=path%2Crevision&archive_id=eq.my+archive"
        );
        assert_eq!(request.headers()["apikey"], "anon-key");
        assert_eq!(request.headers()["Authorization"], "Bearer anon-key");
    }

    #[test]
    fn path_filter_encodes_slashes_in_query() {
        let s = store();
        let request = s
            .client
            .get(s.endpoint())
            .query(&[SupabaseStore::path_filter("graphs/2026-08-09.mmd")])
            .build()
            .unwrap();
        assert_eq!(
            request.url().query(),
            Some("path=eq.graphs%2F2026-08-09.mmd")
        );
    }
}
