#[derive(Debug)]
pub struct TextOutput {
    pub id: String,
    pub content: String,
    pub status: OutputStatus,
}

#[derive(Debug)]
pub enum OutputStatus {
    Draft,
    Committed,
    Canceled,
}
