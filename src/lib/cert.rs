use rustls::ClientConfig as RustlsClientConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::fs;
use std::path::Path;
use std::sync::Arc;

pub fn install_default_crypto_provider() {
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Failed to install default crypto provider");
}

pub fn load_cert(
    cert_path: String,
    key_path: String,
) -> anyhow::Result<(CertificateDer<'static>, PrivateKeyDer<'static>)> {
    install_default_crypto_provider();
    if !Path::new(&cert_path).exists() || !Path::new(&key_path).exists() {
        return Err(anyhow::anyhow!(
            "Certificate files not found. Please place {} and {} in the cert/ directory.",
            cert_path,
            key_path
        ));
    }
    if !Path::new(&cert_path).exists() || !Path::new(&key_path).exists() {
        return Err(anyhow::anyhow!(
            "Certificate files not found. Please place {} and {} in the cert/ directory.",
            cert_path,
            key_path
        ));
    }

    println!("Loading certificate from {} and {}", cert_path, key_path);
    // 从文件加载证书
    let cert_pem = fs::read(&cert_path)?;
    let key_pem = fs::read(&key_path)?;
    // 解析 PEM 格式的证书
    let cert_der = rustls_pemfile::certs(&mut cert_pem.as_slice())
        .next()
        .ok_or_else(|| anyhow::anyhow!("No certificate found in {}", cert_path))??;
    let cert_der = CertificateDer::from(cert_der);

    // 解析 PEM 格式的私钥
    let key_der = rustls_pemfile::pkcs8_private_keys(&mut key_pem.as_slice())
        .next()
        .ok_or_else(|| anyhow::anyhow!("No private key found in {}", key_path))??;
    let key_der = PrivateKeyDer::Pkcs8(key_der.into());

    Ok((cert_der, key_der))
}

#[derive(Debug)]
pub struct NoCertificateVerification;

impl rustls::client::danger::ServerCertVerifier for NoCertificateVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA512,
            rustls::SignatureScheme::ED25519,
            rustls::SignatureScheme::ED448,
        ]
    }
}

pub fn get_client_crypto_config() -> anyhow::Result<quinn::crypto::rustls::QuicClientConfig> {
    install_default_crypto_provider();
    let mut client_config = RustlsClientConfig::builder()
        .with_root_certificates(rustls::RootCertStore::empty())
        .with_no_client_auth();

    client_config
        .dangerous()
        .set_certificate_verifier(Arc::new(NoCertificateVerification));

    let quic_client_config = match quinn::crypto::rustls::QuicClientConfig::try_from(client_config)
    {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Failed to create QUIC client config: {}", e);
            return Err(anyhow::anyhow!(
                "Failed to create QUIC client config: {}",
                e
            ));
        }
    };
    Ok(quic_client_config)
}
