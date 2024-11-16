use constants::ENVS_TO_GET;

pub struct EnvScanner;

impl EnvScanner {
    pub fn scan_env() -> std::collections::HashMap<String, String> {
        let mut envs = std::collections::HashMap::with_capacity(ENVS_TO_GET.len());
        for env in ENVS_TO_GET {
            if let Ok(val) = std::env::var(env) {
                envs.insert(env.to_string(), val);
            }
        }
        envs
    }
}
