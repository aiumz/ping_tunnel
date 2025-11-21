use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::fs;
use std::path::Path;

pub fn load_cert() -> anyhow::Result<(CertificateDer<'static>, PrivateKeyDer<'static>)> {
    if !Path::new("./cert/cert.pem").exists() || !Path::new("./cert/key.pem").exists() {
        return Err(anyhow::anyhow!(
            "Certificate files not found. Please place cert.pem and key.pem in the cert/ directory."
        ));
    }

    println!("Loading certificate from cert/cert.pem and cert/key.pem");
    // 从文件加载证书
    let cert_pem = fs::read("./cert/cert.pem")?;
    let key_pem = fs::read("./cert/key.pem")?;
    // 解析 PEM 格式的证书
    let cert_der = rustls_pemfile::certs(&mut cert_pem.as_slice())
        .next()
        .ok_or_else(|| anyhow::anyhow!("No certificate found in cert/cert.pem"))??;
    let cert_der = CertificateDer::from(cert_der);

    // 解析 PEM 格式的私钥
    let key_der = rustls_pemfile::pkcs8_private_keys(&mut key_pem.as_slice())
        .next()
        .ok_or_else(|| anyhow::anyhow!("No private key found in cert/key.pem"))??;
    let key_der = PrivateKeyDer::Pkcs8(key_der.into());

    Ok((cert_der, key_der))
}
