#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PlayerDecision {
    Hit,
    Stand,
    Double,
}
