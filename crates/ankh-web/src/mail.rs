//! Mail state used by shared Ankh web services.

use std::{collections::HashMap, sync::Arc};

use ankh_mail::{Email, MailBranding, MailCatalog, Mailer};
use async_trait::async_trait;

use crate::api::{ApiError, ApiResult};

/// Object-safe mail transport used by web state.
#[async_trait]
pub trait MailTransport: Send + Sync {
    /// Send an email message.
    async fn send(&self, email: &Email) -> ankh_mail::Result<()>;
}

#[async_trait]
impl<T> MailTransport for T
where
    T: Mailer + Send + Sync,
{
    async fn send(&self, email: &Email) -> ankh_mail::Result<()> {
        Mailer::send(self, email).await
    }
}

/// Mail renderer and transport for shared Ankh services.
#[derive(Clone)]
pub struct MailState {
    /// Shared mail delivery backend.
    transport: Arc<dyn MailTransport>,
    /// Shared mail template catalog.
    catalog: MailCatalog,
    /// Product branding used in generated mail.
    branding: MailBranding,
}

impl MailState {
    /// Build mail state from a concrete mailer, catalog, and product branding.
    #[must_use]
    pub fn new<T>(transport: T, catalog: MailCatalog, branding: MailBranding) -> Self
    where
        T: MailTransport + 'static,
    {
        Self {
            transport: Arc::new(transport),
            catalog,
            branding,
        }
    }

    /// Build mail state from an already shared transport.
    #[must_use]
    pub fn from_transport(
        transport: Arc<dyn MailTransport>,
        catalog: MailCatalog,
        branding: MailBranding,
    ) -> Self {
        Self {
            transport,
            catalog,
            branding,
        }
    }

    /// Render a named template into an email.
    pub fn render_email(
        &self,
        template_name: &str,
        recipient: &str,
        vars: &HashMap<String, String>,
    ) -> ApiResult<Email> {
        self.catalog
            .render_email(template_name, recipient, &self.branding, vars)
            .map_err(|err| ApiError::internal(err.to_string()))
    }

    /// Construct an absolute URL for a path.
    #[must_use]
    pub fn link_url(&self, path: &str) -> String {
        self.branding.link_url(path)
    }

    /// Construct an absolute action URL with a token query parameter.
    #[must_use]
    pub fn action_url(&self, path: &str, token: &str) -> String {
        self.branding.action_url(path, token)
    }

    /// Send a rendered email.
    pub async fn send(&self, email: &Email) -> ApiResult<()> {
        self.transport
            .send(email)
            .await
            .map_err(|err| ApiError::internal(err.to_string()))
    }
}
