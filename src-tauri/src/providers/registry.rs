use super::{
    bailian_cli::BailianCliProvider, deepseek::DeepSeekProvider, plans::PlanProvider, Provider,
};
use crate::domain::models::ProviderDescriptor;
pub fn get_provider(id: &str) -> Option<Box<dyn Provider>> {
    match id {
        "codex_http" => Some(Box::new(super::codex_http::CodexHttpProvider)),
        "codex" => Some(Box::new(super::codex::CodexProvider)),
        "claude_code" => Some(Box::new(super::claude::ClaudeProvider)),
        "deepseek_api" => Some(Box::new(DeepSeekProvider)),
        "minimax_api" => Some(Box::new(PlanProvider("minimax_api"))),
        "minimax_token_plan" => Some(Box::new(PlanProvider("minimax_token_plan"))),
        "bailian_token_plan" => Some(Box::new(BailianCliProvider)),
        "bailian_api" => Some(Box::new(PlanProvider("bailian_api"))),
        _ => None,
    }
}
pub fn descriptors() -> Vec<ProviderDescriptor> {
    stillenough_core::providers::descriptors()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn desktop_registry_matches_shared_catalog() {
        let entries = descriptors();
        assert_eq!(
            entries.len(),
            stillenough_core::providers::PROVIDER_IDS.len()
        );
        for descriptor in entries {
            let provider =
                get_provider(descriptor.id).expect("catalog entry has a desktop adapter");
            assert_eq!(
                serde_json::to_value(provider.descriptor()).unwrap(),
                serde_json::to_value(descriptor).unwrap()
            );
        }
    }
}
