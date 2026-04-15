use crate::doc::{classify_document, ClassifiedLine};
use crate::eval::{evaluate_document, DocumentResult};

pub struct AcordDoc {
    pub text: String,
    pub uuid: String,
    lines: Vec<ClassifiedLine>,
}

impl AcordDoc {
    pub fn new() -> Self {
        AcordDoc {
            text: String::new(),
            uuid: uuid::Uuid::new_v4().to_string(),
            lines: Vec::new(),
        }
    }

    pub fn with_uuid(uuid: String) -> Self {
        AcordDoc {
            text: String::new(),
            uuid,
            lines: Vec::new(),
        }
    }

    pub fn set_text(&mut self, text: &str) {
        self.text = text.to_string();
        self.lines = classify_document(text);
    }

    pub fn classified_lines(&self) -> &[ClassifiedLine] {
        &self.lines
    }

    pub fn evaluate(&self) -> DocumentResult {
        evaluate_document(&self.text)
    }
}
