use anyhow::Result;
use certify::{CA, CertSigAlgo, CertType, generate_ca, generate_cert};
use tokio::fs;

struct CertPem {
    cert_type: CertType,
    cert: String,
    key: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let pem = create_ca()?;
    gen_files(&pem).await?;
    let ca = CA::load(&pem.cert, &pem.key)?;
    let pem = create_cert(&ca, &["kvserver.acme.inc"], "Acme KV server", false)?;
    gen_files(&pem).await?;
    let pem = create_cert(&ca, &[], "awesome-device-id", true)?;
    gen_files(&pem).await?;
    Ok(())
}

fn create_ca() -> Result<CertPem> {
    let (cert, key) = generate_ca(
        "CN",
        "acme.inc",
        "Acme Inc.",
        CertSigAlgo::ED25519,
        None,
        Some(10 * 365),
    )?;
    Ok(CertPem {
        cert_type: CertType::CA,
        cert,
        key,
    })
}

fn create_cert(ca: &CA, domains: &[&str], cn: &str, is_client: bool) -> Result<CertPem> {
    let (days, cert_type) = if is_client {
        (Some(365), CertType::Client)
    } else {
        (Some(5 * 365), CertType::Server)
    };
    let (cert, key) = generate_cert(
        ca,
        domains.iter().map(|s| s.to_string()).collect(),
        "CN",
        "Acme Inc.",
        cn,
        CertSigAlgo::ED25519,
        None,
        is_client,
        days,
    )?;

    Ok(CertPem {
        cert_type,
        cert,
        key,
    })
}

async fn gen_files(pem: &CertPem) -> Result<()> {
    let name = match pem.cert_type {
        CertType::Client => "client",
        CertType::Server => "server",
        CertType::CA => "ca",
    };
    fs::write(format!("fixtures/{}.cert", name), pem.cert.as_bytes()).await?;
    fs::write(format!("fixtures/{}.key", name), pem.key.as_bytes()).await?;
    Ok(())
}
