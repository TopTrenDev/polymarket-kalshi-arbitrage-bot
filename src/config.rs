const KALSHI_BASE_PROD: &str = "https://api.elections.kalshi.com/trade-api/v2";
const KALSHI_BASE_DEMO: &str = "https://demo-api.kalshi.co/trade-api/v2";
const PEM_HEADER: &str = "-----BEGIN RSA PRIVATE KEY-----";
const PEM_FOOTER: &str = "-----END RSA PRIVATE KEY-----";

fn normalize_pem(value: &str) -> String {
    let trimmed = value.trim();
    let base64: String = trimmed
        .replace(PEM_HEADER, "")
        .replace(PEM_FOOTER, "")
        .split_whitespace()
        .collect();
    if base64.is_empty() {
        return trimmed.to_string();
    }
    let lines: Vec<String> = base64
        .as_bytes()
        .chunks(64)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect();
    format!("{}\n{}\n{}", PEM_HEADER, lines.join("\n"), PEM_FOOTER)
}

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.trim().is_empty())
}

#[derive(Clone)]
pub struct KalshiConfig {
    pub api_id: String,
    pub rsa_private_key: String,
    pub base_url: String,
    pub demo: bool,
    pub dry_run: bool,
}

impl KalshiConfig {
    pub fn from_env() -> Self {
        let api_id = env("KALSHI_API_ID")
            .or_else(|| env("KALSHI_API_KEY"))
            .unwrap_or_default();
        let demo = env("KALSHI_DEMO").map(|s| s.eq_ignore_ascii_case("true")).unwrap_or(false);
        let base_url = env("KALSHI_BASE_PATH")
            .unwrap_or_else(|| {
                if demo {
                    KALSHI_BASE_DEMO.to_string()
                } else {
                    KALSHI_BASE_PROD.to_string()
                }
            });
        let dry_run = env("DRY_RUN")
            .or_else(|| env("KALSHI_DRY_RUN"))
            .map(|s| s.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let rsa_private_key = load_rsa_private_key();
        Self {
            api_id,
            rsa_private_key,
            base_url,
            demo,
            dry_run,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn is_dry_run(&self) -> bool {
        self.dry_run
    }
}

fn load_rsa_private_key() -> String {
    if let Some(path) = env("KALSHI_PRIVATE_KEY_PATH") {
        if let Ok(content) = std::fs::read_to_string(&path) {
            return normalize_pem(&content);
        }
    }
    let raw = env("KALSHI_RSA_PRIVATE_KEY").or_else(|| env("KALSHI_PRIVATE_KEY_PEM"));
    raw.map(|s| normalize_pem(&s)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_pem() {
        let with_whitespace = "  -----BEGIN RSA PRIVATE KEY-----\n  aGVsbG8=  \n  -----END RSA PRIVATE KEY-----  ";
        let out = normalize_pem(with_whitespace);
        assert!(out.contains(PEM_HEADER));
        assert!(out.contains(PEM_FOOTER));
        assert!(out.contains("aGVsbG8="));
    }
}
