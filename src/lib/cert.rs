use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::fs;
use std::path::Path;

pub fn load_cert(
    cert_path: String,
    key_path: String,
) -> anyhow::Result<(CertificateDer<'static>, PrivateKeyDer<'static>)> {
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Failed to install default crypto provider");

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
