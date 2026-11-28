use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

// Keep trust-store policy independent of the selected TLS implementation.
pub(crate) fn download_tls_config() -> TlsConfig {
    let rustls = cfg!(feature = "tls-rustls-no-provider");
    let provider = if rustls { TlsProvider::Rustls } else { TlsProvider::NativeTls };
    let roots = if rustls && !cfg!(feature = "tls-rustls-platform") {
        RootCerts::WebPki
    } else {
        RootCerts::PlatformVerifier
    };
    TlsConfig::builder().provider(provider).root_certs(roots).build()
}
