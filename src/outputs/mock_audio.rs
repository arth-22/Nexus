#[derive(Debug)]
pub struct AudioOutput {
    pub id: String,
    pub duration_ms: u64,
    pub status: OutputStatus,
}

#[derive(Debug)]
pub enum OutputStatus {
    Draft,
    Committed,
    Canceled,
}
