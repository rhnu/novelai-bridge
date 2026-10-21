use std::path::Path;

use novelai_bridge::{ApiKeySource, ReqwestOptions};

use crate::TaskResult;

pub fn options() -> TaskResult<ReqwestOptions> {
    let mut options = ReqwestOptions::default();
    if std::env::var_os("NOVELAI_API_KEY").is_some() {
        return Ok(options);
    }
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.env");
    let contents = std::fs::read_to_string(&file)?;
    for line in contents.lines() {
        let line = line.trim().trim_start_matches("export ").trim();
        if let Some((key, value)) = line.split_once('=')
            && key.trim() == "NOVELAI_API_KEY"
        {
            options.api_key_source = ApiKeySource::Inline {
                value: value.trim().trim_matches(['\"', '\'']).to_owned().into(),
            };
            return Ok(options);
        }
    }
    Err(format!("NOVELAI_API_KEY is missing from {}", file.display()).into())
}
