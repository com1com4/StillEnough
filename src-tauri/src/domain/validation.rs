use super::models::AccountInput;
pub fn validate_account(input: &AccountInput) -> Result<(), String> {
    if input.alias.trim().is_empty() || input.alias.len() > 240 {
        return Err("请填写有效的账号别名".into());
    }
    crate::providers::registry::get_provider(&input.provider_id)
        .ok_or("该产品尚未支持读取")?
        .validate_url(&input.base_url)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn front_end_cannot_submit_metrics() {
        let payload = r#"{"id":null,"providerId":"deepseek_api","alias":"test","baseUrl":"https://api.deepseek.com","enabled":false,"metrics":[]}"#;
        assert!(serde_json::from_str::<AccountInput>(payload).is_err());
    }
}
