use anyhow::Result;

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub listen_addr: String,
    pub escrow_validator_script_hash: String,
    pub escrow_validator_addr: String,
    pub oracle_endpoint: String,
    pub points_per_success: i64,
    pub otp_default_ttl_seconds: i64,
    pub otp_default_max_uses: i64,
    pub collateral_base_lovelace: i64,
    pub collateral_bps: i64,
    pub collateral_max_lovelace: i64,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            database_url: env_or(
                "DATABASE_URL",
                "sqlite://./thirdman.db?mode=rwc",
            ),
            listen_addr: env_or("LISTEN_ADDR", "127.0.0.1:8080"),
            escrow_validator_script_hash: env_or(
                "ESCROW_VALIDATOR_SCRIPT_HASH",
                "00".repeat(28),
            ),
            escrow_validator_addr: env_or(
                "ESCROW_VALIDATOR_ADDR",
                "addr_test1w000000000000000000000000000000000000000000000000000",
            ),
            oracle_endpoint: env_or("ORACLE_ENDPOINT", "https://oracle.example.com/query"),
            points_per_success: env_or_parse("POINTS_PER_SUCCESS", 10),
            otp_default_ttl_seconds: env_or_parse("OTP_DEFAULT_TTL_SECONDS", 3600),
            otp_default_max_uses: env_or_parse("OTP_DEFAULT_MAX_USES", 1),
            collateral_base_lovelace: env_or_parse("COLLATERAL_BASE_LOVELACE", 2_000_000),
            collateral_bps: env_or_parse("COLLATERAL_BPS", 500),
            collateral_max_lovelace: env_or_parse("COLLATERAL_MAX_LOVELACE", 20_000_000),
        })
    }
}

fn env_or(key: &str, default: impl Into<String>) -> String {
    std::env::var(key).unwrap_or_else(|_| default.into())
}
fn env_or_parse<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
