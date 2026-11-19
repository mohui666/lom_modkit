use crate::{
    i18n::{key, message},
    shell,
};
use eframe::egui;
use reqwest::{StatusCode, Url};
use serde::{Deserialize, Serialize};
use std::{sync::mpsc, time::Duration};

const MAX_DESCRIPTION: usize = 2000;
const MAX_RECEIPT_BYTES: usize = 256;
const PRODUCTION_ENDPOINT: &str = "https://toolbox.mahui.site/api/v1/feedback";

// Automatic diagnostics use a fixed set of fields. Full errors and project
// identifiers stay in the local diagnostic tools.
#[derive(Clone, Serialize)]
pub(crate) struct Context {
    pub(crate) editor_version: &'static str,
    pub(crate) host_version: Option<String>,
    pub(crate) game_build: Option<u64>,
    pub(crate) os: &'static str,
    pub(crate) architecture: &'static str,
    pub(crate) locale: &'static str,
    pub(crate) page: &'static str,
    pub(crate) project_mode: Option<&'static str>,
    pub(crate) step_type: Option<String>,
    pub(crate) error_kind: Option<&'static str>,
    pub(crate) os_error: Option<u32>,
}

#[derive(Serialize)]
struct Report {
    schema: u8,
    id: String,
    description: String,
    context: Context,
}

#[derive(Deserialize)]
struct Receipt {
    id: String,
    number: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Failure {
    Connection,
    Rejected,
    Busy,
    Receipt,
}

impl Failure {
    fn key(self) -> &'static str {
        match self {
            Self::Connection => "feedback.connection_error",
            Self::Rejected => "feedback.rejected",
            Self::Busy => "feedback.busy",
            Self::Receipt => "feedback.invalid_receipt",
        }
    }
}

pub(crate) struct Dialog {
    context: Context,
    endpoint: Option<Url>,
    description: String,
    submission: Option<(String, String)>,
    worker: Option<mpsc::Receiver<Result<u64, Failure>>>,
    failure: Option<Failure>,
    receipt: Option<u64>,
}

impl Dialog {
    pub(crate) fn new(context: Context) -> Self {
        Self {
            context,
            endpoint: https_endpoint(option_env!("LOM_FEEDBACK_URL").unwrap_or(PRODUCTION_ENDPOINT)),
            description: String::new(),
            submission: None,
            worker: None,
            failure: None,
            receipt: None,
        }
    }

    pub(crate) fn show(&mut self, ctx: &egui::Context) -> bool {
        self.poll();
        let sending = self.worker.is_some();
        let response = egui::Modal::new(egui::Id::new("feedback"))
            .frame(shell::dialog_frame_for(ctx))
            .show(ctx, |ui| {
                ui.set_width((ctx.content_rect().width() - 48.0).clamp(120.0, 470.0));
                let close = shell::dialog_header(ui, &key("feedback.title"));
                if let Some(receipt) = &self.receipt {
                    let receipt = format!("#{receipt}");
                    ui.label(message("feedback.sent", &[("id", receipt.clone())]));
                    if ui.button(key("feedback.copy_id")).clicked() {
                        ctx.copy_text(receipt);
                    }
                    return close;
                }
                if let Some(endpoint) = &self.endpoint {
                    ui.label(message(
                        "feedback.destination",
                        &[
                            ("version", self.context.editor_version.to_owned()),
                            ("server", endpoint.host_str().unwrap_or_default().to_owned()),
                        ],
                    ));
                } else {
                    ui.label(key("feedback.unconfigured"));
                }
                ui.add_enabled(
                    !sending,
                    egui::TextEdit::multiline(&mut self.description)
                        .desired_rows(4)
                        .desired_width(f32::INFINITY)
                        .char_limit(MAX_DESCRIPTION)
                        .hint_text(key("feedback.description")),
                );
                ui.small(key("feedback.scope"));
                ui.collapsing(key("feedback.details"), |ui| {
                    let json = serde_json::to_string_pretty(&self.context).unwrap_or_default();
                    egui::ScrollArea::vertical()
                        .max_height(170.0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(egui::RichText::new(json).monospace())
                                    .selectable(true),
                            );
                        });
                });
                if let Some(failure) = self.failure {
                    ui.label(key(failure.key()));
                }
                shell::dialog_buttons(ui, |ui| {
                    if sending {
                        ui.spinner();
                        ui.label(key("feedback.sending"));
                    } else if ui
                        .add_enabled(
                            self.endpoint.is_some()
                                && (self.context.error_kind.is_some()
                                    || !self.description.trim().is_empty()),
                            egui::Button::new(key("feedback.send")),
                        )
                        .clicked()
                    {
                        self.send(ctx);
                    }
                });
                close
            });
        self.worker.is_none() && (response.inner || response.should_close())
    }

    fn send(&mut self, ctx: &egui::Context) {
        let Some(endpoint) = self.endpoint.clone() else {
            return;
        };
        let description = self.description.trim().to_owned();
        // An uncertain response can be retried with the same ID as long as the
        // draft is unchanged. Editing it creates a new report.
        if self
            .submission
            .as_ref()
            .is_none_or(|(note, _)| note != &description)
        {
            self.submission = Some((
                description.clone(),
                uuid::Uuid::new_v4().simple().to_string(),
            ));
        }
        let id = self.submission.as_ref().unwrap().1.clone();
        let payload = Report {
            schema: 1,
            id,
            description,
            context: self.context.clone(),
        };
        let (sender, receiver) = mpsc::channel();
        let ctx = ctx.clone();
        self.failure = None;
        self.worker = Some(receiver);
        std::thread::spawn(move || {
            let result = submit(endpoint, payload);
            let _ = sender.send(result);
            ctx.request_repaint();
        });
    }

    fn poll(&mut self) {
        let Some(worker) = &self.worker else { return };
        match worker.try_recv() {
            Ok(Ok(receipt)) => self.receipt = Some(receipt),
            Ok(Err(failure)) => self.failure = Some(failure),
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => self.failure = Some(Failure::Connection),
        }
        self.worker = None;
    }
}

fn https_endpoint(value: &str) -> Option<Url> {
    let url = Url::parse(value).ok()?;
    (url.scheme() == "https"
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none())
    .then_some(url)
}

fn submit(endpoint: Url, payload: Report) -> Result<u64, Failure> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| Failure::Connection)?;
    let result = runtime.block_on(async {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|_| Failure::Connection)?;
        let body = serde_json::to_vec(&payload).map_err(|_| Failure::Rejected)?;
        let mut response = client
            .post(endpoint)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await
            .map_err(|_| Failure::Connection)?;
        match response.status() {
            StatusCode::OK | StatusCode::CREATED => {}
            StatusCode::TOO_MANY_REQUESTS | StatusCode::SERVICE_UNAVAILABLE => {
                return Err(Failure::Busy);
            }
            _ => return Err(Failure::Rejected),
        }
        let mut receipt = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Failure::Connection)? {
            if receipt.len() + chunk.len() > MAX_RECEIPT_BYTES {
                return Err(Failure::Receipt);
            }
            receipt.extend_from_slice(&chunk);
        }
        let receipt: Receipt = serde_json::from_slice(&receipt).map_err(|_| Failure::Receipt)?;
        if receipt.id != payload.id || receipt.number == 0 {
            return Err(Failure::Receipt);
        }
        Ok(receipt.number)
    });
    runtime.shutdown_timeout(Duration::from_millis(100));
    result
}
