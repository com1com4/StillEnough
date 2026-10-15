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
    let mut entries = vec![DeepSeekProvider.descriptor()];
    entries.extend(
        ["minimax_api", "minimax_token_plan", "bailian_api"]
            .map(|id| PlanProvider(id).descriptor()),
    );
    entries.push(BailianCliProvider.descriptor());
    entries.push(super::codex_http::CodexHttpProvider.descriptor());
    entries.push(super::codex::CodexProvider.descriptor());
    entries.push(super::claude::ClaudeProvider.descriptor());
    entries
}
