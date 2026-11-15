#[derive(Debug, Clone)]
pub struct IshikawaNode {
    pub text: String,
    pub children: Vec<usize>,
}
