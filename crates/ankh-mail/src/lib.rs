#![warn(missing_docs)]
//! Transactional mail rendering, delivery, and test capture.

mod error;

use std::{
    collections::HashMap,
    fs,
    future::Future,
    path::{Path, PathBuf},
    result,
    sync::{Arc, Mutex},
};

use chrono::{DateTime, SecondsFormat, Utc};
pub use error::Error;
use uuid::Uuid;

/// Shorthand for mail operation results.
pub type Result<T> = result::Result<T, Error>;

/// Function used by [`DevMailer`] to get the current timestamp.
pub type Clock = Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>;

/// Function used by [`DevMailer`] to create unique artifact identifiers.
pub type IdGenerator = Arc<dyn Fn() -> String + Send + Sync>;

/// Shared template names.
pub mod template {
    /// Email verification template.
    pub const EMAIL_VERIFICATION: &str = "email_verification";
    /// Password reset template.
    pub const PASSWORD_RESET: &str = "password_reset";
    /// Waitlist invite template.
    pub const WAITLIST_INVITE: &str = "waitlist_invite";
    /// Waitlist release template.
    pub const WAITLIST_RELEASE: &str = "waitlist_release";
    /// Organization invite template.
    pub const ORG_INVITE: &str = "org_invite";
}

/// Provider-agnostic email message.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Email {
    /// Recipient address.
    pub to: String,
    /// Sender address.
    pub from: String,
    /// Message subject.
    pub subject: String,
    /// Plain-text message body.
    pub text_body: String,
    /// Optional HTML message body.
    pub html_body: Option<String>,
}

/// Mail delivery backend.
pub trait Mailer: Send + Sync {
    /// Send an email message.
    fn send(&self, email: &Email) -> impl Future<Output = Result<()>> + Send;
}

/// Base URL used to construct absolute links in mail.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicBaseUrl(String);

impl PublicBaseUrl {
    /// Normalize and validate a public base URL.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let mut value = value.into();
        while value.ends_with('/') {
            value.pop();
        }
        if value.is_empty() {
            return Err(Error::InvalidBaseUrl);
        }
        Ok(Self(value))
    }

    /// Join a path onto the base URL, trimming leading slashes.
    pub fn join(&self, path: &str) -> String {
        let path = path.strip_prefix('/').unwrap_or(path);
        format!("{}/{path}", self.0)
    }

    /// Construct an action URL with a `token` query parameter.
    pub fn action_url(&self, path: &str, token: &str) -> String {
        format!("{}?token={token}", self.join(path))
    }
}

/// Product mail branding applied to shared templates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailBranding {
    /// Product name inserted into shared templates.
    pub app_name: String,
    /// Public base URL used for absolute links.
    pub public_base_url: PublicBaseUrl,
    /// Default sender address used as [`Email::from`].
    pub sender: String,
    /// Support address exposed in templates and shells.
    pub support_address: String,
    /// Optional HTML shell that receives `{html_body}` plus branding placeholders.
    pub html_shell: Option<String>,
}

impl MailBranding {
    /// Build mail branding without an HTML shell.
    pub fn new(
        app_name: impl Into<String>,
        public_base_url: PublicBaseUrl,
        sender: impl Into<String>,
        support_address: impl Into<String>,
    ) -> Self {
        Self {
            app_name: app_name.into(),
            public_base_url,
            sender: sender.into(),
            support_address: support_address.into(),
            html_shell: None,
        }
    }

    /// Return a copy of this branding with the supplied HTML shell.
    #[must_use]
    pub fn with_html_shell(mut self, html_shell: impl Into<String>) -> Self {
        self.html_shell = Some(html_shell.into());
        self
    }

    /// Construct an absolute action URL with a token query parameter.
    pub fn action_url(&self, path: &str, token: &str) -> String {
        self.public_base_url.action_url(path, token)
    }

    /// Construct an absolute URL for a path.
    pub fn link_url(&self, path: &str) -> String {
        self.public_base_url.join(path)
    }
}

/// Text and optional HTML source for a named mail template.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailTemplate {
    /// Plain-text template source. The first line must be `Subject: ...`,
    /// followed by `---` on its own line and then the text body.
    pub text: String,
    /// Optional HTML template source.
    pub html: Option<String>,
}

impl MailTemplate {
    /// Build a mail template from text and optional HTML source.
    pub fn new(text: impl Into<String>, html: Option<impl Into<String>>) -> Self {
        Self {
            text: text.into(),
            html: html.map(Into::into),
        }
    }
}

/// Shared transactional mail catalog with optional product overrides.
#[derive(Clone, Debug, Default)]
pub struct MailCatalog {
    /// Product-provided templates keyed by shared template name.
    overrides: HashMap<String, MailTemplate>,
}

impl MailCatalog {
    /// Build a shared catalog with no product overrides.
    #[must_use]
    pub fn shared() -> Self {
        Self::default()
    }

    /// Build a shared catalog with product template overrides.
    pub fn with_overrides(overrides: impl IntoIterator<Item = (String, MailTemplate)>) -> Self {
        Self {
            overrides: overrides.into_iter().collect(),
        }
    }

    /// Render the named template into an addressed email.
    pub fn render_email(
        &self,
        template_name: &str,
        to: &str,
        branding: &MailBranding,
        vars: &HashMap<String, String>,
    ) -> Result<Email> {
        let source = self.template_source(template_name)?;
        let (subject, text_body) = parse_text_template(template_name, source.text)?;
        let vars = branding_vars(branding, vars);
        let subject = substitute(template_name, "subject", &subject, &vars)?;
        let text_body = substitute(template_name, "text_body", &text_body, &vars)?;
        let html_body = source
            .html
            .map(|html| substitute(template_name, "html_body", html, &vars))
            .transpose()?
            .map(|html| apply_html_shell(template_name, html, branding, &vars))
            .transpose()?;

        Ok(Email {
            to: to.to_owned(),
            from: branding.sender.clone(),
            subject,
            text_body,
            html_body,
        })
    }

    /// Return the product override or embedded default template for `name`.
    fn template_source(&self, name: &str) -> Result<TemplateSource<'_>> {
        if let Some(template) = self.overrides.get(name) {
            return Ok(TemplateSource {
                text: template.text.as_str(),
                html: template.html.as_deref(),
            });
        }

        DEFAULT_TEMPLATES
            .iter()
            .find(|template| template.name == name)
            .map(|template| TemplateSource {
                text: template.text,
                html: Some(template.html),
            })
            .ok_or_else(|| Error::TemplateNotFound(name.to_owned()))
    }
}

/// Borrowed template source used while rendering.
struct TemplateSource<'a> {
    /// Plain-text template source.
    text: &'a str,
    /// Optional HTML template source.
    html: Option<&'a str>,
}

/// Embedded shared template source.
struct EmbeddedTemplate {
    /// Template name.
    name: &'static str,
    /// Plain-text source.
    text: &'static str,
    /// HTML source.
    html: &'static str,
}

/// Shared embedded templates keyed by name.
const DEFAULT_TEMPLATES: &[EmbeddedTemplate] = &[
    EmbeddedTemplate {
        name: template::EMAIL_VERIFICATION,
        text: include_str!("../templates/email_verification.txt"),
        html: include_str!("../templates/email_verification.html"),
    },
    EmbeddedTemplate {
        name: template::PASSWORD_RESET,
        text: include_str!("../templates/password_reset.txt"),
        html: include_str!("../templates/password_reset.html"),
    },
    EmbeddedTemplate {
        name: template::WAITLIST_INVITE,
        text: include_str!("../templates/waitlist_invite.txt"),
        html: include_str!("../templates/waitlist_invite.html"),
    },
    EmbeddedTemplate {
        name: template::WAITLIST_RELEASE,
        text: include_str!("../templates/waitlist_release.txt"),
        html: include_str!("../templates/waitlist_release.html"),
    },
    EmbeddedTemplate {
        name: template::ORG_INVITE,
        text: include_str!("../templates/org_invite.txt"),
        html: include_str!("../templates/org_invite.html"),
    },
];

/// Build the render variable map with branding values taking precedence.
fn branding_vars(
    branding: &MailBranding,
    vars: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut vars = vars.clone();
    vars.insert("app_name".to_owned(), branding.app_name.clone());
    vars.insert(
        "support_address".to_owned(),
        branding.support_address.clone(),
    );
    vars
}

/// Parse a plain-text template into `(subject, body)`.
fn parse_text_template(template_name: &str, text: &str) -> Result<(String, String)> {
    let normalized = text.replace("\r\n", "\n");
    let (raw_subject, raw_body) = normalized.split_once("\n---\n").ok_or_else(|| {
        Error::InvalidTemplate(format!("{template_name}: missing subject separator `---`"))
    })?;

    let subject = raw_subject
        .trim()
        .strip_prefix("Subject:")
        .ok_or_else(|| {
            Error::InvalidTemplate(format!(
                "{template_name}: first line must start with `Subject:`"
            ))
        })?
        .trim();

    if subject.is_empty() {
        return Err(Error::InvalidTemplate(format!(
            "{template_name}: subject cannot be empty"
        )));
    }

    Ok((subject.to_owned(), raw_body.to_owned()))
}

/// Apply `{placeholder}` substitutions, erroring if any are left unresolved.
fn substitute(
    template_name: &str,
    field: &str,
    template: &str,
    vars: &HashMap<String, String>,
) -> Result<String> {
    let mut rendered = template.to_owned();
    for (key, value) in vars {
        rendered = rendered.replace(format!("{{{key}}}").as_str(), value);
    }

    if let Some(placeholder) = find_placeholder(rendered.as_str()) {
        return Err(Error::InvalidTemplate(format!(
            "{template_name}: missing placeholder value for `{placeholder}` in {field}"
        )));
    }

    Ok(rendered)
}

/// Apply the branding HTML shell if one is configured.
fn apply_html_shell(
    template_name: &str,
    html_body: String,
    branding: &MailBranding,
    vars: &HashMap<String, String>,
) -> Result<String> {
    let Some(shell) = branding.html_shell.as_deref() else {
        return Ok(html_body);
    };

    let mut shell_vars = vars.clone();
    shell_vars.insert("html_body".to_owned(), html_body);
    substitute(template_name, "html_shell", shell, &shell_vars)
}

/// First `{name}` placeholder in `s`, where `name` is non-empty and contains
/// only ASCII alphanumerics or underscores. Literal `{...}` sequences
/// containing anything else, such as CSS rules, are ignored.
fn find_placeholder(s: &str) -> Option<String> {
    let mut rest = s;
    while let Some(open) = rest.find('{') {
        let after_open = &rest[open + 1..];
        let close = after_open.find('}')?;
        let candidate = &after_open[..close];
        if !candidate.is_empty()
            && candidate
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Some(format!("{{{candidate}}}"));
        }
        rest = &after_open[close + 1..];
    }
    None
}

/// Configuration for development mail artifacts.
#[derive(Clone)]
pub struct DevMailerConfig {
    /// Output directory for mail artifacts.
    pub out_dir: PathBuf,
    /// Clock used to timestamp artifacts.
    pub clock: Clock,
    /// Identifier generator used to keep artifact names unique.
    pub id_gen: IdGenerator,
}

impl DevMailerConfig {
    /// Build a dev mailer config using wall-clock time and UUID identifiers.
    pub fn new(out_dir: impl Into<PathBuf>) -> Self {
        Self {
            out_dir: out_dir.into(),
            clock: Arc::new(Utc::now),
            id_gen: Arc::new(|| Uuid::new_v4().to_string()),
        }
    }
}

/// Development mailer that persists messages to disk.
#[derive(Clone)]
pub struct DevMailer {
    /// Development mailer configuration.
    config: DevMailerConfig,
}

impl DevMailer {
    /// Create a dev mailer that writes messages into `out_dir`.
    pub fn new(out_dir: impl Into<PathBuf>) -> Self {
        Self::with_config(DevMailerConfig::new(out_dir))
    }

    /// Create a dev mailer with explicit clock and ID generator.
    pub fn with_config(config: DevMailerConfig) -> Self {
        Self { config }
    }

    /// Read the most recent email written by this mailer.
    pub fn read_latest(&self) -> Result<Option<Email>> {
        read_latest_dev_mail(&self.config.out_dir)
    }

    /// Read all emails written by this mailer, sorted by filename.
    pub fn read_all(&self) -> Result<Vec<Email>> {
        read_all_dev_mail(&self.config.out_dir)
    }
}

impl Mailer for DevMailer {
    async fn send(&self, email: &Email) -> Result<()> {
        fs::create_dir_all(&self.config.out_dir)?;

        let now = (self.config.clock)();
        let base = format!(
            "{}_{}",
            now.format("%Y%m%dT%H%M%SZ"),
            sanitize_artifact_id((self.config.id_gen)().as_str())
        );
        let date = now.to_rfc3339_opts(SecondsFormat::Secs, true);
        let text = format!(
            "To: {}\nFrom: {}\nSubject: {}\nDate: {}\n\n{}",
            email.to, email.from, email.subject, date, email.text_body
        );
        fs::write(self.config.out_dir.join(format!("{base}.txt")), text)?;
        if let Some(html_body) = email.html_body.as_ref() {
            fs::write(self.config.out_dir.join(format!("{base}.html")), html_body)?;
        }
        Ok(())
    }
}

/// Mailer that silently drops every message.
///
/// Used in production deployments that intentionally send no mail (e.g. a
/// waitlist launch where signups are collected but no outbound email is sent).
/// Sending always succeeds, so callers that treat a send failure as fatal keep
/// working without a real provider configured.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoopMailer;

impl NoopMailer {
    /// Create a no-op mailer.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Mailer for NoopMailer {
    async fn send(&self, _email: &Email) -> Result<()> {
        Ok(())
    }
}

/// Sanitize generated IDs before they become part of a filename.
fn sanitize_artifact_id(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Read the most recent development email from `out_dir`.
///
/// Standalone reader for callers that do not hold a [`DevMailer`] (e.g. inspecting
/// artifacts written by another process); mirrors [`DevMailer::read_latest`].
pub fn read_latest_dev_mail(out_dir: &Path) -> Result<Option<Email>> {
    let mut paths = list_txt_files(out_dir)?;
    paths.pop().map(|path| read_email(&path)).transpose()
}

/// Read all development emails from `out_dir`, sorted by filename.
///
/// Standalone reader for callers that do not hold a [`DevMailer`]; mirrors
/// [`DevMailer::read_all`].
pub fn read_all_dev_mail(out_dir: &Path) -> Result<Vec<Email>> {
    list_txt_files(out_dir)?
        .iter()
        .map(|path| read_email(path))
        .collect()
}

/// Return all `.txt` mail artifacts in `out_dir`, sorted by filename.
fn list_txt_files(out_dir: &Path) -> Result<Vec<PathBuf>> {
    if !out_dir.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(out_dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|ext| ext == "txt") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

/// Read an `Email` from a `.txt` artifact, pulling in sibling HTML when present.
fn read_email(txt_path: &Path) -> Result<Email> {
    let mut email = parse_dev_mail_text(fs::read_to_string(txt_path)?.as_str())?;
    let html_path = txt_path.with_extension("html");
    if html_path.exists() {
        email.html_body = Some(fs::read_to_string(html_path)?);
    }
    Ok(email)
}

/// Parse the `.txt` dev mail format back into an `Email` struct.
fn parse_dev_mail_text(contents: &str) -> Result<Email> {
    let normalized = contents.replace("\r\n", "\n");
    let (headers, text_body) = normalized
        .split_once("\n\n")
        .ok_or_else(|| Error::InvalidDevMail("missing header/body separator".to_owned()))?;

    let mut to = None;
    let mut from = None;
    let mut subject = None;
    for line in headers.lines() {
        if let Some(value) = line.strip_prefix("To: ") {
            to = Some(value.to_owned());
        } else if let Some(value) = line.strip_prefix("From: ") {
            from = Some(value.to_owned());
        } else if let Some(value) = line.strip_prefix("Subject: ") {
            subject = Some(value.to_owned());
        }
    }

    Ok(Email {
        to: to.ok_or_else(|| Error::InvalidDevMail("missing To header".to_owned()))?,
        from: from.ok_or_else(|| Error::InvalidDevMail("missing From header".to_owned()))?,
        subject: subject
            .ok_or_else(|| Error::InvalidDevMail("missing Subject header".to_owned()))?,
        text_body: text_body.to_owned(),
        html_body: None,
    })
}

/// Cloneable in-memory sink for mail assertions in tests.
#[derive(Debug, Clone, Default)]
pub struct RecordingMailer {
    /// Recorded emails protected for cloneable test access.
    sent: Arc<Mutex<Vec<Email>>>,
}

impl RecordingMailer {
    /// Create an empty recording mailer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an email in memory.
    pub fn record(&self, email: Email) {
        let mut sent = self.sent.lock().expect("recording mailer mutex poisoned");
        sent.push(email);
    }

    /// Return and clear every recorded email.
    #[must_use]
    pub fn take_sent(&self) -> Vec<Email> {
        let mut sent = self.sent.lock().expect("recording mailer mutex poisoned");
        sent.drain(..).collect()
    }
}

impl Mailer for RecordingMailer {
    async fn send(&self, email: &Email) -> Result<()> {
        self.record(email.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! Tests for shared transactional mail.

    use std::{
        collections::HashMap,
        fs,
        future::Future,
        path::PathBuf,
        pin::pin,
        sync::Arc,
        task::{Context, Poll, Waker},
    };

    use chrono::{TimeZone, Utc};

    use super::{
        DevMailer, DevMailerConfig, Email, MailBranding, MailCatalog, MailTemplate, Mailer,
        PublicBaseUrl, RecordingMailer, template,
    };

    /// Run a future that is expected not to park.
    fn block_ready<T>(future: impl Future<Output = T>) -> T {
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        let mut future = pin!(future);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("mail future unexpectedly pending"),
        }
    }

    /// Shared branding used by most tests.
    fn branding() -> MailBranding {
        MailBranding::new(
            "Example App",
            PublicBaseUrl::new("http://example.test/").expect("valid base url"),
            "no-reply@example.test",
            "support@example.test",
        )
    }

    /// Render every shared template with representative variables.
    #[test]
    fn shared_catalog_renders_every_template() {
        let catalog = MailCatalog::shared();
        let branding = branding();

        let action_vars = HashMap::from([(
            "action_url".to_owned(),
            branding.action_url("/verify-email", "verify-token"),
        )]);
        let verification = catalog
            .render_email(
                template::EMAIL_VERIFICATION,
                "alice@example.test",
                &branding,
                &action_vars,
            )
            .expect("render verification");
        assert_eq!(verification.from, "no-reply@example.test");
        assert!(verification.text_body.contains("Example App"));
        assert!(verification.text_body.contains("verify-token"));

        let reset_vars = HashMap::from([(
            "action_url".to_owned(),
            branding.action_url("/reset-password", "reset-token"),
        )]);
        let reset = catalog
            .render_email(
                template::PASSWORD_RESET,
                "alice@example.test",
                &branding,
                &reset_vars,
            )
            .expect("render reset");
        assert!(reset.text_body.contains("reset-token"));

        let invite_vars = HashMap::from([(
            "invite_url".to_owned(),
            format!("{}?invite=invite-token", branding.link_url("/signup")),
        )]);
        let invite = catalog
            .render_email(
                template::WAITLIST_INVITE,
                "alice@example.test",
                &branding,
                &invite_vars,
            )
            .expect("render waitlist invite");
        assert!(invite.subject.contains("Example App"));
        assert!(invite.text_body.contains("invite-token"));

        let release_vars = HashMap::from([("login_url".to_owned(), branding.link_url("/login"))]);
        let release = catalog
            .render_email(
                template::WAITLIST_RELEASE,
                "alice@example.test",
                &branding,
                &release_vars,
            )
            .expect("render release");
        assert!(release.text_body.contains("support@example.test"));

        let org_vars = HashMap::from([
            ("org_name".to_owned(), "Acme Corp".to_owned()),
            (
                "invite_url".to_owned(),
                format!("{}?org_invite=org-token", branding.link_url("/signup")),
            ),
        ]);
        let org_invite = catalog
            .render_email(
                template::ORG_INVITE,
                "alice@example.test",
                &branding,
                &org_vars,
            )
            .expect("render org invite");
        assert!(org_invite.subject.contains("Acme Corp"));
        assert!(org_invite.text_body.contains("Example App"));
        assert!(
            org_invite
                .html_body
                .as_ref()
                .expect("html")
                .contains("Acme Corp")
        );
    }

    /// Missing placeholders fail loudly.
    #[test]
    fn missing_placeholder_fails() {
        let err = MailCatalog::shared()
            .render_email(
                template::EMAIL_VERIFICATION,
                "alice@example.test",
                &branding(),
                &HashMap::new(),
            )
            .expect_err("missing placeholder");
        assert!(err.to_string().contains("missing placeholder"));
    }

    /// Product overrides take precedence over embedded templates.
    #[test]
    fn product_overrides_take_precedence() {
        let catalog = MailCatalog::with_overrides([(
            template::WAITLIST_INVITE.to_owned(),
            MailTemplate::new(
                "Subject: Custom {app_name}\n---\nCustom body: {custom}",
                Option::<String>::None,
            ),
        )]);
        let vars = HashMap::from([("custom".to_owned(), "yes".to_owned())]);
        let email = catalog
            .render_email(
                template::WAITLIST_INVITE,
                "alice@example.test",
                &branding(),
                &vars,
            )
            .expect("render override");
        assert_eq!(email.subject, "Custom Example App");
        assert_eq!(email.text_body, "Custom body: yes");
        assert_eq!(email.html_body, None);
    }

    /// Base URL helpers normalize slashes and render action tokens.
    #[test]
    fn base_url_helpers_render_links() {
        let base = PublicBaseUrl::new("http://example.test///").expect("valid base url");
        assert_eq!(base.join("/login"), "http://example.test/login");
        assert_eq!(
            base.action_url("/verify-email", "abc"),
            "http://example.test/verify-email?token=abc"
        );
    }

    /// Sender and HTML shell come from branding.
    #[test]
    fn branding_renders_sender_and_html_shell() {
        let branding = branding().with_html_shell("<main>{app_name}:{html_body}</main>");
        let vars = HashMap::from([(
            "action_url".to_owned(),
            branding.action_url("/reset-password", "abc"),
        )]);
        let email = MailCatalog::shared()
            .render_email(
                template::PASSWORD_RESET,
                "alice@example.test",
                &branding,
                &vars,
            )
            .expect("render shell");
        assert_eq!(email.from, "no-reply@example.test");
        assert!(
            email
                .html_body
                .as_ref()
                .expect("html")
                .starts_with("<main>Example App:")
        );
    }

    /// Recording mailer captures sent mail and drains with `take_sent`.
    #[test]
    fn recording_mailer_captures_email() {
        let mailer = RecordingMailer::new();
        let email = Email {
            to: "alice@example.test".to_owned(),
            from: "no-reply@example.test".to_owned(),
            subject: "Welcome".to_owned(),
            text_body: "Hello".to_owned(),
            html_body: None,
        };

        block_ready(mailer.send(&email)).expect("send mail");
        assert_eq!(mailer.take_sent(), vec![email]);
        assert!(mailer.take_sent().is_empty());
    }

    /// Dev mailer writes readable artifacts and reads them back.
    #[test]
    fn dev_mailer_writes_and_reads_artifacts() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let tmp = root.join("tmp");
        fs::create_dir_all(&tmp).expect("create workspace tmp");
        let dir = tempfile::tempdir_in(&tmp).expect("temp dir in workspace tmp");
        let fixed = Utc
            .with_ymd_and_hms(2026, 6, 18, 1, 2, 3)
            .single()
            .expect("valid timestamp");
        let mailer = DevMailer::with_config(DevMailerConfig {
            out_dir: dir.path().to_path_buf(),
            clock: Arc::new(move || fixed),
            id_gen: Arc::new(|| "fixed/id".to_owned()),
        });
        let email = Email {
            to: "alice@example.test".to_owned(),
            from: "no-reply@example.test".to_owned(),
            subject: "Hello".to_owned(),
            text_body: "Text body".to_owned(),
            html_body: Some("<p>HTML</p>".to_owned()),
        };

        block_ready(mailer.send(&email)).expect("send mail");
        assert!(dir.path().join("20260618T010203Z_fixed_id.txt").exists());
        assert!(dir.path().join("20260618T010203Z_fixed_id.html").exists());

        let emails = mailer.read_all().expect("read all");
        assert_eq!(emails, vec![email.clone()]);
        assert_eq!(mailer.read_latest().expect("read latest"), Some(email));
    }
}
