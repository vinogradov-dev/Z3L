//! The endpoint's model list, on its way to the settings page.
//!
//! The page cannot ask: HTTP lives in `mcp` and `ui` may not depend on it, so the *List models*
//! button reports the address it wants and this module does the asking (ADR 0235). One request is in
//! flight at a time and it never touches the frame — the worker sends its answer through a channel
//! and [`Models::poll`] takes it when it is there, exactly as a completion does (ADR 0187).
//!
//! What the page draws is [`ListedModels`], which is the last answer and the address it is about. It
//! is view state: nothing here is written to the settings file, and a listing is thrown away when
//! another address is asked for.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use ui::settings::{ListedModels, ModelRow};

/// What one listing's worker sends back: the rows, or the sentence the wire refused with.
type Listed = Result<Vec<ModelRow>, String>;

/// The listing in flight, and the last answer the page may draw.
#[derive(Default)]
pub(crate) struct Models {
    /// The address being asked and the worker's channel, while one is in flight.
    job: Option<(String, Receiver<Listed>)>,
    /// What the page draws: the last answer, and the address it belongs to.
    listed: ListedModels,
}

impl Models {
    /// Asks `address` what it answers, unless it is already being asked.
    ///
    /// The address is trimmed the way the field is read everywhere else, so a trailing slash is the
    /// same endpoint and does not start a second request. A thread that cannot be spawned answers the
    /// sentence a person can act on now, because a button that silently does nothing is worse than a
    /// listing that failed.
    pub(crate) fn request(&mut self, address: &str, folder: Option<PathBuf>) {
        let address = address.trim().trim_end_matches('/').to_owned();
        if address.is_empty() || self.job.as_ref().is_some_and(|(asking, _)| asking == &address) {
            return;
        }
        let (sender, answer) = mpsc::channel();
        let asked = address.clone();
        let spawned = std::thread::Builder::new().name("model-list".to_owned()).spawn(move || {
            // A send fails only when the page was closed and this job dropped with it; the answer is
            // then nobody's, which is what closing the window asked for.
            let _ = sender.send(list(&asked, folder.as_deref()));
        });
        match spawned {
            Ok(_) => {
                self.listed = ListedModels { address: address.clone(), listing: true, ..ListedModels::default() };
                self.job = Some((address, answer));
            }
            Err(error) => {
                self.job = None;
                self.listed = ListedModels {
                    address,
                    failed: Some(format!("the request cannot be started: {error}")),
                    ..ListedModels::default()
                };
            }
        }
    }

    /// Takes the worker's answer, if one arrived since the last frame.
    pub(crate) fn poll(&mut self) {
        let Some((address, answer)) = self.job.as_ref() else {
            return;
        };
        let address = address.clone();
        let answer = match answer.try_recv() {
            Ok(answer) => answer,
            Err(TryRecvError::Empty) => return,
            // A worker that died without answering is the one case the channel cannot describe; the
            // page says so instead of drawing a listing that never ends.
            Err(TryRecvError::Disconnected) => Err("the request ended without an answer".to_owned()),
        };
        self.job = None;
        let (rows, failed) = match answer {
            Ok(rows) => (rows, None),
            Err(sentence) => (Vec::new(), Some(sentence)),
        };
        self.listed = ListedModels { address, listing: false, rows, failed };
    }

    /// What the page draws.
    pub(crate) fn listed(&self) -> &ListedModels {
        &self.listed
    }

    /// Hands the page an answer, so a frame test draws a listing without a wire.
    #[cfg(test)]
    pub(crate) fn land(&mut self, listed: ListedModels) {
        self.listed = listed;
    }
}

/// One listing, on the worker's thread.
///
/// The answer is the page's shape, not the wire's: `ui` names what it draws and this is where the two
/// meet, so a model list is a value the page can hold without knowing what HTTP is.
fn list(address: &str, folder: Option<&std::path::Path>) -> Listed {
    match mcp::models(address, folder) {
        Ok(models) => Ok(models.into_iter().map(|model| ModelRow { id: model.id, name: model.name }).collect()),
        Err(error) => Err(error.sentence().to_owned()),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::time::{Duration, Instant};

    use super::*;

    /// Points the module at a loopback answer and waits for the worker the way a frame does.
    fn listed(address: &str) -> ListedModels {
        let (url, _requests) = testkit::stub_endpoint(serde_json::json!({
            "object": "list",
            "data": [{ "id": "stub-model", "name": "Stub Model" }]
        }));
        let mut models = Models::default();
        models.request(&format!("{url}/"), None);
        assert!(models.listed().listing, "a request in flight says so at once");
        let deadline = Instant::now() + Duration::from_secs(10);
        while models.listed().listing && Instant::now() < deadline {
            models.poll();
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(!models.listed().listing, "the worker answered");
        let _ = address;
        models.listed().clone()
    }

    /// The address the preset writes is the address the wire routes on: a Go gateway is known by its
    /// address and needs the session header, and no person types one (X124, ADR 0235).
    ///
    /// This is the one test that can hold both halves, because `ui` may not see `mcp` and `mcp` may
    /// not see `ui`: the shell sees both, so the agreement between the page's table and the wire's
    /// predicate is asserted here.
    #[test]
    fn the_go_preset_is_the_address_the_wire_routes_on() {
        let go = ui::settings::PRESETS
            .iter()
            .find(|preset| preset.name == text::Message::AssistantProviderOpenCodeGo)
            .expect("the page offers the Go tier");
        assert!(mcp::go_gateway(go.base_url), "the preset is the gateway the wire recognises: {}", go.base_url);
        assert_eq!(
            mcp::Endpoint::new(go.base_url, "deepseek-v4.1-flash").chat_url(),
            "https://opencode.ai/inference/go/v1/chat/completions",
            "one completion, at the family the address names"
        );
        let plain = ui::settings::PRESETS
            .iter()
            .find(|preset| preset.name == text::Message::AssistantProviderOpenCode)
            .expect("the page offers the plain tier too");
        assert!(!mcp::go_gateway(plain.base_url), "the plain tier is another address: {}", plain.base_url);
        for preset in ui::settings::PRESETS {
            assert!(
                mcp::Endpoint::new(preset.base_url, "model").chat_url().ends_with("/chat/completions"),
                "every preset is the one OpenAI-compatible wire: {}",
                preset.base_url
            );
        }
    }

    #[test]
    fn a_listing_lands_in_the_page_shape() {
        let listed = listed("http://127.0.0.1:1/v1");
        assert_eq!(listed.rows, [ModelRow { id: "stub-model".to_owned(), name: "Stub Model".to_owned() }]);
        assert_eq!(listed.failed, None, "a good answer leaves nothing to explain");
        assert!(listed.address.starts_with("http://127.0.0.1:"), "the address is kept for the page: {listed:?}");
        assert!(!listed.address.ends_with('/'), "the field's trailing slash is not a second address");
    }

    #[test]
    fn the_same_address_twice_is_one_request() {
        let (url, requests) = testkit::stub_endpoint(serde_json::json!({ "data": [] }));
        let mut models = Models::default();
        models.request(&url, None);
        models.request(&url, None);
        models.request(&format!("{url}/"), None);
        let deadline = Instant::now() + Duration::from_secs(10);
        while models.listed().listing && Instant::now() < deadline {
            models.poll();
            std::thread::sleep(Duration::from_millis(2));
        }
        // The stub answers one connection and then goes quiet, so a second request would leave the
        // job in flight for ever; the answer arriving is the proof that only one was made.
        assert!(!models.listed().listing, "the endpoint answered");
        assert!(models.job.is_none(), "one request, one answer");
        assert!(requests.recv_timeout(Duration::from_secs(5)).is_ok(), "the endpoint was asked");
    }

    #[test]
    fn a_refusal_reaches_the_page_as_the_wires_own_sentence() {
        let (url, _requests) = testkit::stub_endpoint_status(401, r#"{"error":{"message":"Invalid credential"}}"#);
        let mut models = Models::default();
        models.request(&url, None);
        let deadline = Instant::now() + Duration::from_secs(10);
        while models.listed().failed.is_none() && Instant::now() < deadline {
            models.poll();
            std::thread::sleep(Duration::from_millis(2));
        }
        let failed = models.listed().failed.clone().expect("the refusal is kept for the page");
        assert!(failed.contains("401") && failed.contains("Invalid credential"), "{failed}");
    }
}
