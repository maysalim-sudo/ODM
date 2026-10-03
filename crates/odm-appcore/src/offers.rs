//! Reuse offers (spec 5.5): a matching paused or failed download is offered back to the user.
use odm_core::{DownloadState, OdmError};
use odm_engine::{AddRequest, ManagerHandle};
use odm_store::DownloadRecord;
use serde::Serialize;
use std::sync::{Mutex, MutexGuard};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OfferDto {
    pub offer_id: String,
    pub existing_id: String,
    pub filename: String,
}

/// Pending offers, oldest first.
#[derive(Default)]
pub struct Offers {
    items: Mutex<Vec<(OfferDto, AddRequest)>>,
}

impl Offers {
    fn items(&self) -> MutexGuard<'_, Vec<(OfferDto, AddRequest)>> {
        self.items.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn create(&self, existing_id: String, filename: String, req: AddRequest) -> OfferDto {
        let dto = OfferDto {
            offer_id: uuid::Uuid::new_v4().simple().to_string(),
            existing_id,
            filename,
        };
        self.items().push((dto.clone(), req));
        dto
    }

    pub fn list(&self) -> Vec<OfferDto> {
        self.items().iter().map(|(d, _)| d.clone()).collect()
    }

    pub fn take(&self, offer_id: &str) -> Option<(OfferDto, AddRequest)> {
        let mut items = self.items();
        let pos = items.iter().position(|(d, _)| d.offer_id == offer_id)?;
        Some(items.remove(pos))
    }
}

/// The newest Paused or Failed file download whose filename equals `name` (ASCII case-insensitive).
pub async fn find_reusable(
    m: &ManagerHandle,
    name: &str,
) -> Result<Option<DownloadRecord>, OdmError> {
    // `list` is newest first.
    Ok(m.list().await?.into_iter().map(|(r, _)| r).find(|r| {
        r.kind == "file"
            && matches!(r.state, DownloadState::Paused | DownloadState::Failed)
            && r.filename.eq_ignore_ascii_case(name)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use odm_engine::EngineConfig;
    use odm_store::Store;
    use odm_testserver::{Faults, TestServer};
    use std::time::{Duration, Instant};

    #[test]
    fn offers_create_list_take() {
        let o = Offers::default();
        let a = o.create("1".into(), "a.zip".into(), AddRequest::new("http://x/a"));
        let b = o.create("2".into(), "b.zip".into(), AddRequest::new("http://x/b"));
        assert_eq!(o.list(), vec![a.clone(), b]);
        let (dto, req) = o.take(&a.offer_id).unwrap();
        assert_eq!((dto, req.url.as_str()), (a.clone(), "http://x/a"));
        assert!(o.take(&a.offer_id).is_none());
        assert_eq!(o.list().len(), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn find_reusable_picks_newest_paused_or_failed() {
        let d = tempfile::tempdir().unwrap();
        let s = TestServer::start(1 << 20, Faults::default()).await;
        let cfg = EngineConfig {
            download_dir: d.path().to_path_buf(),
            ..Default::default()
        };
        let m = ManagerHandle::start(cfg, Store::open_in_memory().unwrap())
            .await
            .unwrap();
        let paused = |name: &str| AddRequest {
            filename: Some(name.into()),
            start_paused: true,
            ..AddRequest::new(s.url("/file"))
        };
        let _old = m.add(paused("a.zip")).await.unwrap();
        let newer = m.add(paused("a.zip")).await.unwrap();
        let done = m
            .add(AddRequest {
                filename: Some("a.zip".into()),
                ..AddRequest::new(s.url("/file"))
            })
            .await
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let (r, _) = m.get(&done).await.unwrap().unwrap();
            if r.state == DownloadState::Completed {
                break;
            }
            assert!(Instant::now() < deadline, "timed out in {:?}", r.state);
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let got = find_reusable(&m, "A.ZIP").await.unwrap().unwrap();
        assert_eq!(got.id, newer);
        assert!(find_reusable(&m, "b.zip").await.unwrap().is_none());
    }
}
