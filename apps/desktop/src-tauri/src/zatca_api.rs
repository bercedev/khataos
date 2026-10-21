//! ZATCA's e-invoicing APIs (Fatoora): onboarding (compliance and
//! production CSIDs), compliance checks, reporting and clearance.
//!
//! Source: ZATCA's own API specifications served by the developer portal,
//! "e-Invoicing Sandbox Release (2.1.0)" (Compliance CSID, Production CSID,
//! Compliance Invoice, Reporting, Clearance, Renewal), fetched 20 Oct 2026
//! from `gw-fatoora.zatca.gov.sa/e-invoicing/developer-portal/users/api/v1/swagger/render`
//! [PRIMARY]. Every call:
//! - sends `Accept-Version: V2` (anything else is refused with 406) and
//!   `Accept-Language: en`;
//! - `POST /compliance`: header `OTP`, body `{"csr": <Base64 of the CSR
//!   PEM>}`, no authentication; answers `requestID`, `dispositionMessage`,
//!   `binarySecurityToken` (Base64 of the certificate's Base64 text) and
//!   `secret`;
//! - the others authenticate with HTTP Basic, user = `binarySecurityToken`,
//!   password = `secret` of the CSID;
//! - `POST /compliance/invoices` (compliance CSID), `POST
//!   /invoices/reporting/single` (simplified documents, `Clearance-Status:
//!   0`) and `POST /invoices/clearance/single` (standard documents,
//!   `Clearance-Status: 1`) take `{"invoiceHash", "uuid", "invoice":
//!   <Base64 of the XML>}` and answer `validationResults` (info, warning
//!   and error messages, status PASS/WARNING/ERROR) with `reportingStatus`
//!   or `clearanceStatus` and, when cleared, `clearedInvoice` (Base64 XML);
//!   200 accepted, 202 accepted with warnings, 400 refused, 409 already
//!   reported, 208 already cleared, 303 clearance switched off (report
//!   instead);
//! - `POST /production/csids` (compliance CSID) takes
//!   `{"compliance_request_id"}` and answers like `/compliance`;
//! - `PATCH /production/csids` (current production CSID, header `OTP`)
//!   takes `{"csr"}` and renews the production CSID
//!   ([`Client::renew_production_csid`]; the verb is [UNCERTAIN]).
//!
//! **Failures** carry a [`CallFailure`]: transient ones (no connection,
//! 429, 5xx) are retried by the reporting queue in `zatca.rs`; a 401 means
//! the CSID expired or was revoked.
//!
//! **Secrets.** The OTP, the CSID tokens and secrets travel only in request
//! headers and are never written to logs, errors or the audit trail;
//! [`Csid`]'s `Debug` hides them. Errors carry ZATCA's own messages, which
//! never echo them.

use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use khata_sa::csr::Environment;
use serde::{Deserialize, Serialize};

use crate::api::ApiResult;

/// The developer portal (Integration Sandbox): the server named by every
/// API specification above [PRIMARY]. Anyone can onboard here with the
/// test OTP "123345" (checked live 20 Oct 2026).
pub const SANDBOX_URL: &str = "https://gw-fatoora.zatca.gov.sa/e-invoicing/developer-portal";
/// The simulation environment. [SECONDARY: ZATCA developer community
/// forum and integrators' guides; the server answered our CSR with an OTP
/// check, live 20 Oct 2026.]
pub const SIMULATION_URL: &str = "https://gw-fatoora.zatca.gov.sa/e-invoicing/simulation";
/// Production ("core"). [PRIMARY: Fatoora Portal User Manual; not called
/// by KhataOS's tests.]
pub const PRODUCTION_URL: &str = "https://gw-fatoora.zatca.gov.sa/e-invoicing/core";

pub fn base_url(env: Environment) -> &'static str {
    match env {
        Environment::Sandbox => SANDBOX_URL,
        Environment::Simulation => SIMULATION_URL,
        Environment::Production => PRODUCTION_URL,
    }
}

/// A CSID: the certificate token, its API secret and the request id.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Csid {
    /// `binarySecurityToken`: Base64 of the certificate's Base64 text.
    pub token: String,
    pub secret: String,
    pub request_id: String,
}

impl std::fmt::Debug for Csid {
    /// Never prints the token or the secret.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Csid").field("request_id", &self.request_id).finish_non_exhaustive()
    }
}

impl Csid {
    fn basic(&self) -> String {
        format!("Basic {}", STANDARD.encode(format!("{}:{}", self.token, self.secret)))
    }
}

/// One message of ZATCA's validation results.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Message {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub code: Option<String>,
    pub category: Option<String>,
    pub message: Option<String>,
    pub status: Option<String>,
}

/// What ZATCA said about a document.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outcome {
    pub http_status: u16,
    /// PASS, WARNING or ERROR.
    pub status: String,
    /// REPORTED / NOT_REPORTED.
    pub reporting_status: Option<String>,
    /// CLEARED / NOT_CLEARED.
    pub clearance_status: Option<String>,
    pub info: Vec<Message>,
    pub warnings: Vec<Message>,
    pub errors: Vec<Message>,
    /// The cleared XML ZATCA stamped, for a cleared standard document.
    pub cleared_xml: Option<String>,
    /// A plain message (303 "clearance is off", 409 "already reported"…).
    pub note: Option<String>,
}

impl Outcome {
    /// ZATCA took the document (with or without warnings).
    pub fn accepted(&self) -> bool {
        matches!(self.reporting_status.as_deref(), Some("REPORTED"))
            || matches!(self.clearance_status.as_deref(), Some("CLEARED"))
            || (self.reporting_status.is_none() && self.clearance_status.is_none() && self.status != "ERROR" && (200..300).contains(&self.http_status))
    }
}

/// A signed document for the APIs.
#[derive(Debug, Clone, Copy)]
pub struct Document<'a> {
    pub xml: &'a str,
    /// The invoice hash (Base64).
    pub hash: &'a str,
    pub uuid: &'a str,
}

/// Why a document call failed, so a queue knows whether to retry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CallFailure {
    /// No connection, a timeout, or ZATCA busy or down (429, 5xx): send
    /// again later. The Detailed Guidelines (v2, pp.45-49) list 429, 500,
    /// 503 and 504 as "please resend".
    Transient,
    /// 401: the CSID or its secret was refused (expired or revoked).
    Credentials,
    /// Anything else: retrying the same request will not help.
    Other,
}

/// A failed call: what kind, and a message without any secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallError {
    pub kind: CallFailure,
    pub message: String,
}

impl CallError {
    fn new(kind: CallFailure, message: impl Into<String>) -> Self {
        Self { kind, message: message.into() }
    }

    pub fn transient(&self) -> bool {
        self.kind == CallFailure::Transient
    }
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl From<CallError> for String {
    fn from(e: CallError) -> Self {
        e.message
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Results {
    #[serde(rename = "infoMessages", deserialize_with = "messages")]
    info: Vec<Message>,
    #[serde(rename = "warningMessages", deserialize_with = "messages")]
    warnings: Vec<Message>,
    #[serde(rename = "errorMessages", deserialize_with = "messages")]
    errors: Vec<Message>,
    status: Option<String>,
}

/// ZATCA sends a list, a single object or null.
fn messages<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Message>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        Many(Vec<Message>),
        One(Message),
    }
    Ok(match Option::<OneOrMany>::deserialize(d)? {
        Some(OneOrMany::Many(v)) => v,
        Some(OneOrMany::One(m)) => vec![m],
        None => vec![],
    })
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct InvoiceReply {
    #[serde(rename = "validationResults")]
    validation: Option<Results>,
    #[serde(rename = "reportingStatus")]
    reporting_status: Option<String>,
    #[serde(rename = "clearanceStatus")]
    clearance_status: Option<String>,
    #[serde(rename = "clearedInvoice")]
    cleared_invoice: Option<String>,
    message: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct CsidReply {
    #[serde(rename = "requestID")]
    request_id: Option<serde_json::Value>,
    #[serde(rename = "dispositionMessage")]
    disposition: Option<String>,
    #[serde(rename = "binarySecurityToken")]
    token: Option<String>,
    secret: Option<String>,
    errors: Option<serde_json::Value>,
    #[serde(rename = "errorCategory")]
    error_category: Option<String>,
    #[serde(rename = "errorMessage")]
    error_message: Option<String>,
    message: Option<String>,
}

/// A client for one environment.
pub struct Client {
    base: String,
    agent: ureq::Agent,
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        // A 303 from clearance means "report instead": don't follow it.
        .max_redirects(0)
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_global(Some(Duration::from_secs(90)))
        .build()
        .into()
}

fn net(e: ureq::Error) -> String {
    format!("cannot reach ZATCA ({e}); check the internet connection and try again")
}

/// The text of ZATCA's CSID errors, without anything we sent.
fn csid_error(status: u16, r: &CsidReply) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(errors) = r.errors.as_ref().and_then(|e| e.as_array()) {
        for e in errors {
            let code = e.get("code").and_then(|c| c.as_str()).unwrap_or_default();
            let msg = e.get("message").and_then(|c| c.as_str()).unwrap_or_default();
            parts.push(format!("{code}: {msg}").trim_matches([' ', ':']).to_owned());
        }
    }
    if let Some(m) = r.error_message.as_ref().or(r.message.as_ref()) {
        parts.push(format!("{}{m}", r.error_category.as_ref().map(|c| format!("{c}: ")).unwrap_or_default()));
    }
    let detail = if parts.is_empty() { String::new() } else { format!(": {}", parts.join("; ")) };
    match status {
        401 => "ZATCA refused the credentials (401): the CSID or its secret is wrong or expired".into(),
        406 => "ZATCA refused the API version (406)".into(),
        _ => format!("ZATCA refused the request ({status}){detail}"),
    }
}

impl Client {
    pub fn new(env: Environment) -> Self {
        Self::with_base(base_url(env))
    }

    /// For tests against a local mock server.
    pub fn with_base(base: &str) -> Self {
        Self { base: base.trim_end_matches('/').to_owned(), agent: agent() }
    }

    fn csid_call(&self, req: ureq::RequestBuilder<ureq::typestate::WithBody>, body: serde_json::Value) -> ApiResult<Csid> {
        let mut resp = req
            .header("Accept", "application/json")
            .header("Accept-Version", "V2")
            .header("Accept-Language", "en")
            .send_json(body)
            .map_err(net)?;
        let status = resp.status().as_u16();
        let text = resp.body_mut().read_to_string().unwrap_or_default();
        let reply: CsidReply = serde_json::from_str(&text).unwrap_or_default();
        match (status, reply.token.as_deref(), reply.secret.as_deref()) {
            (200, Some(token), Some(secret)) if !token.is_empty() && !secret.is_empty() => Ok(Csid {
                token: token.to_owned(),
                secret: secret.to_owned(),
                request_id: match &reply.request_id {
                    Some(serde_json::Value::String(s)) => s.clone(),
                    Some(v) => v.to_string(),
                    None => String::new(),
                },
            }),
            (200, _, _) => Err(format!(
                "ZATCA did not issue the CSID ({})",
                reply.disposition.as_deref().unwrap_or("no certificate in the answer")
            )),
            _ if reply.errors.is_none() && reply.error_message.is_none() && reply.message.is_none() && !text.trim().is_empty() && text.len() < 200 => {
                Err(format!("ZATCA refused the request ({status}): {}", text.trim()))
            }
            _ => Err(csid_error(status, &reply)),
        }
    }

    /// The compliance CSID for a CSR (its [`khata_sa::csr::Csr::api_value`])
    /// with the OTP from the Fatoora portal.
    pub fn compliance_csid(&self, otp: &str, csr_api_value: &str) -> ApiResult<Csid> {
        let otp = otp.trim();
        if otp.is_empty() || !otp.bytes().all(|b| b.is_ascii_digit()) {
            return Err("the OTP is the number shown on the Fatoora portal".into());
        }
        let req = self.agent.post(format!("{}/compliance", self.base)).header("OTP", otp);
        self.csid_call(req, serde_json::json!({ "csr": csr_api_value }))
    }

    /// The production CSID, after the compliance checks.
    pub fn production_csid(&self, compliance: &Csid) -> ApiResult<Csid> {
        let req = self.agent.post(format!("{}/production/csids", self.base)).header("Authorization", &compliance.basic());
        self.csid_call(req, serde_json::json!({ "compliance_request_id": compliance.request_id }))
    }

    fn invoice_call(&self, path: &str, csid: &Csid, clearance_status: Option<&str>, doc: Document<'_>) -> Result<Outcome, CallError> {
        let mut req = self
            .agent
            .post(format!("{}{path}", self.base))
            .header("Accept", "application/json")
            .header("Accept-Version", "V2")
            .header("Accept-Language", "en")
            .header("Authorization", &csid.basic());
        if let Some(c) = clearance_status {
            req = req.header("Clearance-Status", c);
        }
        let body = serde_json::json!({ "invoiceHash": doc.hash, "uuid": doc.uuid, "invoice": STANDARD.encode(doc.xml) });
        let mut resp = req.send_json(body).map_err(|e| CallError::new(CallFailure::Transient, net(e)))?;
        let status = resp.status().as_u16();
        let text = resp.body_mut().read_to_string().unwrap_or_default();
        match status {
            401 => {
                return Err(CallError::new(
                    CallFailure::Credentials,
                    "ZATCA refused the credentials (401): the CSID has expired or was revoked; renew it with a new OTP, or onboard the device again",
                ));
            }
            406 => return Err(CallError::new(CallFailure::Other, "ZATCA refused the API version (406)")),
            429 => return Err(CallError::new(CallFailure::Transient, "ZATCA is busy (429); the document stays queued and is sent again")),
            500..=599 => {
                return Err(CallError::new(
                    CallFailure::Transient,
                    format!("ZATCA had an internal error ({status}); the document stays queued and is sent again"),
                ));
            }
            _ => {}
        }
        let reply: InvoiceReply = serde_json::from_str(&text).unwrap_or_default();
        let v = reply.validation.unwrap_or_default();
        let cleared_xml = match reply.cleared_invoice.as_deref().filter(|c| !c.is_empty()) {
            Some(b64) => Some(
                STANDARD
                    .decode(b64)
                    .ok()
                    .and_then(|b| String::from_utf8(b).ok())
                    .ok_or_else(|| CallError::new(CallFailure::Other, "ZATCA's cleared invoice is not Base64 XML"))?,
            ),
            None => None,
        };
        let note = reply.message.or_else(|| {
            (v.status.is_none() && !text.trim().is_empty() && !text.trim_start().starts_with('{')).then(|| text.trim().chars().take(300).collect())
        });
        if v.status.is_none() && reply.reporting_status.is_none() && reply.clearance_status.is_none() && !(status == 303 || status == 409 || status == 208) {
            return Err(CallError::new(
                CallFailure::Other,
                format!("ZATCA answered {status} without validation results{}", note.map(|n| format!(": {n}")).unwrap_or_default()),
            ));
        }
        let mut errors = v.errors;
        if status == 409 && errors.is_empty() {
            errors.push(Message { message: Some("already reported".into()), ..Message::default() });
        }
        Ok(Outcome {
            http_status: status,
            status: v.status.unwrap_or_else(|| if (200..300).contains(&status) { "PASS".into() } else { "ERROR".into() }),
            reporting_status: reply.reporting_status,
            clearance_status: reply.clearance_status,
            info: v.info,
            warnings: v.warnings,
            errors,
            cleared_xml,
            note,
        })
    }

    /// A compliance check of one sample document (compliance CSID).
    pub fn compliance_check(&self, compliance: &Csid, doc: Document<'_>) -> Result<Outcome, CallError> {
        self.invoice_call("/compliance/invoices", compliance, None, doc)
    }

    /// Report a simplified document (production CSID).
    pub fn report(&self, production: &Csid, doc: Document<'_>) -> Result<Outcome, CallError> {
        self.invoice_call("/invoices/reporting/single", production, Some("0"), doc)
    }

    /// Clear a standard document (production CSID). A 303 answer means
    /// clearance is switched off: report it instead ([`Outcome::http_status`]).
    pub fn clear(&self, production: &Csid, doc: Document<'_>) -> Result<Outcome, CallError> {
        self.invoice_call("/invoices/clearance/single", production, Some("1"), doc)
    }

    /// Renew the production CSID before it expires: a new CSR (its
    /// [`khata_sa::csr::Csr::api_value`]) with a new OTP from the Fatoora
    /// portal, authenticated with the current production CSID. The answer
    /// is a new production CSID, like `/production/csids`.
    ///
    /// Source: the Developer Portal Manual v3 §2.3.10.4 "Production CSID
    /// (Renewal) API" (OTP and CSR, `Accept-Version: V2`, Basic
    /// authentication) [PRIMARY]; the verb `PATCH` on `/production/csids`
    /// and authenticating with the current production CSID come from
    /// ZATCA's developer community and SDKs [UNCERTAIN: the swagger was not
    /// read].
    pub fn renew_production_csid(&self, production: &Csid, otp: &str, csr_api_value: &str) -> ApiResult<Csid> {
        let otp = otp.trim();
        if otp.is_empty() || !otp.bytes().all(|b| b.is_ascii_digit()) {
            return Err("the OTP is the number shown on the Fatoora portal".into());
        }
        let req = self
            .agent
            .patch(format!("{}/production/csids", self.base))
            .header("OTP", otp)
            .header("Authorization", &production.basic());
        self.csid_call(req, serde_json::json!({ "csr": csr_api_value }))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn debug_hides_secrets_and_basic_auth_is_token_and_secret() {
        let c = Csid { token: "VE9LRU4=".into(), secret: "s3cr3t".into(), request_id: "42".into() };
        let d = format!("{c:?}");
        assert!(d.contains("42") && !d.contains("s3cr3t") && !d.contains("VE9LRU4"));
        assert_eq!(c.basic(), format!("Basic {}", STANDARD.encode("VE9LRU4=:s3cr3t")));
        assert_eq!(base_url(Environment::Sandbox), "https://gw-fatoora.zatca.gov.sa/e-invoicing/developer-portal");
        assert!(base_url(Environment::Production).ends_with("/core"));
    }

    #[test]
    fn messages_parse_in_every_shape() {
        let r: InvoiceReply = serde_json::from_str(
            r#"{"validationResults":{"infoMessages":{"type":"INFO","code":"XSD_ZATCA_VALID","status":"PASS"},"warningMessages":[],"errorMessages":null,"status":"PASS"},"reportingStatus":"REPORTED"}"#,
        )
        .unwrap();
        let v = r.validation.unwrap();
        assert_eq!((v.info.len(), v.warnings.len(), v.errors.len(), v.status.as_deref()), (1, 0, 0, Some("PASS")));
        assert!(Outcome { reporting_status: Some("REPORTED".into()), status: "WARNING".into(), http_status: 202, ..Outcome::default() }.accepted());
        assert!(!Outcome { reporting_status: Some("NOT_REPORTED".into()), status: "ERROR".into(), http_status: 400, ..Outcome::default() }.accepted());
        let e = csid_error(400, &serde_json::from_str(r#"{"errors":[{"code":"Invalid-OTP","message":"The provided OTP is invalid"}]}"#).unwrap());
        assert_eq!(e, "ZATCA refused the request (400): Invalid-OTP: The provided OTP is invalid");
    }

    #[test]
    fn failures_say_whether_to_retry() {
        // Nothing listens on port 9 of localhost: a transient network failure.
        let c = Client::with_base("http://127.0.0.1:9");
        let csid = Csid { token: "t".into(), secret: "s".into(), request_id: "1".into() };
        let e = c.report(&csid, Document { xml: "<x/>", hash: "h", uuid: "u" }).unwrap_err();
        assert!(e.transient(), "{e:?}");
        let msg: String = e.into();
        assert!(msg.contains("cannot reach ZATCA"));
        assert_eq!(CallError::new(CallFailure::Credentials, "x").kind, CallFailure::Credentials);
    }
}
